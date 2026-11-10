use super::dokument::dokument_by_id;
use crate::api::PaginationResponsePart;
use crate::error::{DataValidationError, PazufaError, Result};
use pazufa_models::enumerations::{Ressort, Sachgebiet};
use pazufa_models::{self as models, Zusammenfassungstupel};
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
    pub sg: Option<Sachgebiet>,
    pub rt: Option<Ressort>,
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
            LEFT JOIN ressort rt           ON vorgang.ressort = rt.id
            WHERE TRUE
            AND  ($1::int4 IS NULL OR $1 = vorgang.wahlperiode)
            AND  ($2::text IS NULL OR $2 = vt.value)
            AND  ($3::text IS NULL OR $3 = ext_stat.parl)
			AND  ($4::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.person ILIKE CONCAT('%',$4::text,'%') AND rvi.vg_id = vorgang.id))
			AND  ($5::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.organisation  ILIKE CONCAT('%',$5::text,'%') AND rvi.vg_id = vorgang.id))
			AND  ($6::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.fachgebiet  ILIKE CONCAT('%',$6::text,'%') AND rvi.vg_id = vorgang.id))
            AND ($10::text IS NULL OR $10 = rt.value)
            AND ($11::int4 IS NULL OR EXISTS(SELECT 1 FROM vg_sachgebiete sachgb INNER JOIN sachgebiet s ON s.id = sachgb.sg_id WHERE sachgb.vg_id = vorgang.id AND $11 = s.value))
        GROUP BY vorgang.id
        ORDER BY meta_last_modified 
        )

SELECT id FROM pre_table WHERE
($7::timestamptz IS NULL OR $7 <= meta_last_modified) AND
($8::timestamptz IS NULL OR $8 <= min_start) AND
($9::timestamptz IS NULL OR $9 >= max_start)

ORDER BY pre_table.meta_last_modified ASC",
filters.wp,
filters.vgtyp.map(|x|x.to_string()),
filters.parlament.map(|p|p.to_string()),
filters.inipsn, filters.iniorg, filters.inifch,
filters.if_mod_since,
filters.lower_date,
filters.upper_date,
filters.rt.map(|x|x.to_string()),
filters.sg.map(i32::from)
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

/// Returns a vorgang by it's database id
/// # Errors
/// can fail due to database errors
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
    .map(|rec| Sachgebiet::try_from(rec.value))
    .fetch_all(&mut **executor)
    .await?
    .into_iter()
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(|e| PazufaError::Other {
        message: format!("unknown sachgebiet value from the database: {e}"),
    })?;

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

    let station_ids: Vec<i32> = sqlx::query_scalar!("SELECT id FROM station WHERE vg_id = $1", id)
        .fetch_all(&mut **executor)
        .await?;

    let mut stationen = vec![];
    for sid in station_ids {
        stationen.push(station_by_id(sid, expand, executor).await?);
    }
    stationen.sort_by_key(|a| a.zp_start);

    let lobbyregister = lobbyreg_of_vorgang(id, executor).await?;

    let recs: Vec<_> = sqlx::query!(
        "
    SELECT zf.value as inhalt, zft.value as typ 
    FROM
        zusammenf_typ zft,
        vg_zusammenfassung zf
    WHERE
        zft.id = zf.zf_typ AND
        zf.vg_id = $1
        ",
        id
    )
    .fetch_all(&mut **executor)
    .await?;
    let mut tuples = vec![];
    for r in recs {
        tuples.push(Zusammenfassungstupel {
            inhalt: r.inhalt,
            typ: r.typ,
        });
    }

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
        zusammenfassung: tuples,
    })
}

/// Returns a station by it's database id
/// # Errors
/// can fail due to database errors
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
    let trojanergefahr = temp_stat
        .trojanergefahr
        .map(|x| {
            u8::try_from(x).map_err(|e| PazufaError::Other {
                message: format!("Failed to convert station trojanergefahr to u8: {e}"),
            })
        })
        .transpose()?;

    let meta_tiebreaker = temp_stat
        .tiebreak_ordering
        .map(|x| {
            u16::try_from(x).map_err(|e| PazufaError::Other {
                message: format!("Failed to convert station trojanergefahr to u8: {e}"),
            })
        })
        .transpose()?;

    Ok(models::Station {
        touched_by: vec![],
        typ: models::Stationstyp::from_str(temp_stat.stattyp.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        dokumente: doks,
        schlagworte: vec![],
        stellungnahmen,
        zp_start: temp_stat.zp_start,
        zp_modifiziert: Some(temp_stat.zp_modifiziert),

        trojanergefahr,
        titel: temp_stat.titel,
        gremium,
        api_id: Some(temp_stat.api_id),
        link: temp_stat.link,
        additional_links,
        gremium_federf: temp_stat.gremium_isff,
        meta_tiebreaker,
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
        utils::testing::{TestSetup, generate},
    };
    use pazufa_models::{DokumentOrApiId, params};
    use uuid::Uuid;

    /// tests
    /// `pub async fn vorgang_fetch_dbids<'e, E: sqlx::PgExecutor<'e>>(`
    /// with all variants of failing and nonfailing inputs
    #[tokio::test]
    async fn filtering_vorgaenge() {
        use super::{VGGetParameters, vorgang_dbid, vorgang_fetch_dbids};
        use crate::api::PutResponse;
        use pazufa_models::{
            self as models,
            dokument::HashWrapper,
            enumerations::{Ressort, Sachgebiet},
        };
        use std::collections::HashSet;

        fn ts(s: &str) -> chrono::DateTime<chrono::Utc> {
            chrono::DateTime::parse_from_rfc3339(s)
                .expect("static datetime")
                .to_utc()
        }
        /// default Vorgang, made unique so it does not merge with its siblings
        fn unique_vorgang(
            n: u8,
            wp: u16,
            zp_start: &str,
            initiatoren: Vec<models::Autor>,
            ressort: Option<Ressort>,
            sachgebiete: Vec<Sachgebiet>,
        ) -> models::Vorgang {
            let mut vg = generate::default_vorgang();
            vg.api_id = Uuid::from_u128(0xf117_0000_0000_0000_0000_0000_0000_0000 | u128::from(n));
            vg.wahlperiode = wp;
            vg.initiatoren = initiatoren;
            vg.ressort = ressort;
            vg.sachgebiete = sachgebiete;
            for id in &mut vg.ids {
                id.id = format!("{}-{n}", id.id);
            }
            let stat = &mut vg.stationen[0];
            stat.api_id = Some(Uuid::from_u128(
                0xf117_5717_0000_0000_0000_0000_0000_0000 | u128::from(n),
            ));
            stat.gremium.wahlperiode = wp;
            stat.zp_start = ts(zp_start);
            for (i, d) in stat
                .dokumente
                .iter_mut()
                .chain(stat.stellungnahmen.iter_mut())
                .enumerate()
            {
                if let DokumentOrApiId::Dokument(d) = d {
                    d.api_id = Some(Uuid::from_u128(
                        0xf117_d0c0_0000_0000_0000_0000_0000_0000
                            | (u128::from(n) << 8)
                            | i as u128,
                    ));
                    d.hash = HashWrapper::Legacy(format!("f117{n:02x}{i:02x}{}", "0".repeat(40)));
                }
            }
            vg
        }

        let scenario = TestSetup::new("filtering_vorgaenge").await;

        let vg_a = unique_vorgang(
            1,
            20,
            "1950-01-01T22:01:02+00:00",
            vec![
                generate::default_autor_person(),
                generate::default_autor_institution(),
            ],
            None,
            vec![],
        );
        let vg_b = unique_vorgang(
            2,
            21,
            "2000-06-01T12:00:00+00:00",
            vec![generate::default_autor_experte()],
            Some(Ressort::Finanzen),
            vec![Sachgebiet::StaatundPolitik, Sachgebiet::Wahlen],
        );
        let vg_c = unique_vorgang(
            3,
            20,
            "2020-01-01T12:00:00+00:00",
            vec![generate::default_autor_lobby()],
            Some(Ressort::Bildung),
            vec![Sachgebiet::Wahlen],
        );
        for vg in [&vg_a, &vg_b, &vg_c] {
            let rsp = vorgang_put(&scenario.server, 1, &params::XScraperId(Uuid::now_v7()), vg)
                .await
                .expect("Failed to push Vorgang");
            assert!(
                matches!(rsp, PutResponse::Success),
                "Unexpected put response for {}: {rsp:?}",
                vg.api_id
            );
        }

        let pool = &scenario.server.sqlx_db;
        let mut dbid = std::collections::HashMap::new();
        for (name, vg) in [("a", &vg_a), ("b", &vg_b), ("c", &vg_c)] {
            let id = vorgang_dbid(vg.api_id, pool)
                .await
                .expect("db lookup")
                .unwrap_or_else(|| panic!("Vorgang {name} was not inserted"));
            dbid.insert(id, name);
        }
        assert_eq!(dbid.len(), 3, "Vorgänge were merged instead of inserted");

        let fetch = async |filters: VGGetParameters| -> HashSet<&str> {
            let dbg = format!("{filters:?}");
            vorgang_fetch_dbids(filters, pool)
                .await
                .unwrap_or_else(|e| panic!("query failed for {dbg}: {e}"))
                .iter()
                .map(|id| *dbid.get(id).expect("unknown db id returned"))
                .collect()
        };
        let set = |v: &[&'static str]| v.iter().copied().collect::<HashSet<&str>>();

        let cases: Vec<(&str, VGGetParameters, HashSet<&str>)> = vec![
            (
                "no filters",
                VGGetParameters::default(),
                set(&["a", "b", "c"]),
            ),
            // wahlperiode
            (
                "wp 20",
                VGGetParameters {
                    wp: Some(20),
                    ..Default::default()
                },
                set(&["a", "c"]),
            ),
            (
                "wp 21",
                VGGetParameters {
                    wp: Some(21),
                    ..Default::default()
                },
                set(&["b"]),
            ),
            (
                "wp 99",
                VGGetParameters {
                    wp: Some(99),
                    ..Default::default()
                },
                set(&[]),
            ),
            // vorgangstyp
            (
                "vgtyp matching",
                VGGetParameters {
                    vgtyp: Some(models::Vorgangstyp::BuZustimmungInibreg),
                    ..Default::default()
                },
                set(&["a", "b", "c"]),
            ),
            (
                "vgtyp not matching",
                VGGetParameters {
                    vgtyp: Some(models::Vorgangstyp::BuEinspruchInibreg),
                    ..Default::default()
                },
                set(&[]),
            ),
            // parlament
            (
                "parlament matching",
                VGGetParameters {
                    parlament: Some(models::Parlament::Bt),
                    ..Default::default()
                },
                set(&["a", "b", "c"]),
            ),
            (
                "parlament not matching",
                VGGetParameters {
                    parlament: Some(models::Parlament::By),
                    ..Default::default()
                },
                set(&[]),
            ),
            // initiator person / organisation / fachgebiet (substring, case insensitive)
            (
                "inipsn substring",
                VGGetParameters {
                    inipsn: Some("töpfer".into()),
                    ..Default::default()
                },
                set(&["a"]),
            ),
            (
                "inipsn not matching",
                VGGetParameters {
                    inipsn: Some("Niemand Bekanntes".into()),
                    ..Default::default()
                },
                set(&[]),
            ),
            (
                "iniorg shared substring",
                VGGetParameters {
                    iniorg: Some("kachelofen".into()),
                    ..Default::default()
                },
                set(&["b", "c"]),
            ),
            (
                "iniorg exact",
                VGGetParameters {
                    iniorg: Some("Mysterium der Ministerien".into()),
                    ..Default::default()
                },
                set(&["a"]),
            ),
            (
                "inifch",
                VGGetParameters {
                    inifch: Some("ofenbau".into()),
                    ..Default::default()
                },
                set(&["b"]),
            ),
            (
                "inifch not matching",
                VGGetParameters {
                    inifch: Some("Raketenwissenschaft".into()),
                    ..Default::default()
                },
                set(&[]),
            ),
            // ressort
            (
                "ressort Finanzen",
                VGGetParameters {
                    rt: Some(Ressort::Finanzen),
                    ..Default::default()
                },
                set(&["b"]),
            ),
            (
                "ressort Bildung",
                VGGetParameters {
                    rt: Some(Ressort::Bildung),
                    ..Default::default()
                },
                set(&["c"]),
            ),
            (
                "ressort not matching",
                VGGetParameters {
                    rt: Some(Ressort::Arbeit),
                    ..Default::default()
                },
                set(&[]),
            ),
            // sachgebiet
            (
                "sg Wahlen",
                VGGetParameters {
                    sg: Some(Sachgebiet::Wahlen),
                    ..Default::default()
                },
                set(&["b", "c"]),
            ),
            (
                "sg StaatundPolitik",
                VGGetParameters {
                    sg: Some(Sachgebiet::StaatundPolitik),
                    ..Default::default()
                },
                set(&["b"]),
            ),
            (
                "sg not matching",
                VGGetParameters {
                    sg: Some(Sachgebiet::Verteidigung),
                    ..Default::default()
                },
                set(&[]),
            ),
            // date range on the stations' zp_start
            (
                "lower_date",
                VGGetParameters {
                    lower_date: Some(ts("1999-01-01T00:00:00+00:00")),
                    ..Default::default()
                },
                set(&["b", "c"]),
            ),
            (
                "upper_date",
                VGGetParameters {
                    upper_date: Some(ts("2010-01-01T00:00:00+00:00")),
                    ..Default::default()
                },
                set(&["a", "b"]),
            ),
            (
                "lower and upper date",
                VGGetParameters {
                    lower_date: Some(ts("1999-01-01T00:00:00+00:00")),
                    upper_date: Some(ts("2010-01-01T00:00:00+00:00")),
                    ..Default::default()
                },
                set(&["b"]),
            ),
            (
                "inverted date range",
                VGGetParameters {
                    lower_date: Some(ts("2010-01-01T00:00:00+00:00")),
                    upper_date: Some(ts("1999-01-01T00:00:00+00:00")),
                    ..Default::default()
                },
                set(&[]),
            ),
            // if_mod_since
            (
                "if_mod_since in the past",
                VGGetParameters {
                    if_mod_since: Some(ts("1900-01-01T00:00:00+00:00")),
                    ..Default::default()
                },
                set(&["a", "b", "c"]),
            ),
            (
                "if_mod_since in the future",
                VGGetParameters {
                    if_mod_since: Some(chrono::Utc::now() + chrono::Duration::days(1)),
                    ..Default::default()
                },
                set(&[]),
            ),
            // combinations
            (
                "wp + ressort",
                VGGetParameters {
                    wp: Some(20),
                    rt: Some(Ressort::Bildung),
                    ..Default::default()
                },
                set(&["c"]),
            ),
            (
                "wp + sg",
                VGGetParameters {
                    wp: Some(20),
                    sg: Some(Sachgebiet::Wahlen),
                    ..Default::default()
                },
                set(&["c"]),
            ),
            (
                "contradicting ressort + sg",
                VGGetParameters {
                    rt: Some(Ressort::Bildung),
                    sg: Some(Sachgebiet::StaatundPolitik),
                    ..Default::default()
                },
                set(&[]),
            ),
        ];

        let mut failures = vec![];
        for (name, filters, expected) in cases {
            let got = fetch(filters).await;
            if got != expected {
                failures.push(format!("{name}: expected {expected:?}, got {got:?}"));
            }
        }
        scenario.teardown().await;
        assert!(
            failures.is_empty(),
            "Filter mismatches:\n{}",
            failures.join("\n")
        );
    }

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
