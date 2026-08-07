use super::dokument::dokument_by_id;
use crate::api::PaginationResponsePart;
use crate::error::{LTZFError, Result};
use crate::models;
use std::str::FromStr;
use uuid::Uuid;

/// the crucial part is how to find out which vg are connected to a DRCKS
/// if there exists a station which contains a document mentioned in the top, its vorgang is connected
pub async fn top_by_id(
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

pub async fn sitzung_by_id(
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
pub async fn sitzung_by_parameter(
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
        vector.push(sitzung_by_id(id, expand, tx).await?);
    }
    Ok((prp, vector))
}
