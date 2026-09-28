use super::KeyIndex;
use crate::db::gen_identifier;
use crate::db::merge::candidates::dokument_merge_candidates;
#[cfg(feature = "track-validation")]
use crate::db::validate;
use crate::error::{DataValidationError, PazufaError};
use crate::models;
use crate::models::enumerations::{HashStrategy, Mime};
use crate::{
    PazufaServer, Result,
    utils::{self, notify::notify_new_enum_entry},
};
use sqlx::PgTransaction;
use uuid::Uuid;

pub(crate) async fn update_scraper_touched_vorgang(
    vg_id: i32,
    rotation_size: u32,
    collector_key: KeyIndex,
    scraper_id: Uuid,
    tx: &mut PgTransaction<'_>,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO scraper_touched_vorgang(vg_id, collector_key, scraper) 
        VALUES ($1, $2, $3) 
        ON CONFLICT(vg_id, scraper) DO UPDATE SET time_stamp=NOW()",
        vg_id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT vg_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY vg_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_vorgang
        )
        DELETE FROM scraper_touched_vorgang stv
        USING ranked_objects ro
        WHERE stv.vg_id=ro.vg_id AND
        stv.scraper=ro.scraper AND
        ro.rn > $1",
        i64::from(rotation_size)
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn insert_stations_for_vg(
    id: i32,
    stationen: &Vec<models::Station>,

    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    server: &PazufaServer,
) -> Result<()> {
    let mut stat_ids = vec![];
    for stat in stationen {
        stat_ids.push(insert_station(stat, id, scraper_id, collector_key, tx, server).await?);
    }
    tracing::debug!("station IDs to be inserted: {stat_ids:?}");
    sqlx::query!(
        "INSERT INTO scraper_touched_station(stat_id, collector_key, scraper) 
        SELECT sid, $2, $3 FROM UNNEST($1::int4[]) as sid 
        ON CONFLICT(stat_id, scraper) 
        DO UPDATE SET time_stamp=NOW()",
        &stat_ids[..],
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT stat_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY stat_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_station
        )
        DELETE FROM scraper_touched_station st
        USING ranked_objects ro
        WHERE st.stat_id=ro.stat_id AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        i64::from(server.config.per_object_scraper_log_size)
    )
    .execute(&mut **tx)
    .await?;

    tracing::trace!("Inserted stations for {id}: {stat_ids:?}");
    Ok(())
}
async fn insert_lobbyregister_for_vg(
    vg_id: i32,
    lre: &Vec<models::Lobbyregistereintrag>,
    tx: &mut sqlx::PgTransaction<'_>,
    server: &PazufaServer,
) -> Result<()> {
    for l in lre {
        let aid = insert_or_retrieve_autor(&l.organisation, tx, server).await?;
        let lrid = sqlx::query!(
            "INSERT INTO lobbyregistereintrag(intention, interne_id, organisation, vg_id, link)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id",
            &l.intention,
            &l.interne_id,
            &aid,
            vg_id,
            &l.link
        )
        .map(|r| r.id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query!(
            "INSERT INTO rel_lobbyreg_drucksnr(drucksnr, lob_id) 
            SELECT x, $1 FROM UNNEST($2::text[]) as x(x)",
            lrid,
            &l.betroffene_drucksachen
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
pub async fn insert_schlagworte_of_vorgang(
    vg_id: i32,
    sw: &[String],
    tx: &mut PgTransaction<'_>,
) -> Result<()> {
    let sw: Vec<_> = sw.iter().map(|s| s.trim().to_lowercase()).collect();
    sqlx::query!(
        "WITH 
    existing_ids AS (SELECT DISTINCT id FROM schlagwort WHERE value = ANY($1::text[])),
    inserted AS (
        INSERT INTO schlagwort(value) 
        SELECT DISTINCT(key) FROM UNNEST($1::text[]) as key
        ON CONFLICT DO NOTHING
        RETURNING id
    ),
    allofthem AS(
        SELECT id FROM inserted 
        UNION 
        SELECT id FROM existing_ids
    )

    INSERT INTO rel_vorgang_schlagwort(vg_id, sw_id)
    SELECT $2, allofthem.id FROM allofthem
    ON CONFLICT DO NOTHING",
        &sw[..],
        vg_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
/// Inserts a new Vorgang into the database.
pub(crate) async fn insert_vorgang(
    vg: &models::Vorgang,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    server: &PazufaServer,
) -> Result<i32> {
    tracing::info!("Inserting Complete Vorgang into the database");
    #[cfg(feature = "track-validation")]
    validate::validate_vorgang(vg, server).await?;
    tracing::trace!("Validation successful");

    let obj = "vorgang";
    // master insert
    let vg_id = sqlx::query!(
        "INSERT INTO vorgang(api_id, titel, kurztitel, verfaend, wahlperiode, typ, ressort)
    VALUES
    ($1, $2, $3, $4, $5, (SELECT id FROM vorgangstyp WHERE value=$6), (SELECT id from ressort WHERE value = $7))
    RETURNING vorgang.id;",
        vg.api_id,
        vg.titel,
        vg.kurztitel,
        vg.verfassungsaendernd,
        i32::try_from(vg.wahlperiode).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert vorgang wahlperiode as it exceeds i32::MAX: {e}")
        })?,
        server.guard_ts(&vg.typ, vg.api_id, obj),
        vg.ressort.map(|x| x.to_string())
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;
    update_scraper_touched_vorgang(
        vg_id,
        server.config.per_object_scraper_log_size,
        collector_key,
        scraper_id,
        tx,
    )
    .await?;
    // insert schlagworte
    insert_schlagworte_of_vorgang(vg_id, &vg.schlagworte, tx).await?;
    // insert sachgebiete
    let sg_numbers: Vec<_> = vg.sachgebiete.iter().map(|x| i32::from(*x)).collect();
    sqlx::query!(
        "INSERT INTO vg_sachgebiete(vg_id, sg_id)
        SELECT $1, s.id FROM UNNEST($2::int4[]) as i, sachgebiet s
        WHERE i = s.value
        ",
        vg_id,
        &sg_numbers[..]
    )
    .execute(&mut **tx)
    .await?;

    // insert links
    sqlx::query!(
        "INSERT INTO rel_vorgang_links(link, vg_id) SELECT val, $2 FROM UNNEST($1::text[]) as val",
        &vg.links[..],
        vg_id
    )
    .execute(&mut **tx)
    .await?;

    // insert initiatoren
    let mut init_ids = vec![];
    for x in &vg.initiatoren {
        init_ids.push(insert_or_retrieve_autor(x, tx, server).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_vorgang_init(in_id, vg_id) SELECT val, $2 FROM UNNEST($1::int4[])as val;",
        &init_ids[..],
        vg_id
    )
    .execute(&mut **tx)
    .await?;

    // insert ids
    let (mut ident_list, mut identtyp_list) = (vec![], vec![]);
    for id in &vg.ids {
        ident_list.push(id.id.clone());
        identtyp_list.push(id.typ.clone());
    }

    sqlx::query!(
        "INSERT INTO rel_vorgang_ident (vg_id, typ, identifikator) 
    SELECT $1, t.id, ident.ident FROM 
    UNNEST($2::text[], $3::text[]) as ident(ident, typ)
    INNER JOIN vg_ident_typ t ON t.value = ident.typ
    ON CONFLICT DO NOTHING",
        vg_id,
        &ident_list[..],
        &identtyp_list[..]
    )
    .execute(&mut **tx)
    .await?;

    // insert stations
    insert_stations_for_vg(vg_id, &vg.stationen, scraper_id, collector_key, tx, server).await?;

    // insert Lobbyregister
    insert_lobbyregister_for_vg(vg_id, &vg.lobbyregister, tx, server).await?;

    tracing::info!("Vorgang Insertion Successful with ID: {}", vg_id);
    Ok(vg_id)
}

pub(crate) async fn insert_station(
    stat: &models::Station,
    vg_id: i32,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    // master insert
    let sapi = stat.api_id.unwrap_or(gen_identifier(scraper_id));
    let obj = "station";
    if sqlx::query!("SELECT 1 as id FROM station WHERE api_id = $1", sapi)
        .fetch_optional(&mut **tx)
        .await?
        .is_some()
    {
        return Err(DataValidationError::DuplicateApiId {
            id: sapi,
            object: obj.to_owned(),
        }
        .into());
    }
    let gr_id = insert_or_retrieve_gremium(&stat.gremium, tx, srv).await?;
    let stat_id = sqlx::query!(
        "INSERT INTO station 
        (api_id, gr_id, link, titel, trojanergefahr, typ, 
        zp_start, vg_id, zp_modifiziert, gremium_isff)
        VALUES
        ($1, $2, $3, $4, $5,
        (SELECT id FROM stationstyp WHERE value = $6), $7, $8, 
        COALESCE($9, NOW()), $10)
        RETURNING station.id",
        sapi,
        gr_id,
        stat.link,
        stat.titel,
        stat.trojanergefahr.map(|x| i32::from(x)),
        srv.guard_ts(&stat.typ, sapi, obj),
        stat.zp_start,
        vg_id,
        stat.zp_modifiziert,
        stat.gremium_federf
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;

    // links
    sqlx::query!(
        "INSERT INTO rel_station_link(stat_id, link)
        SELECT $1, blub FROM UNNEST($2::text[]) as blub ON CONFLICT DO NOTHING",
        stat_id,
        &stat.additional_links[..]
    )
    .execute(&mut **tx)
    .await?;

    // assoziierte dokumente
    let mut did = vec![];
    for dokument in &stat.dokumente {
        did.push(insert_or_retrieve_dok(dokument, scraper_id, collector_key, tx, srv).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_station_dokument(stat_id, dok_id) 
    SELECT $1, blub FROM UNNEST($2::int4[]) as blub ON CONFLICT DO NOTHING",
        stat_id,
        &did[..]
    )
    .execute(&mut **tx)
    .await?;

    // stellungnahmen
    let mut doks = Vec::with_capacity(stat.stellungnahmen.len());
    for stln in &stat.stellungnahmen {
        doks.push(insert_or_retrieve_dok(stln, scraper_id, collector_key, tx, srv).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_station_stln (stat_id, dok_id)
        SELECT $1, did FROM UNNEST($2::int4[]) as did ON CONFLICT DO NOTHING",
        stat_id,
        &doks[..]
    )
    .execute(&mut **tx)
    .await?;
    // schlagworte, inserted on a vorgang level, see #54 for details
    insert_schlagworte_of_vorgang(vg_id, &stat.schlagworte, tx).await?;

    Ok(stat_id)
}
async fn insert_hashes_of_dok(
    did: i32,
    hash: &models::HashWrapper,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<()> {
    let mut v_hashstrats: Vec<String> = vec![];
    let mut v_hashvalues: Vec<String> = vec![];
    let mut v_hash_mimes: Vec<String> = vec![];
    match hash {
        models::HashWrapper::Legacy(h) => {
            v_hashstrats.push(HashStrategy::Sha256Bytes.to_string());
            v_hashvalues.push(h.clone());
            v_hash_mimes.push(Mime::ApplicationPdf.to_string());
        }
        models::HashWrapper::Future(hvec) => {
            for h in hvec {
                v_hashvalues.push(h.value.clone());
                v_hashstrats.push(h.strategy.to_string());
                v_hash_mimes.push(h.mime.to_string());
            }
        }
    }
    sqlx::query!(
        "INSERT INTO dok_hashes(dok_id, hash_strat_id, value, mime_id) 
        SELECT $1, hs.id, inpt.value, m.id 
        FROM
            UNNEST($2::text[], $3::text[], $4::text[]) as inpt(strat, value, mime),
            hash_strategy hs, 
            mime as m
        WHERE 
            hs.value = inpt.strat AND
            m.value = inpt.mime
        ",
        did,
        &v_hashstrats[..],
        &v_hashvalues[..],
        &v_hash_mimes[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
async fn insert_zusammenfassung_of_dok(
    did: i32,
    zusammenfassung: &models::ZusammenfassungWrapper,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<()> {
    let mut v_zftypes: Vec<String> = vec![];
    let mut v_zfvalue: Vec<String> = vec![];
    match zusammenfassung {
        models::ZusammenfassungWrapper::Legacy(h) => {
            v_zftypes.push("full".to_string());
            v_zfvalue.push(h.clone());
        }
        models::ZusammenfassungWrapper::Future(hvec) => {
            for h in hvec {
                v_zftypes.push(h.typ.clone());
                v_zfvalue.push(h.inhalt.clone());
            }
        }
    }
    sqlx::query!(
        "INSERT INTO zusammenf_typ (value) 
            SELECT * FROM UNNEST($1::text[]) ON CONFLICT (value) DO NOTHING",
        &v_zftypes[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO dok_zusammenfassungen(dok_id, zf_typ, value) 
        SELECT $1, zft.id, inpt.value FROM
            zusammenf_typ zft, UNNEST($2::text[], $3::text[]) as inpt(strat, value)
            WHERE zft.value = inpt.strat
        ",
        did,
        &v_zftypes[..],
        &v_zfvalue[..]
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
pub(crate) async fn insert_dokument(
    dok: &models::Dokument,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    let dapi = dok.api_id.unwrap_or(gen_identifier(scraper_id));
    match dokument_merge_candidates(dok, &mut **tx, srv).await? {
        super::merge::MatchState::ExactlyOne(id) => return Ok(id),
        super::merge::MatchState::Ambiguous(matches) => {
            let api_ids = sqlx::query!(
                "SELECT api_id FROM dokument WHERE id = ANY($1::int4[])",
                &matches[..]
            )
            .map(|r| r.api_id)
            .fetch_all(&mut **tx)
            .await?;
            utils::notify::notify_ambiguous_match(&api_ids, &dok, "insert_dokument", srv)?;
        }
        super::merge::MatchState::NoMatch => {}
    }
    let obj = "Dokument";
    let did = sqlx::query!(
        "INSERT INTO dokument(api_id, drucksnr, typ, titel, kurztitel, vorwort, 
        volltext, zp_lastmod, link, zp_referenz, zp_created, meinung, subdoc_id)
        VALUES(
            $1,$2, (SELECT id FROM dokumententyp WHERE value = $3),
            $4,$5,$6,$7,$8,$9,$10,$11, $12, $13
        )RETURNING id",
        dapi,
        dok.drucksnr,
        srv.guard_ts(&dok.typ, dapi, obj),
        dok.titel,
        dok.kurztitel,
        dok.vorwort,
        dok.volltext,
        dok.zp_modifiziert,
        dok.link,
        dok.zp_referenz,
        dok.zp_erstellt,
        dok.meinung.map(i32::from),
        dok.subdoc_id.map(i32::from)
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;
    // hash
    insert_hashes_of_dok(did, &dok.hash, tx).await?;
    // zusammenfassung
    if let Some(zsf) = &dok.zusammenfassung {
        insert_zusammenfassung_of_dok(did, zsf, tx).await?;
    }
    // Schlagworte
    insert_schlagworte_of_dok(did, &dok.schlagworte, tx).await?;

    // authoren
    let mut aids = vec![];
    for a in &dok.autoren {
        aids.push(insert_or_retrieve_autor(a, tx, srv).await?);
    }
    sqlx::query!(
        "INSERT INTO rel_dok_autor(dok_id, aut_id) 
    SELECT $1, blub FROM UNNEST($2::int4[]) as blub ON CONFLICT DO NOTHING",
        did,
        &aids[..]
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        "INSERT INTO scraper_touched_dokument(dok_id, collector_key, scraper) 
    VALUES ($1, $2, $3) 
    ON CONFLICT(dok_id, scraper) DO UPDATE SET time_stamp=NOW()",
        did,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT dok_id, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY dok_id
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_dokument
        )
        DELETE FROM scraper_touched_dokument st
        USING ranked_objects ro
        WHERE st.dok_id=ro.dok_id AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        i64::from(srv.config.per_object_scraper_log_size)
    )
    .execute(&mut **tx)
    .await?;
    Ok(did)
}

pub(crate) async fn insert_sitzung(
    ass: &models::Sitzung,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    let api_id = ass.api_id.unwrap_or(gen_identifier(scraper_id));

    // gremium insert or fetch
    let gr_id = insert_or_retrieve_gremium(&ass.gremium, tx, srv).await?;
    // master insert
    let id = sqlx::query!(
        "INSERT INTO sitzung 
        (api_id, termin, public, gr_id, link, nummer, titel)
        VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        api_id,
        ass.termin,
        ass.public,
        gr_id,
        ass.link,
        i32::try_from(ass.nummer).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert sitzung number as it exceeds i32::MAX: {e}")
        })?,
        ass.titel
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;
    // insert tops
    for top in &ass.tops {
        insert_top(id, top, scraper_id, collector_key, tx, srv).await?;
    }

    // insert experten
    let mut exp_ids = vec![];
    for exp in &ass.experten {
        let ex_id = insert_or_retrieve_autor(exp, tx, srv).await?;
        exp_ids.push(ex_id);
    }
    sqlx::query!(
        "INSERT INTO rel_sitzung_experten(sid, eid)
    SELECT $1, eids FROM UNNEST($2::int4[]) as eids",
        id,
        &exp_ids[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO scraper_touched_sitzung (sid, collector_key, scraper) VALUES ($1, $2, $3) ON CONFLICT(sid, scraper) 
        DO UPDATE SET time_stamp=NOW()",
        id,
        collector_key,
        scraper_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "WITH ranked_objects AS (
        SELECT sid, scraper, 
        ROW_NUMBER() OVER (
            PARTITION BY sid
            ORDER BY time_stamp DESC
        ) AS rn 
        FROM scraper_touched_sitzung
        )
        DELETE FROM scraper_touched_sitzung st
        USING ranked_objects ro
        WHERE st.sid=ro.sid AND
        st.scraper=ro.scraper AND
        ro.rn > $1",
        i64::from(srv.config.per_object_scraper_log_size)
    )
    .execute(&mut **tx)
    .await?;

    // insert documents
    let mut dok_ids = vec![];
    for d in &ass.dokumente {
        if let models::DokumentOrApiId::Dokument(d) = d {
            let id = insert_dokument(d, scraper_id, collector_key, tx, srv).await?;
            dok_ids.push(id);
        }
    }
    sqlx::query!(
        "INSERT INTO rel_sitzung_doks(sid, did)
                SELECT $1, dokid from UNNEST($2::int4[]) as dokid",
        id,
        &dok_ids[..]
    )
    .execute(&mut **tx)
    .await?;
    tracing::info!(
        "Neue Sitzung angelegt am {} im Parlament {}",
        ass.termin,
        ass.gremium.parlament
    );
    Ok(id)
}

pub(crate) async fn insert_top(
    sid: i32,
    top: &models::Top,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    // master insert
    let tid = sqlx::query!(
        "INSERT INTO top(titel, nummer, sid) VALUES($1, $2, $3) RETURNING id;",
        top.titel,
        i32::try_from(top.nummer).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert top number as it exceeds i32::MAX: {e}")
        })?,
        sid
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;

    // drucksachen
    let mut dids = vec![];
    for d in &top.dokumente {
        dids.push(insert_or_retrieve_dok(d, scraper_id, collector_key, tx, srv).await?);
    }
    sqlx::query!(
        "INSERT INTO tops_doks(top_id, dok_id)
        SELECT $1, did FROM UNNEST($2::int4[]) as did",
        tid,
        &dids[..]
    )
    .execute(&mut **tx)
    .await?;

    Ok(tid)
}

pub(crate) async fn insert_or_retrieve_gremium(
    gr: &models::Gremium,
    tx: &mut PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    let gid = sqlx::query!(
        "SELECT g.id FROM gremium g, parlament p WHERE
        g.name = $1 AND 
        p.id = g.parl AND  p.value = $2
        AND g.wp = $3",
        gr.name,
        gr.parlament.to_string(),
        i32::try_from(gr.wahlperiode).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert gremium wahlperiode as it exceeds i32::MAX: {e}"),
        })?,
    )
    .map(|r| r.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(ogid) = gid {
        return Ok(ogid);
    }

    let similarity = sqlx::query!(
        r#"SELECT g.wp,g.name, SIMILARITY(name, $1) as "sim!", g.link
        FROM gremium g, parlament p
        WHERE SIMILARITY(name, $1) > 0.66 AND 
        g.parl = p.id AND p.value = $2"#,
        gr.name,
        gr.parlament.to_string()
    )
    .map(|r| {
        Ok((
            r.sim,
            models::Gremium {
                link: r.link,
                parlament: gr.parlament,
                wahlperiode: u16::try_from(r.wp).map_err(|e| PazufaError::Other {
                    message: format!(
                        "Failed to convert wahlperiode to u32 as the value is negative: {e}"
                    ),
                })?,
                name: r.name,
            },
        ))
    })
    .fetch_all(&mut **tx)
    .await?
    .drain(..)
    .collect::<Result<Vec<_>>>()?;
    notify_new_enum_entry(gr, &similarity, srv);
    let id = sqlx::query!(
        "INSERT INTO gremium(name, parl, wp, link) VALUES 
        ($1, (SELECT id FROM parlament p WHERE p.value = $2), $3, $4) 
        RETURNING gremium.id",
        gr.name,
        gr.parlament.to_string(),
        i32::try_from(gr.wahlperiode).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert gremium wahlperiode as it exceeds i32::MAX: {e}")
        })?,
        gr.link
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

pub(crate) async fn insert_or_retrieve_autor(
    at: &models::Autor,
    tx: &mut PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    let eid = sqlx::query!(
        "SELECT a.id FROM autor a WHERE 
        ((a.person IS NULL AND $1::text IS NULL) OR a.person = $1) AND 
        ((a.organisation IS NULL AND $2::text IS NULL) OR a.organisation = $2) AND 
        ((a.fachgebiet IS NULL AND $3::text IS NULL) OR a.fachgebiet = $3)",
        at.person,
        at.organisation,
        at.fachgebiet
    )
    .map(|r| r.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(eid) = eid {
        return Ok(eid);
    }

    let similarity = sqlx::query!(
        r#"
        WITH similarities AS (
            SELECT id, 
            SIMILARITY(person, $1) as p, 
            SIMILARITY(organisation, $2) as o, 
            SIMILARITY(fachgebiet, $3) as f
            FROM autor a
        )
        SELECT a.*, 
        CASE WHEN s.p IS NOT NULL THEN s.p
        ELSE s.o END AS "sim!"
        
        FROM autor a 
        INNER JOIN similarities s ON s.id = a.id
        
        WHERE 
        
        (($1 IS NULL AND a.person IS NULL) OR s.p > 0.66) AND 
        s.o > 0.66 AND
        (($3 IS NULL AND a.fachgebiet IS NULL) OR s.f > 0.66)"#,
        at.person,
        at.organisation,
        at.fachgebiet
    )
    .map(|r| {
        (
            r.sim,
            models::Autor {
                fachgebiet: r.fachgebiet,
                person: r.person,
                organisation: r.organisation,
                lobbyregister: r.lobbyregister,
            },
        )
    })
    .fetch_all(&mut **tx)
    .await?;
    notify_new_enum_entry(at, &similarity, srv);
    let id = sqlx::query!(
        "INSERT INTO autor(person, organisation, lobbyregister, fachgebiet) 
        VALUES ($1, $2, $3, $4) RETURNING autor.id",
        at.person,
        at.organisation,
        at.lobbyregister,
        at.fachgebiet,
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

pub(crate) async fn insert_or_retrieve_dok(
    dr: &models::DokumentOrApiId,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    match dr {
        models::DokumentOrApiId::Dokument(dok) => {
            Ok(insert_dokument(dok, scraper_id, collector_key, tx, srv).await?)
        }
        models::DokumentOrApiId::ApiId(dapi_id) => Ok(sqlx::query!(
            "SELECT id FROM dokument WHERE api_id = $1",
            dapi_id
        )
        .map(|r| r.id)
        .fetch_one(&mut **tx)
        .await?),
    }
}

pub(crate) async fn insert_schlagworte_of_dok(
    did: i32,
    sw: &[String],
    tx: &mut PgTransaction<'_>,
) -> Result<()> {
    let sw: Vec<_> = sw.iter().map(|s| s.trim().to_lowercase()).collect();
    sqlx::query!(
        "
    WITH 
    existing_ids AS (SELECT DISTINCT id FROM schlagwort WHERE value = ANY($1::text[])),
    inserted AS (
        INSERT INTO schlagwort(value) 
        SELECT DISTINCT(key) FROM UNNEST($1::text[]) as key
        ON CONFLICT DO NOTHING
        RETURNING id
    ),
    allofthem AS(
        SELECT id FROM inserted 
        UNION 
        SELECT id FROM existing_ids
    )

    INSERT INTO rel_dok_schlagwort(dok_id, sw_id)
    SELECT $2, allofthem.id FROM allofthem
    ON CONFLICT DO NOTHING",
        &sw[..],
        did
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
