use super::KeyIndex;
use crate::db::gen_identifier;
use crate::models;
use crate::{PazufaServer, Result};
use sqlx::PgTransaction;
use uuid::Uuid;

/// Inserts a full Sitzung object into the database as new object
/// # Errors
/// Can fail due to database errors or if the session number exeeds `i32::MAX`
pub async fn insert_sitzung(
    ass: &models::Sitzung,
    scraper_id: Uuid,
    collector_key: KeyIndex,
    tx: &mut PgTransaction<'_>,
    srv: &PazufaServer,
) -> Result<i32> {
    let api_id = ass.api_id.unwrap_or(gen_identifier(scraper_id));

    // gremium insert or fetch
    let gr_id = super::insert_or_retrieve_gremium(&ass.gremium, tx).await?;
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
        i32::from(ass.nummer),
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
        let ex_id = super::insert_or_retrieve_autor(exp, tx).await?;
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
            let id =
                super::dokument::insert_dokument(d, scraper_id, collector_key, tx, srv).await?;
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
        i32::from(top.nummer),
        sid
    )
    .map(|r| r.id)
    .fetch_one(&mut **tx)
    .await?;

    // drucksachen
    let mut dids = vec![];
    for d in &top.dokumente {
        dids.push(
            super::dokument::insert_or_retrieve_dok(d, scraper_id, collector_key, tx, srv).await?,
        );
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
