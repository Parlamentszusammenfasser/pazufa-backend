use super::RoundTimestamp;
use crate::api::{GetSingleResponse, PutResponse, auth::Claims};
use crate::db::retrieve::sitzung_dbid;
use crate::db::{delete, insert, retrieve};
use crate::error::PazufaError;
use crate::models::params::XScraperId;
use crate::models::{APIScope, params};
use crate::{PazufaServer, Result, models};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use tracing::{debug, info, instrument, warn};
use utoipa::IntoResponses;
use uuid::Uuid;

// helper that converts the documents in a sitzung into just their uuids instead of full objects
fn st_to_uuiddoks(st: &models::Sitzung) -> models::Sitzung {
    use models::DokumentOrApiId;

    let mut st = st.clone();

    let all_docs = st
        .tops
        .iter_mut()
        .flat_map(|t| t.dokumente.iter_mut())
        .chain(st.dokumente.iter_mut());

    for d in all_docs {
        if let DokumentOrApiId::Dokument(dk) = d {
            if let Some(api_id) = dk.api_id {
                *d = DokumentOrApiId::ApiId(api_id);
            } else {
                unreachable!(
                    "This function should only ever be used on retrieved data, which must have api ids"
                )
            }
        }
    }

    st
}

/// Is the response to any of the sitzung GET endpoints that return a listing of some kind
/// more specifically: `s_get`, `kal_date_get`, `kal_get`
#[derive(Debug, IntoResponses, PartialEq)]
pub enum SitzungListingResponse {
    #[response(status = 200, content_type = "application/json")]
    Success {
        body: Vec<models::Sitzung>,
        x_page: u32,
        x_per_page: u32,
        x_total_count: u32,
        x_total_pages: u32,
        link: String,
    },
    #[response(status = 204)]
    NoContent,
    #[response(status = 304)]
    NotMod,
    #[response(status = 416)]
    RangeNotSat,
    #[response(status = 400)]
    BadReq,
    #[response(status = 500)]
    Error(String),
}

impl IntoResponse for SitzungListingResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::RangeNotSat => StatusCode::RANGE_NOT_SATISFIABLE.into_response(),
            Self::NotMod => StatusCode::NOT_MODIFIED.into_response(),
            Self::NoContent => StatusCode::NO_CONTENT.into_response(),
            Self::Success {
                body,
                x_page,
                x_per_page,
                x_total_count,
                x_total_pages,
                link,
            } => {
                let mut hm = HeaderMap::new();
                let Ok(link) = HeaderValue::from_str(&link) else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                hm.insert("link", link);
                hm.insert("x-per-page", x_per_page.into());
                hm.insert("x-page", x_page.into());
                hm.insert("x-total-pages", x_total_pages.into());
                hm.insert("x-total-count", x_total_count.into());

                (StatusCode::OK, hm, Json(body)).into_response()
            }
        }
    }
}

/// Inserts a day's worth of sessions into the database.
/// This replaces the whole day (of this parliament), and does not merge objects
/// Only Admin level users may insert into any day, collectors are only allowed to update a day into
/// the past. This is to prevent data loss, if parliaments take down old sessions from their website
///
/// # Errors
/// Can fail due to database errors or date constraints
#[instrument(skip_all, fields(claim=%claims.0, date=%path_params.datum))]
pub async fn kal_date_put(
    server: &PazufaServer,
    claims: &Claims,
    scraper_id: &XScraperId,
    path_params: &params::KalDatePutPathParams,
    body: &[models::Sitzung],
) -> Result<()> {
    let last_upd_day = chrono::Utc::now()
        .date_naive()
        .checked_sub_days(chrono::Days::new(1))
        .ok_or_else(|| PazufaError::Other {
            message: "Date::now() out of bounds.".to_string(),
        })?;
    if claims.0 == APIScope::Collector && path_params.datum < last_upd_day {
        warn!(
            "Permission not Granted because you are only a collector and {} < {last_upd_day}",
            path_params.datum,
        );
        return Ok(());
    }
    let len = body.len();
    let body: Vec<_> = body
        .iter()
        .filter(|&f| {
            f.termin.date_naive() >= last_upd_day && f.gremium.parlament == path_params.parlament
        })
        .collect();

    if len < body.len() {
        debug!(
            "Filtered {}/{} Sitzungen due to date and parlament equality constraints",
            len.wrapping_sub(body.len()),
            len
        );
    }
    if len == 0 {
        warn!("Body was empty");
        return Ok(());
    }

    let mut tx = server.sqlx_db.begin().await?;

    let dt_begin = path_params
        .datum
        .and_time(chrono::NaiveTime::default())
        .and_utc();
    let dt_end = path_params
        .datum
        .checked_add_days(chrono::Days::new(1))
        .ok_or_else(|| PazufaError::Other {
            message: "Parameter `datum` out of bounds.".to_string(),
        })?
        .and_time(chrono::NaiveTime::default())
        .and_utc();
    // delete all entries that fit the description
    debug!("Deleting entries from {dt_begin} until {dt_end}");
    sqlx::query!(
        "DELETE FROM sitzung WHERE sitzung.id = ANY(SELECT s.id FROM sitzung s 
        INNER JOIN gremium g ON g.id=s.gr_id 
        INNER JOIN parlament p ON p.id=g.parl 
        WHERE p.value = $1 AND s.termin BETWEEN $2 AND $3)",
        path_params.parlament.to_string(),
        dt_begin,
        dt_end
    )
    .execute(&mut *tx)
    .await?;

    // insert all entries
    for s in &body {
        insert::insert_sitzung(s, scraper_id.0, claims.1, &mut tx, server).await?;
    }
    tx.commit().await?;
    info!(target: "obj", "Inserted sitzungen into db: {:?}", body);
    info!("Inserted {} sessions into the database", body.len());
    Ok(())
}

/// PUTs a `models::Sitzung` into the database with checks on whether
/// the objects modifies internal state.
/// NOTE: Documents that are referenced by UUID (within body.dokumente)
/// and point to a document that is not in the database are silently
/// filtered out.
/// # Errors
/// Can fail due to database errors
#[doc = "`SidPut` - PUT /api/v2/sitzung/{sid}"]
#[instrument(skip_all, fields(claim=%claims.0, sid=%path_params.api_id))]
pub async fn sid_put(
    server: &PazufaServer,
    claims: &Claims,
    path_params: &params::ApiIdPathParam,
    body: &models::Sitzung,
) -> Result<PutResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    let api_id = path_params.api_id;
    let db_id = sqlx::query!("SELECT id FROM sitzung WHERE api_id = $1", api_id)
        .map(|x| x.id)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(db_id) = db_id {
        let db_cmpvg = retrieve::sitzung_by_id(db_id, false, &mut tx).await?;
        debug!(
            "odb: {}\nonew: {}",
            serde_json::to_string(&db_cmpvg.with_round_timestamps())
                .unwrap_or_else(|_| format!("{:?}", db_cmpvg.with_round_timestamps())),
            serde_json::to_string(&st_to_uuiddoks(body).with_round_timestamps())
                .unwrap_or_else(|_| format!("{:?}", st_to_uuiddoks(body).with_round_timestamps()))
        );
        if db_cmpvg.with_round_timestamps() == st_to_uuiddoks(body).with_round_timestamps() {
            info!("Sitzung has the same state as the input object");
            return Ok(PutResponse::NotMod);
        }
        delete::delete_sitzung_by_api_id(api_id, &mut *tx).await?;
        insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, server).await?;
    } else {
        insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, server).await?;
    }
    tx.commit().await?;
    info!(target: "obj", "PUT Sitzung {}", api_id);
    info!("Successfully PUT session into database");
    Ok(PutResponse::Success)
}

/// Retrieves a sitzung by its `api_id`
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields(sid=%path_params.api_id))]
pub async fn s_get_by_id(
    server: &PazufaServer,
    header_params: &params::IfModSince,
    path_params: &params::ApiIdPathParam,
    expand: bool,
) -> Result<GetSingleResponse<models::Sitzung>> {
    let claims = (APIScope::Collector, 0);
    let mut tx = server.sqlx_db.begin().await?;
    let api_id = path_params.api_id;

    let Some(id) = sitzung_dbid(api_id, &mut tx).await? else {
        info!("Sitzung does not exist");
        return Ok(GetSingleResponse::NotFound);
    };

    let Some(mut result) =
        retrieve::sitzung_by_id_ims(id, expand, header_params.if_modified_since, &mut tx).await?
    else {
        info!(
            "Success, but not modified since {:?}",
            header_params.if_modified_since
        );
        return Ok(GetSingleResponse::NotMod);
    };

    if claims.0 == APIScope::KeyAdder || claims.0 == APIScope::Admin {
        result.touched_by = sqlx::query!(
            "SELECT * FROM scraper_touched_sitzung sts
                    INNER JOIN api_keys ON api_keys.id = sts.collector_key
                    WHERE sid = $1",
            id
        )
        .map(|r| models::TouchedByInner {
            key: Some(r.key_hash),
            scraper_id: Some(r.scraper),
        })
        .fetch_all(&mut *tx)
        .await?;
    }
    info!("Success");
    Ok(GetSingleResponse::Success(Box::new(result)))
}

/// Retrieves Sessions from the database and does basic HTTP-Returncode Checks
/// # Errors
/// Can fail due to:
pub async fn get_sitzung_by_parameter(
    params: retrieve::SitzungFilterParameters,
    page: Option<u32>,
    per_page: Option<u32>,
    expand: bool,
    link_prefix: &str,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<SitzungListingResponse> {
    let result = retrieve::sitzung_by_parameter(&params, page, per_page, expand, tx).await?;
    let prp = result.0;
    if result.1.is_empty() && params.if_mod_since.is_none() {
        info!("No Content found matching the date criteria");
        Ok(SitzungListingResponse::NoContent)
    } else if let Some(ims) = params.if_mod_since
        && result.1.is_empty()
    {
        info!("No Content found that was modified since {}", ims);
        Ok(SitzungListingResponse::NotMod)
    } else {
        info!("Successfully retrieved {} Sitzungen", result.1.len());
        Ok(SitzungListingResponse::Success {
            body: result.1,
            x_total_count: prp.x_total_count as u32,
            x_total_pages: prp.x_total_pages as u32,
            x_page: prp.x_page as u32,
            x_per_page: prp.x_per_page as u32,
            link: prp.generate_link_header(link_prefix),
        })
    }
}

#[cfg(test)]
mod sitzung_test {
    use crate::api::find_applicable_date_range;
    use crate::api::sitzung::get_sitzung_by_parameter;
    use crate::api::{GetSingleResponse, PutResponse, auth::Claims};
    use crate::models::{self, APIScope, params};
    use crate::utils::testing::{TestSetup, generate};
    use chrono::Datelike;
    use tracing_test::traced_test;
    use uuid::Uuid;

    // Calendar tests
    #[tokio::test]
    #[traced_test]
    async fn test_calendar_auth() {
        let scenario = TestSetup::new("test_calendar_auth").await;
        let server = &scenario.server;
        let test_date = chrono::Utc::now().date_naive();
        let test_session = generate::default_sitzung();

        let datum = test_date
            .checked_sub_days(chrono::Days::new(5))
            .expect("Failed to subtract days from current date");

        super::kal_date_put(
            server,
            &Claims(APIScope::Collector, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum,
                parlament: models::Parlament::Bt,
            },
            &[test_session],
        )
        .await
        .expect("error calling kal_date_put endpoint");
    }
    #[tokio::test]
    async fn sitzung_get_nocontent_on_page_overflow() {
        let scenario = TestSetup::new("sitzung_get_nocontent_page_overflow").await;
        let session = generate::default_sitzung();

        super::kal_date_put(
            &scenario.server,
            &Claims(APIScope::Collector, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum: session.termin.date_naive(),
                parlament: models::Parlament::Bt,
            },
            std::slice::from_ref(&session),
        )
        .await
        .expect("Failed to create calendar entry");

        let mut tx = scenario.server.sqlx_db.begin().await.unwrap();
        let response = super::get_sitzung_by_parameter(
            crate::db::retrieve::SitzungFilterParameters::default(),
            Some(500),
            None,
            false,
            "",
            &mut tx,
        )
        .await
        .expect("Failed to get calendar entries");
        assert_eq!(response, super::SitzungListingResponse::NoContent);
        scenario.teardown().await;
    }
    #[tokio::test]
    #[traced_test]
    async fn test_cal_date_put() {
        // Setup test server and database
        let scenario = TestSetup::new("test_cal_date_put").await;
        let server = &scenario.server;

        // Create test calendar entry
        let today = chrono::Utc::now().date_naive();
        let outdated_session = generate::default_sitzung();
        let recent_session = models::Sitzung {
            termin: chrono::Utc::now(),
            ..outdated_session.clone()
        };
        let parlament = recent_session.gremium.parlament;

        // Test cases for kal_date_put:
        // 1. Create calendar entry out of valid date range with admin permissions
        super::kal_date_put(
            server,
            &Claims(APIScope::Admin, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum: today,
                parlament,
            },
            std::slice::from_ref(&outdated_session),
        )
        .await
        .expect("Failed to create calendar entry with admin permissions");

        // 2. Update calendar entry with date constraints and fail (collector is only allowed to update recent dates)
        super::kal_date_put(
            server,
            &Claims(APIScope::Collector, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum: today,
                parlament,
            },
            std::slice::from_ref(&outdated_session),
        )
        .await
        .expect("Failed to update calendar entry with collector permissions");

        // Update calendar entry with date constraints and succeed
        super::kal_date_put(
            server,
            &Claims(APIScope::Collector, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum: today,
                parlament,
            },
            std::slice::from_ref(&recent_session),
        )
        .await
        .expect("Failed to update calendar entry with recent session");

        let range = find_applicable_date_range(
            Some(today.year()),
            Some(today.month()),
            Some(today.day()),
            None,
            None,
            None,
        )
        .unwrap();
        let mut tx = scenario.server.sqlx_db.begin().await.unwrap();
        let response = get_sitzung_by_parameter(
            crate::db::retrieve::SitzungFilterParameters {
                since: range.since,
                until: range.until,
                ..crate::db::retrieve::SitzungFilterParameters::default()
            },
            None,
            None,
            true,
            "",
            &mut tx,
        )
        .await
        .unwrap();
        assert!(
            matches!(response, super::SitzungListingResponse::Success { body, .. } if body.len() == 1)
        );
        scenario.teardown().await;
    }

    async fn insert_default_session(server: &crate::PazufaServer) {
        let session = generate::default_sitzung();
        let response = super::sid_put(
            server,
            &Claims(APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: Uuid::nil(),
            },
            &session.clone(),
        )
        .await;
        assert!(matches!(response, Ok(PutResponse::Success)));
    }

    #[tokio::test]
    async fn session_get_by_id() {
        // Setup test server and database
        let scenario = TestSetup::new("test_session_get_by_id").await;
        let server = &scenario.server;
        let session = generate::default_sitzung();
        insert_default_session(&scenario.server).await;

        // Test cases for s_get_by_id:
        // 1. Get existing session
        let response = super::s_get_by_id(
            server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam {
                api_id: session.api_id.expect("Failed"),
            },
            false,
        )
        .await
        .expect("Failed to get existing session");
        match response {
            GetSingleResponse::Success(body) => {
                assert_eq!(body.api_id, session.api_id);
                assert_eq!(body.titel, session.titel);
            }
            _ => panic!("Expected successful operation response"),
        }

        // 2. Get non-existent session
        let non_existent_id = Uuid::now_v7();
        let response = super::s_get_by_id(
            server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam {
                api_id: non_existent_id,
            },
            false,
        )
        .await
        .expect("Failed to get non-existent session");
        assert!(matches!(response, GetSingleResponse::NotFound));

        // 3. Get session with invalid ID
        let invalid_id = Uuid::nil();
        let response = super::s_get_by_id(
            server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id: invalid_id },
            false,
        )
        .await
        .expect("Failed to get session with invalid ID");
        assert!(matches!(response, GetSingleResponse::NotFound));

        // Test if-modified-since header
        let response = super::s_get_by_id(
            server,
            &params::IfModSince {
                if_modified_since: Some(chrono::Utc::now()),
            },
            &params::ApiIdPathParam {
                api_id: session.api_id.expect("Failed"),
            },
            false,
        )
        .await
        .expect("Failed to get session with if-modified-since header");
        assert!(matches!(response, GetSingleResponse::NotMod));

        // Cleanup
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_session_modify_update() {
        let scenario = TestSetup::new("test_session_modify_update").await;
        let server = &scenario.server;
        let sitzung = generate::random::sitzung(12);

        // Test case: Input non-existing session
        let response = super::sid_put(
            server,
            &Claims(APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: sitzung.api_id.expect("session API ID is none"),
            },
            &sitzung,
        )
        .await
        .expect("Failed to create non-existing session");
        assert!(matches!(response, PutResponse::Success));

        // Test case: Update existing session with the same data
        let response = super::sid_put(
            server,
            &Claims(APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: sitzung.api_id.expect("session API ID is none"),
            },
            &sitzung,
        )
        .await
        .expect("Failed to update session with same data");
        assert!(
            matches!(response, PutResponse::NotMod),
            "Failed to update existing session with the same data.\nInput:  {sitzung:?}\n\n Output: {response:?}",
        );

        // Test case: Update existing session with valid new data
        let rsp_new = models::Sitzung {
            link: Some("https://example.com/a/b/c".to_string()),
            ..sitzung.clone()
        };
        let response = super::sid_put(
            server,
            &Claims(APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: sitzung.api_id.expect("session API ID is none"),
            },
            &rsp_new,
        )
        .await
        .expect("Failed to update session with new data");
        assert!(matches!(response, PutResponse::Success));

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }

    #[tokio::test]
    async fn test_malformed_req_data() {
        // dokumente uniqueness konflikt
    }
}
