use crate::api::PaginationResponsePart;
use crate::error::{DataValidationError, PazufaError, Result};
use crate::models;
use crate::models::dokument::{
    DokumentHash, HashWrapper, ZusammenfassungWrapper, Zusammenfassungstupel,
};
use crate::models::enumerations::{HashStrategy, Mime};
use std::str::FromStr;
use uuid::Uuid;

/// Retrieves Hashes as Legacy/Future from the database
///
/// # Errors
/// Can fail due to IO Errors or failed database constraints
/// does NOT fail if an empty list of hashes was retrieved. This indicates a corrupted database
/// state, but is not critical for the integrity of the system itself. That is why this fails on
/// with a warning.
pub async fn hashes_of_dok(id: i32, executor: &mut sqlx::PgTransaction<'_>) -> Result<HashWrapper> {
    let recs: Vec<_> = sqlx::query!(
        "
    SELECT dh.value as hash, hs.value as strat, m.value as mime
    FROM
        dok_hashes dh,
        hash_strategy hs,
        mime m
    WHERE
        hs.id = dh.hash_strat_id AND
        dh.dok_id = $1 AND
        dh.mime_id = m.id
        ",
        id
    )
    .fetch_all(&mut **executor)
    .await?;
    if recs.is_empty() {
        tracing::warn!(
            "Found Corrupted Database State: Document without hash was supplied at some point"
        );
        return Ok(HashWrapper::Future(vec![]));
    } else if recs.len() == 1
        && recs[0].strat == "sha256+bytes"
        && recs[0].mime == "application/pdf"
    {
        return Ok(HashWrapper::Legacy(recs[0].hash.clone()));
    }
    let mut tuples = vec![];
    for r in recs {
        let strat = HashStrategy::from_str(&r.strat)
            .map_err(|e| crate::error::DataValidationError::InvalidEnumValue { msg: e.clone() })?;
        let mime = Mime::from_str(&r.mime).map_err(|e| PazufaError::Other {
            message: format!(
                "Error querying mime from the database: 
                Unable to convert {} into Mime Enumeration: {e}",
                r.mime
            ),
        })?;
        tuples.push(DokumentHash {
            strategy: strat,
            value: r.hash,
            mime,
        });
    }
    Ok(HashWrapper::Future(tuples))
}
/// Retrieves Zusammenfassungen as Legacy/Future from the database
///
/// # Errors
/// Can fail due to IO Errors or failed database constraints
pub async fn zusammenfassungen_of_dok(
    id: i32,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<Option<ZusammenfassungWrapper>> {
    let recs: Vec<_> = sqlx::query!(
        "
    SELECT zf.value as inhalt, zft.value as typ 
    FROM
        zusammenf_typ zft,
        dok_zusammenfassungen zf
    WHERE
        zft.id = zf.zf_typ AND
        zf.dok_id = $1
        ",
        id
    )
    .fetch_all(&mut **executor)
    .await?;
    if recs.is_empty() {
        return Ok(None);
    } else if recs.len() == 1 && recs[0].typ == "full" {
        return Ok(Some(ZusammenfassungWrapper::Legacy(recs[0].inhalt.clone())));
    }
    let mut tuples = vec![];
    for r in recs {
        tuples.push(Zusammenfassungstupel {
            inhalt: r.inhalt,
            typ: r.typ,
        });
    }
    Ok(Some(ZusammenfassungWrapper::Future(tuples)))
}

pub async fn dokument_dbid(
    api_id: Uuid,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<Option<i32>> {
    let id = sqlx::query_scalar!("SELECT id FROM dokument WHERE api_id = $1", api_id)
        .fetch_optional(&mut **executor)
        .await?;
    Ok(id)
}
pub async fn dokument_by_id_ims(
    id: i32,
    if_mod_since: Option<chrono::DateTime<chrono::Utc>>,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<Option<models::Dokument>> {
    let id: Option<i32> = sqlx::query_scalar!(
        "SELECT id FROM dokument WHERE 
    $1 = id AND ($2 <= meta_last_modified OR $2 IS NULL)",
        id,
        if_mod_since
    )
    .fetch_optional(&mut **executor)
    .await?;
    if let Some(id) = id {
        Ok(Some(dokument_by_id(id, executor).await?))
    } else {
        Ok(None)
    }
}
pub async fn dokument_by_id(
    id: i32,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Dokument> {
    tracing::debug!("Fetching dokument with id {}", id);
    let rec = sqlx::query!(
        "SELECT d.*, value as typ_value FROM dokument d
        INNER JOIN dokumententyp dt ON dt.id = d.typ
        WHERE d.id = $1",
        id
    )
    .fetch_one(&mut **executor)
    .await?;
    let schlagworte = sqlx::query!(
        "SELECT DISTINCT value 
        FROM rel_dok_schlagwort r
        LEFT JOIN schlagwort sw ON sw.id = r.sw_id
        WHERE dok_id = $1
        ORDER BY value ASC",
        id
    )
    .map(|r| r.value)
    .fetch_all(&mut **executor)
    .await?;
    let autoren = sqlx::query!(
        "SELECT a.* FROM rel_dok_autor 
        INNER JOIN autor a ON a.id = aut_id
        WHERE dok_id = $1 
        ORDER BY organisation ASC",
        id
    )
    .map(|r| models::Autor {
        person: r.person,
        organisation: r.organisation,
        lobbyregister: r.lobbyregister,
        fachgebiet: r.fachgebiet,
    })
    .fetch_all(&mut **executor)
    .await?;
    let subdoc_id = if let Some(i) = rec.subdoc_id {
        Some(u16::try_from(i).map_err(|e| PazufaError::Other {
            message: format!("failed converting into u16 out of the database: {e}"),
        })?)
    } else {
        None
    };

    Ok(models::Dokument {
        api_id: Some(rec.api_id),
        touched_by: vec![],
        titel: rec.titel,
        kurztitel: rec.kurztitel,
        vorwort: rec.vorwort,
        volltext: rec.volltext,
        subdoc_id,

        zp_erstellt: rec.zp_created,
        zp_modifiziert: rec.zp_lastmod,
        zp_referenz: rec.zp_referenz,

        link: rec.link,
        hash: hashes_of_dok(id, executor).await?,
        meinung: rec
            .meinung
            .map(|x| {
                u8::try_from(x).map_err(|e| PazufaError::Other {
                    message: format!("Failed to convert meinung to u8: {e}"),
                })
            })
            .transpose()?,
        zusammenfassung: zusammenfassungen_of_dok(id, executor).await?,
        schlagworte,
        autoren,
        typ: models::Doktyp::from_str(rec.typ_value.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        drucksnr: rec.drucksnr,
    })
}

#[derive(Default, Debug)]
pub struct DokumentRetrivalParameters {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub if_mod_since: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_dokument_by_parameter(
    parameters: DokumentRetrivalParameters,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<(PaginationResponsePart, Vec<models::Dokument>)> {
    let dokument_count: i64 = sqlx::query_scalar!(
        "SELECT count(1) as \"cnt!\" FROM dokument WHERE
        ($1::timestamptz is NULL) OR
        $1 <= meta_last_modified",
        parameters.if_mod_since
    )
    .fetch_one(&mut **tx)
    .await?;

    let prp = PaginationResponsePart::new(
        u32::try_from(dokument_count).map_err(|e| PazufaError::Other {
            message: format!(
                "Result length cannot be converted to i32, as it exceed i32::MAX: {e}"
            ),
        })?,
        parameters.page,
        parameters.per_page,
    );
    if let Some(page) = parameters.page
        && prp.x_page != page
    {
        return Ok((prp, vec![]));
    }
    if dokument_count == 0 {
        return Ok((prp, vec![]));
    }
    let dok_ids = sqlx::query!(
        "SELECT id FROM dokument WHERE
        ($1::timestamptz is NULL OR $1 <= meta_last_modified) AND
        ($2::timestamptz is NULL OR $2 <= zp_referenz) AND
        ($3::timestamptz is NULL OR $3 >= zp_referenz)
        LIMIT $4 
        OFFSET $5",
        parameters.if_mod_since,
        parameters.since,
        parameters.until,
        prp.limit(),
        prp.offset()
    )
    .map(|r| r.id)
    .fetch_all(&mut **tx)
    .await?;

    let mut result = vec![];
    for id in dok_ids {
        result.push(dokument_by_id(id, tx).await?);
    }
    Ok((prp, result))
}
