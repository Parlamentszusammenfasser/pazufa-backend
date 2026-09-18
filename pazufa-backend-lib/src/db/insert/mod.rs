pub mod dokument;
pub mod sitzung;
pub mod vorgang;

use super::KeyIndex;
use crate::error::PazufaError;
use crate::{Result, models};
pub use dokument::{insert_dokument, insert_or_retrieve_dok, insert_schlagworte_of_dok};
pub use sitzung::insert_sitzung;
use sqlx::PgTransaction;
pub use vorgang::{
    insert as insert_vorgang, insert_schlagworte_of_vorgang, insert_station,
    update_scraper_touched_vorgang,
};

pub(crate) async fn insert_or_retrieve_gremium(
    gr: &models::Gremium,
    tx: &mut PgTransaction<'_>,
) -> Result<i32> {
    let gid: Option<i32> = sqlx::query_scalar!(
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
    tracing::info!(
        target = "obj",
        "New Gremium; Similarity vector: {similarity:?}"
    );
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

    tracing::info!(
        target = "obj",
        "New Autor; Similarity vector: {similarity:?}"
    );
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
