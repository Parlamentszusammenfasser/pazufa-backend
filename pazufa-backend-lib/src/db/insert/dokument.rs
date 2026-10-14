use super::KeyIndex;
use crate::db::gen_identifier;
use crate::db::merge::MatchState;
use crate::db::merge::candidates::dokument_merge_candidates;
use crate::models::enumerations::{HashStrategy, Mime};
use crate::models::{self};
use crate::{PazufaServer, Result};
use sqlx::PgTransaction;
use uuid::Uuid;

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
/// Inserts a document into the database, returning its db id
/// # Errors
/// Can fail due to database errors
#[tracing::instrument(skip_all)]
pub async fn insert_dokument(
    dok: &models::Dokument,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut sqlx::PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    let dapi = dok.api_id.unwrap_or(gen_identifier(scraper_id));
    match dokument_merge_candidates(dok, &mut **tx, srv).await? {
        MatchState::ExactlyOne(id) => return Ok(id),
        MatchState::Ambiguous(matches) => {
            let api_ids = sqlx::query!(
                "SELECT api_id FROM dokument WHERE id = ANY($1::int4[])",
                &matches[..]
            )
            .map(|r| r.api_id)
            .fetch_all(&mut **tx)
            .await?;
            tracing::warn!("Ambiguous Document match: {api_ids:?}\n====> all match with\n{dok:?}");
        }
        MatchState::NoMatch => {}
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
        aids.push(super::insert_or_retrieve_autor(a, tx).await?);
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

/// Inserts a document into the database or retrieves it by api id, returning its db id
/// # Errors
/// Can fail due to database errors
pub async fn insert_or_retrieve_dok(
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

/// Inserts schlagwort vector of a document into the database
/// # Errors
/// Can fail due to database errors
pub async fn insert_schlagworte_of_dok(
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
