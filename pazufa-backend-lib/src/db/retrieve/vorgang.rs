use super::dokument::dokument_by_id;
use crate::api::PaginationResponsePart;
use crate::error::{DataValidationError, PazufaError, Result};
use crate::models;
use crate::models::enumerations::{Ressort, Sachgebiet};
use std::str::FromStr;
use uuid::Uuid;

/// This struct contains all applicable filters for a database query that
/// filters Vorgang Objects
#[derive(Debug, Default)]
pub struct VGGetParameters {
    pub lower_date: Option<chrono::DateTime<chrono::Utc>>,
    pub upper_date: Option<chrono::DateTime<chrono::Utc>>,
    pub if_mod_since: Option<chrono::DateTime<chrono::Utc>>,
    pub parlament: Option<models::Parlament>,
    pub wp: Option<i32>,
    pub inipsn: Option<String>,
    pub iniorg: Option<String>,
    pub inifch: Option<String>,
    pub vgtyp: Option<models::Vorgangstyp>,
}

async fn lobbyreg_of_vorgang(
    id: i32,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<Vec<models::Lobbyregistereintrag>> {
    let mut lobbyreg_records =
        sqlx::query!("SELECT * FROM lobbyregistereintrag WHERE vg_id = $1", id)
            .map(|r| {
                (
                    r.id,
                    models::Lobbyregistereintrag {
                        intention: r.intention,
                        organisation: models::Autor {
                            fachgebiet: None,
                            lobbyregister: None,
                            organisation: String::new(),
                            person: None,
                        },
                        link: r.link,
                        interne_id: r.interne_id,
                        betroffene_drucksachen: vec![],
                    },
                    r.organisation,
                )
            })
            .fetch_all(&mut **executor)
            .await?;
    let mut lobbyregs = vec![];
    for (id, object, org_id) in lobbyreg_records.drain(..) {
        let drucks = sqlx::query!(
            "SELECT drucksnr FROM rel_lobbyreg_drucksnr WHERE lob_id = $1",
            id
        )
        .map(|r| r.drucksnr)
        .fetch_all(&mut **executor)
        .await?;
        lobbyregs.push(models::Lobbyregistereintrag {
            organisation: sqlx::query!("SELECT * FROM autor WHERE id = $1", org_id)
                .map(|r| models::Autor {
                    fachgebiet: r.fachgebiet,
                    lobbyregister: r.lobbyregister,
                    organisation: r.organisation,
                    person: r.person,
                })
                .fetch_one(&mut **executor)
                .await?,
            betroffene_drucksachen: drucks,
            ..object
        });
    }
    Ok(lobbyregs)
}

/// Returns the database row ID (primary key) of any Vorgang object
/// given it's API ID
/// # Errors
/// Can fail if the database operation fails
pub async fn vorgang_dbid<'e, E: sqlx::PgExecutor<'e>>(
    api_id: Uuid,
    executor: E,
) -> Result<Option<i32>> {
    let id = sqlx::query_scalar!("SELECT id FROM vorgang WHERE api_id = $1", api_id)
        .fetch_optional(executor)
        .await?;
    Ok(id)
}
/// Fetches the Database Primary keys of the Vorgang table, filtered according to the parameter
/// struct passed along.
/// Note that `VGGetParameters::default()` will NOT yield an empty result, so be careful
///
/// # Errors
/// Can fail due to database constraints
pub async fn vorgang_fetch_dbids<'e, E: sqlx::PgExecutor<'e>>(
    filters: VGGetParameters,
    exec: E,
) -> Result<Vec<i32>> {
    let vg_list: Vec<i32> = sqlx::query_scalar!(
        "WITH pre_table AS (
        SELECT vorgang.id, meta_last_modified, MIN(ext_stat.zp_start) as min_start, MAX(ext_stat.zp_start) as max_start FROM vorgang
            INNER JOIN vorgangstyp vt ON vt.id = vorgang.typ
            LEFT JOIN (
                    SELECT s.vg_id, parlament.value as parl, s.zp_start FROM station s
                    INNER JOIN gremium g ON g.id = s.gr_id
			        INNER JOIN parlament on parlament.id = g.parl
                ) AS ext_stat ON ext_stat.vg_id = vorgang.id
            WHERE TRUE
            AND ($1::int4 IS NULL OR $1 = vorgang.wahlperiode)
            AND ($2::text IS NULL OR $2 = vt.value)
            AND ($3::text IS NULL OR $3 = ext_stat.parl)
			AND ($4::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.person ILIKE CONCAT('%',$4::text,'%') AND rvi.vg_id = vorgang.id))
			AND ($5::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.organisation  ILIKE CONCAT('%',$5::text,'%') AND rvi.vg_id = vorgang.id))
			AND ($6::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.fachgebiet  ILIKE CONCAT('%',$6::text,'%') AND rvi.vg_id = vorgang.id))
        GROUP BY vorgang.id
        ORDER BY meta_last_modified 
        )

SELECT id FROM pre_table WHERE
($7::timestamptz IS NULL OR $7 <= meta_last_modified) AND
($8::timestamptz IS NULL OR $8 <= min_start) AND
($9::timestamptz IS NULL OR $9 >= max_start)

ORDER BY pre_table.meta_last_modified ASC",
filters.wp, filters.vgtyp.map(|x|x.to_string()),
filters.parlament.map(|p|p.to_string()),
filters.inipsn, filters.iniorg, filters.inifch,
filters.if_mod_since,
filters.lower_date,
filters.upper_date
    )
    .fetch_all(exec).await?;
    Ok(vg_list)
}

/// Get a Vorgang by it's DB ID with options to expand the result (not fold the documents)
/// and limit by `if_mod_since` timestamps
/// # Errors
/// Can fail due to Database errors
pub async fn vorgang_by_id_ims(
    id: i32,
    expand: bool,
    if_mod_since: Option<chrono::DateTime<chrono::Utc>>,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<Option<models::Vorgang>> {
    let id: Option<i32> = sqlx::query_scalar!(
        "SELECT id FROM vorgang 
        WHERE 
        $1 = id AND 
        ($2 <= meta_last_modified OR $2 IS NULL)",
        id,
        if_mod_since
    )
    .fetch_optional(&mut **executor)
    .await?;
    if let Some(id) = id {
        Ok(Some(vorgang_by_id(id, expand, executor).await?))
    } else {
        Ok(None)
    }
}

pub async fn vorgang_by_id(
    id: i32,
    expand: bool,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Vorgang> {
    let pre_vg = sqlx::query!(
        "SELECT v.*, vt.value FROM vorgang v
        INNER JOIN vorgangstyp vt ON vt.id = v.typ
        WHERE 
        v.id = $1
        ",
        id
    )
    .fetch_one(&mut **executor)
    .await?;

    let ressort = sqlx::query!("SELECT value from ressort WHERE id = $1", pre_vg.ressort)
        .map(|r| Ressort::from_str(&r.value))
        .fetch_optional(&mut **executor)
        .await?;
    let ressort = if let Some(ressort) = ressort {
        Some(ressort.map_err(|e| PazufaError::Other {
            message: format!("unknown ressort value from the database: {e}"),
        })?)
    } else {
        None
    };
    let sachgebiete = sqlx::query!(
        "SELECT s.value 
        FROM vg_sachgebiete vs, sachgebiet s
        WHERE
            s.id = vs.sg_id AND vs.vg_id = $1
        ",
        id
    )
    .map(|rec| Sachgebiet::from(rec.value))
    .fetch_all(&mut **executor)
    .await?;

    let links = sqlx::query!(
        "SELECT link FROM rel_vorgang_links WHERE vg_id = $1 ORDER BY link ASC",
        id
    )
    .map(|row| row.link)
    .fetch_all(&mut **executor)
    .await?;

    let init_inst = sqlx::query!(
        "SELECT a.* FROM rel_vorgang_init 
        INNER JOIN autor a ON a.id = in_id
        WHERE vg_id = $1 ORDER BY organisation ASC",
        id
    )
    .map(|row| models::Autor {
        fachgebiet: row.fachgebiet,
        lobbyregister: row.lobbyregister,
        organisation: row.organisation,
        person: row.person,
    })
    .fetch_all(&mut **executor)
    .await?;

    let ids = sqlx::query!(
        "
    SELECT value as typ, identifikator as ident 
    FROM rel_vorgang_ident r
    INNER JOIN vg_ident_typ t ON t.id = r.typ
    WHERE r.vg_id = $1
    ORDER BY ident ASC",
        id
    )
    .map(|row| models::VgIdent {
        typ: row.typ,
        id: row.ident,
    })
    .fetch_all(&mut **executor)
    .await?;

    let schlagworte: Vec<String> = sqlx::query_scalar!(
        "SELECT s.value from schlagwort s
    INNER JOIN rel_vorgang_schlagwort rvs ON rvs.sw_id = s.id
    WHERE rvs.vg_id = $1
    ORDER BY s.value ASC",
        id
    )
    .fetch_all(&mut **executor)
    .await?;

    let station_ids = sqlx::query!("SELECT id FROM station WHERE vg_id = $1", id)
        .map(|row| row.id)
        .fetch_all(&mut **executor)
        .await?;

    let mut stationen = vec![];
    for sid in station_ids {
        stationen.push(station_by_id(sid, expand, executor).await?);
    }
    stationen.sort_by_key(|a| a.zp_start);

    let lobbyregister = lobbyreg_of_vorgang(id, executor).await?;

    Ok(models::Vorgang {
        touched_by: vec![],
        lobbyregister,
        api_id: pre_vg.api_id,
        titel: pre_vg.titel,
        kurztitel: pre_vg.kurztitel,
        wahlperiode: u16::try_from(pre_vg.wahlperiode).map_err(|e| PazufaError::Other {
            message: format!("Failed to convert vorgang wahlperiode as it is negative: {e}"),
        })?,
        schlagworte,
        verfassungsaendernd: pre_vg.verfaend,
        typ: models::Vorgangstyp::from_str(pre_vg.value.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        initiatoren: init_inst,
        ids,
        links,
        stationen,
        ressort,
        sachgebiete,
    })
}
pub async fn station_by_id(
    id: i32,
    expand: bool,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Station> {
    let doks = if expand {
        let doks = sqlx::query!(
            "SELECT d.id FROM rel_station_dokument rsd
        INNER JOIN dokument d ON d.id = rsd.dok_id
        WHERE rsd.stat_id = $1
        ORDER BY d.link ASC",
            id
        )
        .map(|r| r.id)
        .fetch_all(&mut **executor)
        .await?;
        let mut final_doks = vec![];
        for d in doks {
            final_doks.push(models::DokumentOrApiId::Dokument(Box::new(
                dokument_by_id(d, &mut *executor).await?,
            )));
        }
        final_doks
    } else {
        sqlx::query!(
            "SELECT d.api_id FROM rel_station_dokument rsd
        INNER JOIN dokument d ON d.id = rsd.dok_id
        WHERE rsd.stat_id = $1
        ORDER BY d.link ASC",
            id
        )
        .map(|r| models::DokumentOrApiId::ApiId(r.api_id))
        .fetch_all(&mut **executor)
        .await?
    };
    let stellungnahmen = if expand {
        let doks = sqlx::query!(
            "SELECT id FROM rel_station_stln rss 
        INNER JOIN dokument d ON d.id = rss.dok_id 
        WHERE rss.stat_id = $1
        ORDER BY d.link ASC",
            id
        )
        .map(|r| r.id)
        .fetch_all(&mut **executor)
        .await?;
        let mut final_doks = vec![];
        for d in doks {
            final_doks.push(models::DokumentOrApiId::Dokument(Box::new(
                dokument_by_id(d, &mut *executor).await?,
            )));
        }
        final_doks
    } else {
        sqlx::query!(
            "SELECT api_id FROM rel_station_stln rss 
        INNER JOIN dokument d ON d.id = rss.dok_id 
        WHERE rss.stat_id = $1
        ORDER BY d.link ASC",
            id
        )
        .map(|r| models::DokumentOrApiId::ApiId(r.api_id))
        .fetch_all(&mut **executor)
        .await?
    };

    let additional_links = sqlx::query!("SELECT link FROM rel_station_link WHERE stat_id = $1", id)
        .map(|r| r.link)
        .fetch_all(&mut **executor)
        .await?;
    let temp_stat = sqlx::query!(
        "SELECT s.*, p.value as parlv, st.value as stattyp
        FROM station s
        INNER JOIN gremium g ON g.id = s.gr_id
        INNER JOIN parlament p ON p.id = g.parl
        INNER JOIN stationstyp st ON st.id = s.typ
        WHERE s.id=$1",
        id
    )
    .fetch_one(&mut **executor)
    .await?;

    let gremium = sqlx::query!(
        "SELECT p.value, g.name, g.wp, g.link 
        FROM gremium g INNER JOIN parlament p on p.id = g.parl
        WHERE g.id = $1",
        temp_stat.gr_id
    )
    .map(|x| {
        Result::<models::Gremium>::Ok(models::Gremium {
            name: x.name,
            wahlperiode: u16::try_from(x.wp).map_err(|e| PazufaError::Other {
                message: format!("Failed to convert gremium wahlperiode as it is negative: {e}"),
            })?,
            parlament: models::Parlament::from_str(&x.value).map_err(|_e| PazufaError::Other {
                message: format!("Invalid parliament in database: {}", x.value),
            })?,
            link: x.link,
        })
    })
    .fetch_one(&mut **executor)
    .await??;

    Ok(models::Station {
        touched_by: vec![],
        typ: models::Stationstyp::from_str(temp_stat.stattyp.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        dokumente: doks,
        schlagworte: vec![],
        stellungnahmen,
        zp_start: temp_stat.zp_start,
        zp_modifiziert: Some(temp_stat.zp_modifiziert),

        trojanergefahr: temp_stat
            .trojanergefahr
            .map(|x| {
                u8::try_from(x).map_err(|e| PazufaError::Other {
                    message: format!("Failed to convert station trojanergefahr to u8: {e}"),
                })
            })
            .transpose()?,
        titel: temp_stat.titel,
        gremium,
        api_id: Some(temp_stat.api_id),
        link: temp_stat.link,
        additional_links,
        gremium_federf: temp_stat.gremium_isff,
    })
}

/// Filters Vorgang Objects by the passed parameters, applies pagination and returns the result
/// as full Objects.
/// Optionally Represents them with folded Documents (`expand==false`) or expanded ones
/// # Errors
/// Can fail due to database errors or integer conversion issues(very unlikely)
pub async fn vorgang_by_parameter(
    params: VGGetParameters,
    page: Option<u32>,
    per_page: Option<u32>,
    expand: bool,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<(PaginationResponsePart, Vec<models::Vorgang>)> {
    let mut vg_list = vorgang_fetch_dbids(params, &mut **executor).await?;
    let prp = PaginationResponsePart::new(
        u32::try_from(vg_list.len()).map_err(|e| PazufaError::Other {
            message: format!(
                "Result length cannot be converted to i32, as it exceed i32::MAX: {e}"
            ),
        })?,
        page,
        per_page,
    );

    // if the list is empty, or prp clamped the page
    // to the existing data, just send an empty vector back
    if let Some(p) = page
        && p > prp.x_page
    {
        return Ok((prp, vec![]));
    }
    if vg_list.is_empty() {
        return Ok((prp, vec![]));
    }

    let mut vector = Vec::with_capacity(vg_list.len());
    for id in vg_list.drain(prp.start()..prp.end()) {
        vector.push(vorgang_by_id(id, expand, executor).await?);
    }
    Ok((prp, vector))
}

#[cfg(test)]
mod test {
    use crate::{
        api::vorgang::vorgang_put,
        models::{DokumentOrApiId, params},
        utils::testing::{TestSetup, generate},
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn station_doks_expanded() {
        let scenario = TestSetup::new("station_doks_expanded").await;
        let test_vorgang = generate::default_vorgang();
        let _ = vorgang_put(
            &scenario.server,
            1,
            &params::XScraperId(Uuid::now_v7()),
            &test_vorgang,
        )
        .await
        .expect("Failed to get valid server respone when trying to push new Vorgang.");
        let mut tx = scenario
            .server
            .sqlx_db
            .begin()
            .await
            .expect("a db connection");
        let vg = super::vorgang_by_id(1, false, &mut tx)
            .await
            .expect("Expected Success");
        let i1 = vg.stationen[0].dokumente.iter();
        let i2 = vg.stationen[0].stellungnahmen.iter();
        for d in i1.chain(i2) {
            assert!(matches!(d, DokumentOrApiId::ApiId(_)));
        }
        let vg = super::vorgang_by_id(1, true, &mut tx)
            .await
            .expect("Expected Success");
        let i1 = vg.stationen[0].dokumente.iter();
        let i2 = vg.stationen[0].stellungnahmen.iter();
        for d in i1.chain(i2) {
            assert!(matches!(d, DokumentOrApiId::Dokument(_)));
        }
        scenario.teardown().await;
    }
}
