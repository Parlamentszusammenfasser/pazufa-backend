use std::str::FromStr;

use crate::api::PaginationResponsePart;
use crate::error::{DataValidationError, LTZFError, Result};
use crate::models;
use uuid::Uuid;

pub(crate) async fn vorgang_by_id(
    id: i32,
    expand: bool,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Vorgang> {
    let pre_vg = sqlx::query!(
        "SELECT v.*, vt.value FROM vorgang v
        INNER JOIN vorgangstyp vt ON vt.id = v.typ
        WHERE v.id = $1",
        id
    )
    .fetch_one(&mut **executor)
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

    let station_ids = sqlx::query!("SELECT id FROM station WHERE vg_id = $1", id)
        .map(|row| row.id)
        .fetch_all(&mut **executor)
        .await?;

    let mut stationen = vec![];
    for sid in station_ids {
        stationen.push(station_by_id(sid, expand, executor).await?);
    }
    stationen.sort_by_key(|a| a.zp_start);

    // lobbyregistereinträge
    let mut lobbyreg_records =
        sqlx::query!("SELECT * FROM lobbyregistereintrag WHERE vg_id = $1", id)
            .map(|r| {
                (
                    r.id,
                    models::Lobbyregeintrag {
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
        lobbyregs.push(models::Lobbyregeintrag {
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

    Ok(models::Vorgang {
        touched_by: vec![],
        lobbyregister: lobbyregs,
        api_id: pre_vg.api_id,
        titel: pre_vg.titel,
        kurztitel: pre_vg.kurztitel,
        wahlperiode: u16::try_from(pre_vg.wahlperiode).map_err(|e| LTZFError::Other {
            message: format!("Failed to convert vorgang wahlperiode as it is negative: {e}"),
        })?,
        verfassungsaendernd: pre_vg.verfaend,
        typ: models::Vorgangstyp::from_str(pre_vg.value.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        initiatoren: init_inst,
        ids,
        links,
        stationen,
    })
}

pub(crate) async fn station_by_id(
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
    let schlagworte = sqlx::query!(
        "SELECT DISTINCT(value) FROM rel_station_schlagwort r
        LEFT JOIN schlagwort sw ON sw.id = r.sw_id
        WHERE r.stat_id = $1
        ORDER BY value ASC",
        id
    )
    .map(|sw| sw.value)
    .fetch_all(&mut **executor)
    .await?;

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
            wahlperiode: u16::try_from(x.wp).map_err(|e| LTZFError::Other {
                message: format!("Failed to convert gremium wahlperiode as it is negative: {e}"),
            })?,
            parlament: models::Parlament::from_str(&x.value).map_err(|_e| LTZFError::Other {
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
        schlagworte,
        stellungnahmen,
        zp_start: temp_stat.zp_start,
        zp_modifiziert: Some(temp_stat.zp_modifiziert),

        trojanergefahr: temp_stat
            .trojanergefahr
            .map(|x| {
                u8::try_from(x).map_err(|e| LTZFError::Other {
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

pub(crate) async fn dokument_by_id(
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

    Ok(models::Dokument {
        api_id: Some(rec.api_id),
        touched_by: vec![],
        titel: rec.titel,
        kurztitel: rec.kurztitel,
        vorwort: rec.vorwort,
        volltext: rec.volltext,

        zp_erstellt: rec.zp_created,
        zp_modifiziert: rec.zp_lastmod,
        zp_referenz: rec.zp_referenz,

        link: rec.link,
        hash: rec.hash,
        meinung: rec
            .meinung
            .map(|x| {
                u8::try_from(x).map_err(|e| LTZFError::Other {
                    message: format!("Failed to convert meinung to u8: {e}"),
                })
            })
            .transpose()?,
        zusammenfassung: rec.zusammenfassung,
        schlagworte,
        autoren,
        typ: models::Doktyp::from_str(rec.typ_value.as_str())
            .map_err(|e| DataValidationError::InvalidEnumValue { msg: e })?,
        drucksnr: rec.drucksnr,
    })
}

/// the crucial part is how to find out which vg are connected to a DRCKS
/// if there exists a station which contains a document mentioned in the top, its vorgang is connected
pub(crate) async fn top_by_id(
    id: i32,
    expand: bool,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Top> {
    let scaffold = sqlx::query!("SELECT titel, nummer FROM top WHERE id = $1", id)
        .fetch_one(&mut **tx)
        .await?;
    // ds
    let dokumente = if expand {
        let doks = sqlx::query!(
            "SELECT d.id
    FROM tops_doks td 
    INNER JOIN dokument d ON td.dok_id = d.id
    WHERE td.top_id = $1
    ORDER BY d.link ASC",
            id
        )
        .map(|r| r.id)
        .fetch_all(&mut **tx)
        .await?;
        let mut final_doks = vec![];
        for d in doks {
            final_doks.push(models::DokumentOrApiId::Dokument(Box::new(
                dokument_by_id(d, &mut *tx).await?,
            )));
        }
        final_doks
    } else {
        sqlx::query!(
            "
    SELECT d.api_id
    FROM tops_doks td 
    INNER JOIN dokument d ON td.dok_id = d.id
    WHERE td.top_id = $1
    ORDER BY d.link ASC",
            id
        )
        .map(|r| models::DokumentOrApiId::ApiId(r.api_id))
        .fetch_all(&mut **tx)
        .await?
    };

    // vgs
    let vorgang_id = sqlx::query!(
        "
    SELECT DISTINCT(v.api_id) FROM station s    -- alle vorgänge von stationen,
INNER JOIN vorgang v ON v.id = s.vg_id
WHERE
EXISTS ( 									-- mit denen mindestens ein dokument assoziiert ist, dass hier auftaucht
	SELECT 1 FROM rel_station_dokument rsd 
	INNER JOIN tops_doks td ON td.dok_id = rsd.dok_id
	WHERE td.top_id = $1 AND rsd.stat_id = s.id
) OR EXISTS(			             		-- mit denen mindestens ein dokument assoziiert ist, dass hier auftaucht
	SELECT 1 FROM rel_station_stln rss
	INNER JOIN tops_doks td ON td.dok_id = rss.dok_id
	WHERE td.top_id = $1 AND rss.stat_id = s.id
)
    ORDER BY api_id ASC",
        id
    )
    .map(|r| r.api_id)
    .fetch_all(&mut **tx)
    .await?;

    Ok(models::Top {
        nummer: u32::try_from(scaffold.nummer).map_err(|e| LTZFError::Other {
            message: format!("Failed to convert top number to u32 as it is negative: {e}"),
        })?,
        titel: scaffold.titel,
        dokumente,
        vorgang_id,
    })
}

pub(crate) async fn sitzung_by_id(
    id: i32,
    expand: bool,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<models::Sitzung> {
    let scaffold = sqlx::query!(
        "SELECT a.api_id, a.public, a.termin, p.value as plm, a.link as as_link, a.titel, a.nummer,
        g.name as grname, g.wp, g.link as gr_link FROM sitzung a
        INNER JOIN gremium g ON g.id = a.gr_id
        INNER JOIN parlament p ON p.id = g.parl 
        WHERE a.id = $1",
        id
    )
    .fetch_one(&mut **tx)
    .await?;
    // tops
    let topids = sqlx::query!(
        "SELECT * FROM top t WHERE t.sid = $1 ORDER BY titel ASC",
        id
    )
    .map(|r| r.id)
    .fetch_all(&mut **tx)
    .await?;
    let mut tops = vec![];
    for top in &topids {
        tops.push(top_by_id(*top, expand, tx).await?);
    }
    // experten
    let experten = sqlx::query!(
        "SELECT a.* FROM rel_sitzung_experten rae 
        INNER JOIN autor a ON rae.eid = a.id
		WHERE rae.sid = $1
        ORDER BY a.organisation ASC, a.person ASC",
        id
    )
    .map(|r| models::Autor {
        fachgebiet: r.fachgebiet,
        lobbyregister: r.lobbyregister,
        organisation: r.organisation,
        person: r.person,
    })
    .fetch_all(&mut **tx)
    .await?;

    let dokumente = sqlx::query!(
        "SELECT api_id from rel_sitzung_doks rsd
        INNER JOIN dokument d ON d.id = rsd.did WHERE rsd.sid = $1",
        id
    )
    .map(|r| models::DokumentOrApiId::ApiId(r.api_id))
    .fetch_all(&mut **tx)
    .await?;
    Ok(models::Sitzung {
        api_id: Some(scaffold.api_id),
        touched_by: vec![],
        nummer: u32::try_from(scaffold.nummer).map_err(|e| LTZFError::Other {
            message: format!("Failed to convert sitzung number to u32 as it is negative: {e}"),
        })?,
        titel: scaffold.titel,
        public: scaffold.public,
        termin: scaffold.termin,
        gremium: models::Gremium {
            name: scaffold.grname,
            link: scaffold.gr_link,
            wahlperiode: u16::try_from(scaffold.wp).map_err(|e| LTZFError::Other {
                message: format!(
                    "Failed to convert gremium wahlperiode to u32 as it is negative: {e}"
                ),
            })?,
            parlament: models::Parlament::from_str(&scaffold.plm).map_err(|_e| {
                LTZFError::Other {
                    message: format!("Invalid parliament in database: {}", scaffold.plm),
                }
            })?,
        },
        tops,
        link: scaffold.as_link,
        experten,
        dokumente,
    })
}

pub struct SitzungFilterParameters {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub parlament: Option<models::Parlament>,
    pub wp: Option<u16>,
    pub vgid: Option<Uuid>,
    pub gremium_like: Option<String>,
}
/// returns a tuple made up of: (`total_count`, `retrieved_items`)
pub(crate) async fn sitzung_by_param(
    params: &SitzungFilterParameters,
    page: Option<u32>,
    per_page: Option<u32>,
    expand: bool,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<(PaginationResponsePart, Vec<models::Sitzung>)> {
    let mut as_list = sqlx::query!(
        "
      WITH pre_table AS (
        SELECT a.id, MAX(a.termin) as lastmod FROM  sitzung a
		INNER JOIN gremium g ON g.id = a.gr_id
		INNER JOIN parlament p ON p.id = g.parl
		WHERE p.value = COALESCE($1, p.value)
		AND g.wp      = COALESCE($2, g.wp)
        AND ($5::text IS NULL OR g.name ILIKE CONCAT('%', $5, '%'))
        GROUP BY a.id
        ORDER BY lastmod
        ),
	vgref AS   (
		SELECT p.id, v.api_id FROM pre_table p
		INNER JOIN top on top.sid = p.id
		INNER JOIN tops_doks ON tops_doks.top_id = top.id
		LEFT JOIN rel_station_dokument rsd ON rsd.dok_id = tops_doks.dok_id
		LEFT JOIN rel_station_stln rss ON rss.dok_id = tops_doks.dok_id
		INNER JOIN station s ON s.id = rsd.stat_id OR s.id = rss.stat_id
		INNER JOIN vorgang v ON s.vg_id = v.id
	)

SELECT * FROM pre_table WHERE
lastmod > COALESCE($3::timestamptz,'1940-01-01T20:20:20Z') AND
lastmod < COALESCE($4, NOW()) AND
($6::uuid IS NULL OR EXISTS (SELECT 1 FROM vgref WHERE pre_table.id = vgref.id AND vgref.api_id = COALESCE($6, vgref.api_id)))
ORDER BY pre_table.lastmod ASC",
        params.parlament.map(|p| p.to_string()),
        params.wp.map(|x| i32::try_from(x).map_err(|e| LTZFError::Other {
            message: format!(
                "Failed to convert sitzung wahlperiode as it exceeds i32::MAX: {e}"
            )
        }))
        .transpose()?,
        params.since,
        params.until,
        params.gremium_like,
        params.vgid
    )
    .map(|r| r.id)
    .fetch_all(&mut **tx)
    .await?;
    let prp = PaginationResponsePart::new(
        u32::try_from(as_list.len()).map_err(|e| LTZFError::Other {
            message: format!(
                "Result length cannot be converted to i32, as it exceed i32::MAX: {e}"
            ),
        })?,
        page,
        per_page,
    );
    if let Some(p) = page
        && p > prp.x_page
    {
        return Ok((prp, vec![]));
    }
    if as_list.is_empty() {
        return Ok((prp, vec![]));
    }

    let as_list = as_list.drain(prp.start()..prp.end());
    let mut vector = Vec::with_capacity(as_list.len());
    for id in as_list {
        vector.push(super::retrieve::sitzung_by_id(id, expand, tx).await?);
    }
    Ok((prp, vector))
}

#[derive(Debug)]
pub struct VGGetParameters {
    pub lower_date: Option<chrono::DateTime<chrono::Utc>>,
    pub upper_date: Option<chrono::DateTime<chrono::Utc>>,
    pub parlament: Option<models::Parlament>,
    pub wp: Option<i32>,
    pub inipsn: Option<String>,
    pub iniorg: Option<String>,
    pub inifch: Option<String>,
    pub vgtyp: Option<models::Vorgangstyp>,
}
/// returns (total number of available elements, chosen elements)
pub(crate) async fn vorgang_by_parameter(
    params: VGGetParameters,
    page: Option<u32>,
    per_page: Option<u32>,
    expand: bool,
    executor: &mut sqlx::PgTransaction<'_>,
) -> Result<(PaginationResponsePart, Vec<models::Vorgang>)> {
    let mut vg_list = sqlx::query!(
        "WITH pre_table AS (
        SELECT vorgang.id, MAX(ext_stat.zp_start) as lastmod FROM vorgang
            INNER JOIN vorgangstyp vt ON vt.id = vorgang.typ
            LEFT JOIN (SELECT s.vg_id, parlament.value as parl, s.zp_start FROM station s
            INNER JOIN gremium g ON g.id = s.gr_id
			INNER JOIN parlament on parlament.id = g.parl) AS ext_stat ON ext_stat.vg_id = vorgang.id
            WHERE TRUE
            AND ($1::int4 IS NULL OR $1 = vorgang.wahlperiode)
            AND ($2::text IS NULL OR $2 = vt.value)
            AND ($3::text IS NULL OR $3 = ext_stat.parl)
			AND ($4::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.person ILIKE CONCAT('%',$4::text,'%') AND rvi.vg_id = vorgang.id))
			AND ($5::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.organisation  ILIKE CONCAT('%',$5::text,'%') AND rvi.vg_id = vorgang.id))
			AND ($6::text IS NULL OR EXISTS(SELECT 1 FROM rel_vorgang_init rvi INNER JOIN autor a ON a.id = rvi.in_id WHERE a.fachgebiet  ILIKE CONCAT('%',$6::text,'%') AND rvi.vg_id = vorgang.id))
        GROUP BY vorgang.id
        ORDER BY lastmod
        )
SELECT * FROM pre_table WHERE
lastmod > COALESCE($7::timestamptz, '1940-01-01T20:20:20Z') 
AND lastmod < COALESCE($8, NOW())
ORDER BY pre_table.lastmod ASC
",params.wp, params.vgtyp.map(|x|x.to_string()),
params.parlament.map(|p|p.to_string()),
params.inipsn, params.iniorg, params.inifch,
params.lower_date, params.upper_date)
    .map(|r|r.id)
    .fetch_all(&mut **executor).await?;
    let prp = PaginationResponsePart::new(
        u32::try_from(vg_list.len()).map_err(|e| LTZFError::Other {
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
        vector.push(super::retrieve::vorgang_by_id(id, expand, executor).await?);
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
