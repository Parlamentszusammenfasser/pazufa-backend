use super::KeyIndex;
use crate::db::gen_identifier;
use crate::error::DataValidationError;
use pazufa_models::{self, Zusammenfassungstupel};
use crate::{PazufaServer, Result};
use sqlx::PgTransaction;
use uuid::Uuid;

/// updates and cycles the records of which scraper touched which vorgang
/// # Errors
/// Can fail due to database errors
pub async fn update_scraper_touched_vorgang(
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
) -> Result<()> {
    for l in lre {
        let aid = super::insert_or_retrieve_autor(&l.organisation, tx).await?;
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
/// inserts the schlagworte of a vorgang object
/// # Errors
/// Can fail due to database errors
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

/// Inserts zusammenfassung tuples for Vorgang
/// # Errors
/// Can fail due to database errors
#[tracing::instrument]
pub async fn insert_zusammenf_for_vorgang(
    vg_id: i32,
    tuples: &[Zusammenfassungstupel],
    tx: &mut PgTransaction<'_>,
) -> Result<()> {
    let (mut types, mut datas) = (vec![], vec![]);
    for t in tuples {
        types.push(t.typ.clone());
        datas.push(t.inhalt.clone());
    }
    sqlx::query!(
        "INSERT INTO zusammenf_typ(value)
        SELECT * FROM UNNEST($1::text[])
        ON CONFLICT DO NOTHING",
        &types[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO vg_zusammenfassung(vg_id, zf_typ, value) 

        SELECT $1, tp.id, data 
        FROM    UNNEST($2::text[], $3::text[]) as i(data, typename)
        JOIN zusammenf_typ tp ON tp.value = typename

        ON CONFLICT (vg_id, zf_typ) DO UPDATE SET
        value = EXCLUDED.value",
        vg_id,
        &datas[..],
        &types[..]
    )
    .execute(&mut **tx)
    .await?;
    tracing::trace!("done");
    Ok(())
}
/// Inserts a new Vorgang into the database.
/// # Errors
/// Can fail due to track validation or database errors
pub async fn insert(
    vg: &models::Vorgang,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    server: &PazufaServer,
) -> Result<i32> {
    tracing::info!("Inserting Complete Vorgang into the database");
    #[cfg(feature = "track-validation")]
    server.tracks.validate(vg)?;
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
        i32::from(vg.wahlperiode),
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
        init_ids.push(super::insert_or_retrieve_autor(x, tx).await?);
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
        "INSERT INTO vg_ident_typ(value)
        SELECT * FROM UNNEST($1::text[])
        ON CONFLICT DO NOTHING",
        &identtyp_list[..]
    )
    .execute(&mut **tx)
    .await?;
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
    insert_lobbyregister_for_vg(vg_id, &vg.lobbyregister, tx).await?;

    // insert summaries
    insert_zusammenf_for_vorgang(vg_id, &vg.zusammenfassung, tx).await?;

    tracing::info!("Vorgang Insertion Successful with ID: {}", vg_id);
    Ok(vg_id)
}

/// Inserts a station into the database, returning its db id
/// # Errors
/// Can fail due to database erorrs
pub async fn insert_station(
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
    let gr_id = super::insert_or_retrieve_gremium(&stat.gremium, tx).await?;
    let stat_id = sqlx::query!(
        "INSERT INTO station 
        (api_id, gr_id, link, titel, trojanergefahr, typ, 
        zp_start, vg_id, zp_modifiziert, gremium_isff, tiebreak_ordering)
        VALUES
        ($1, $2, $3, $4, $5,
        (SELECT id FROM stationstyp WHERE value = $6), $7, $8, 
        COALESCE($9, NOW()), $10, $11)
        RETURNING station.id",
        sapi,
        gr_id,
        stat.link,
        stat.titel,
        stat.trojanergefahr.map(i32::from),
        srv.guard_ts(&stat.typ, sapi, obj),
        stat.zp_start,
        vg_id,
        stat.zp_modifiziert,
        stat.gremium_federf,
        stat.meta_tiebreaker.map(i32::from)
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
        did.push(
            super::dokument::insert_or_retrieve_dok(dokument, scraper_id, collector_key, tx, srv)
                .await?,
        );
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
        doks.push(
            super::dokument::insert_or_retrieve_dok(stln, scraper_id, collector_key, tx, srv)
                .await?,
        );
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

#[cfg(test)]
mod test {
    use crate::{
        api::RoundTimestamp,
        models,
        utils::testing::{TestSetup, generate::default_vorgang},
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn issue204_non_standard_idtyp_not_swallowed() {
        let scenario = TestSetup::new("issue204_allbutvgnr_dropped").await;
        let vorgang = models::Vorgang {
            ids: vec![models::VgIdent {
                id: "beispielid".to_owned(),
                typ: "beispieltyp".to_owned(),
            }],
            ..default_vorgang()
        };
        let mut tx = scenario.server.sqlx_db.begin().await.unwrap();
        let inserted = super::insert(&vorgang, Uuid::nil(), 1, &mut tx, &scenario.server)
            .await
            .unwrap();
        let out_vorgang = crate::db::retrieve::vorgang::vorgang_by_id(inserted, true, &mut tx)
            .await
            .unwrap();
        assert_eq!(
            vorgang.with_round_timestamps(),
            out_vorgang.with_round_timestamps()
        );
    }
}
