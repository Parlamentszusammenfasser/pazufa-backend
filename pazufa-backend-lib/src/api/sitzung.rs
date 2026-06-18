use super::RoundTimestamp;
use super::auth::APIScope;
use super::find_applicable_date_range;
use crate::api::{GetSingleResponse, PutResponse, auth::Claims};
use crate::db::retrieve::{SitzungFilterParameters, sitzung_by_param};
use crate::db::{delete, insert, retrieve};
use crate::error::LTZFError;
use crate::models::params;
use crate::models::params::XScraperId;
use crate::{LTZFServer, Result, models};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::Datelike;
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
#[derive(Debug, IntoResponses)]
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

/// Deletes a session by it's api id
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields(sid=%path_params.api_id))]
pub async fn sitzung_delete(
    server: &LTZFServer,
    path_params: &params::ApiIdPathParam,
) -> Result<()> {
    delete::delete_sitzung_by_api_id(path_params.api_id, server).await?;
    info!(target: "obj", "Deleted Sitzung {}", path_params.api_id);
    info!("Success");
    Ok(())
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
    server: &LTZFServer,
    claims: &Claims,
    scraper_id: &XScraperId,
    path_params: &params::KalDatePutPathParams,
    body: &[models::Sitzung],
) -> Result<()> {
    let last_upd_day = chrono::Utc::now()
        .date_naive()
        .checked_sub_days(chrono::Days::new(1))
        .ok_or_else(|| LTZFError::Other {
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
        .ok_or_else(|| LTZFError::Other {
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
    server: &LTZFServer,
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
        let db_cmpvg = retrieve::sitzung_by_id(db_id, &mut tx).await?;
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
        delete::delete_sitzung_by_api_id(api_id, server).await?;
        insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, server).await?;
    } else {
        insert::insert_sitzung(body, Uuid::nil(), claims.1, &mut tx, server).await?;
    }
    tx.commit().await?;
    info!(target: "obj", "PUT Sitzung {}", api_id);
    info!("Successfully PUT session into database");
    Ok(PutResponse::Success)
}

/// GETs a day of sitzungen according to the other filter parameters.
/// # Errors
/// Can fail due to database errors or date constraints
#[instrument(skip_all, fields(date=%path_params.datum))]
pub async fn kal_date_get(
    server: &LTZFServer,
    header_params: &params::IfModSince,
    path_params: &params::KalDateGetPathParams,
    query_params: &params::KalDateGetQueryParams,
) -> Result<SitzungListingResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    let dr = find_applicable_date_range(
        Some(path_params.datum.year()),
        Some(path_params.datum.month()),
        Some(path_params.datum.day()),
        None,
        None,
        header_params.if_modified_since,
    );
    let Some(dr) = dr else {
        info!("Date Range too narrow or invalid");
        return Ok(SitzungListingResponse::NoContent);
    };

    let dt_begin = dr.since;
    let dt_end = dr.until;
    let result = sitzung_by_param(
        &SitzungFilterParameters {
            parlament: Some(path_params.parlament),
            gremium_like: None,
            since: dt_begin,
            until: dt_end,
            vgid: None,
            wp: None,
        },
        query_params.page,
        query_params.per_page,
        &mut tx,
    )
    .await?;

    if result.1.is_empty() {
        tx.rollback().await?;
        info!("No Sitzungen found in date range {}", dr);
        return Ok(SitzungListingResponse::NoContent);
    }
    tx.commit().await?;

    let prp = &result.0;
    info!("Successfully fetched Sitzungen");
    Ok(SitzungListingResponse::Success {
        body: result.1,
        link: prp.generate_link_header(&format!(
            "/api/v2/kalender/{}/{}",
            path_params.parlament, path_params.datum
        )),
        x_page: prp.x_page as u32,
        x_per_page: prp.x_per_page as u32,
        x_total_count: prp.x_total_count as u32,
        x_total_pages: prp.x_total_pages as u32,
    })
}

/// TODO: unify `kal_get` and `kal_date_get` by utilising `sitzung_retrieve_by_param`
/// find a way to implement pagination and the prp here
///
/// Retrieves a filtered list of sessions
/// # Errors
/// Can fail due to database constraints or incorrectly supplied parameters (i.e. negative pages,
/// ...)
#[doc = "`KalGet` - GET /api/v2/kalender"]
#[instrument(skip_all)]
pub async fn kal_get(
    server: &LTZFServer,
    header_params: &params::IfModSince,
    query_params: &params::KalGetQueryParams,
) -> Result<SitzungListingResponse> {
    let qparams = query_params;
    let hparams = header_params;
    let mut tx = server.sqlx_db.begin().await?;
    let result = find_applicable_date_range(
        qparams.y,
        qparams.m,
        qparams.dom,
        qparams.since,
        qparams.until,
        hparams.if_modified_since,
    );
    let Some(result) = result else {
        warn!(
            "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
            query_params, header_params.if_modified_since
        );
        return Ok(SitzungListingResponse::BadReq);
    };

    let params = retrieve::SitzungFilterParameters {
        gremium_like: qparams.gr.clone(),
        parlament: qparams.p,
        vgid: None,
        wp: qparams.wp,
        since: result.since,
        until: result.until,
    };

    // retrieval
    let result =
        retrieve::sitzung_by_param(&params, query_params.page, query_params.per_page, &mut tx)
            .await?;
    if result.1.is_empty() && header_params.if_modified_since.is_none() {
        info!("No Sitzungen found");
        Ok(SitzungListingResponse::NoContent)
    } else if result.1.is_empty() && header_params.if_modified_since.is_some() {
        info!("All results remain unchanged");
        Ok(SitzungListingResponse::NotMod)
    } else {
        tx.commit().await?;
        let prp = &result.0;
        info!("{} Sitzungen retrieved", result.1.len());
        Ok(SitzungListingResponse::Success {
            body: result.1,
            x_total_count: prp.x_total_count as u32,
            x_total_pages: prp.x_total_pages as u32,
            x_page: prp.x_page as u32,
            x_per_page: prp.x_per_page as u32,
            link: prp.generate_link_header("/api/v2/kalender"),
        })
    }
}

/// Retrieves a sitzung by its `api_id`
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields(sid=%path_params.api_id))]
pub async fn s_get_by_id(
    server: &LTZFServer,
    header_params: &params::IfModSince,
    path_params: &params::ApiIdPathParam,
) -> Result<GetSingleResponse<models::Sitzung>> {
    let claims = (APIScope::Collector, 0);
    let mut tx = server.sqlx_db.begin().await?;
    let api_id = path_params.api_id;
    let id_exists = sqlx::query!("SELECT 1 as x FROM sitzung WHERE api_id = $1", api_id)
        .fetch_optional(&mut *tx)
        .await?;
    if id_exists.is_none() {
        info!("Sitzung does not exist");
        return Ok(GetSingleResponse::NotFound);
    }

    let id = sqlx::query!(
        "SELECT id FROM sitzung WHERE api_id = $1
        AND last_update > COALESCE($2::timestamptz, '1940-01-01T00:00:00');",
        api_id,
        header_params.if_modified_since
    )
    .map(|r| r.id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = id {
        let mut result = retrieve::sitzung_by_id(id, &mut tx).await?;
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
        tx.commit().await?;
        info!("Success");
        Ok(GetSingleResponse::Success(Box::new(result)))
    } else if let Some(ims) = header_params.if_modified_since {
        info!("Success, but not modified since {}", ims);
        Ok(GetSingleResponse::NotMod)
    } else {
        unreachable!("Session ID was not found a second time despite if_modified_since being None. This might indicate a grave database state error.\n
            Call Parameters: {:?}, {:?}", path_params, header_params);
    }
}

/// Get Sitzung objects by filters
/// # Errors
/// Can fail due to:
/// - Database Errors
/// - Date constraints
/// - Filter constraints
#[instrument(skip_all)]
pub async fn s_get(
    server: &LTZFServer,
    header_params: &params::IfModSince,
    query_params: &params::SGetQueryParams,
) -> Result<SitzungListingResponse> {
    let range = find_applicable_date_range(
        None,
        None,
        None,
        query_params.since,
        query_params.until,
        header_params.if_modified_since,
    );
    let Some(range) = range else {
        warn!(
            "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
            query_params, header_params.if_modified_since
        );
        return Ok(SitzungListingResponse::BadReq);
    };
    let params = retrieve::SitzungFilterParameters {
        gremium_like: query_params.gr.clone(),
        parlament: query_params.p,
        wp: query_params.wp,
        since: range.since,
        until: range.until,
        vgid: query_params.vgid,
    };

    let mut tx: sqlx::Transaction<'_, sqlx::Postgres> = server.sqlx_db.begin().await?;
    let result =
        retrieve::sitzung_by_param(&params, query_params.page, query_params.per_page, &mut tx)
            .await?;
    let prp = result.0;
    tx.commit().await?;
    if result.1.is_empty() && header_params.if_modified_since.is_none() {
        info!("No Content found matching the date criteria");
        Ok(SitzungListingResponse::NoContent)
    } else if let Some(ims) = header_params.if_modified_since
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
            link: prp.generate_link_header("/api/v2/sitzung"),
        })
    }
}

#[cfg(test)]
mod sitzung_test {
    use crate::models::params;
    use chrono::Utc;
    use tracing::info;
    use tracing_test::traced_test;

    use crate::models;
    use chrono::Datelike;
    use uuid::Uuid;

    use crate::api::{GetSingleResponse, PutResponse, RoundTimestamp, auth::Claims};

    use crate::utils::testing::{TestSetup, generate};

    use super::super::auth;

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
            &Claims(auth::APIScope::Collector, 1),
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
            &Claims(auth::APIScope::Admin, 1),
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
            &Claims(auth::APIScope::Collector, 1),
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
            &Claims(auth::APIScope::Collector, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum: today,
                parlament,
            },
            std::slice::from_ref(&recent_session),
        )
        .await
        .expect("Failed to update calendar entry with recent session");

        let response = super::kal_date_get(
            server,
            &params::IfModSince {
                if_modified_since: None,
            },
            &params::KalDateGetPathParams {
                datum: today,
                parlament,
            },
            &params::KalDateGetQueryParams {
                page: None,
                per_page: None,
            },
        )
        .await
        .expect("Failed to get calendar entries");
        assert!(
            matches!(response, super::SitzungListingResponse::Success { body, .. } if body.len() == 1)
        );
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_kal_date_get() {
        let setup = TestSetup::new("kal_date_get").await;
        let server = &setup.server;
        let old_session = generate::default_sitzung();
        let session = models::Sitzung {
            termin: chrono::Utc::now(),
            ..old_session
        };
        let parlament = session.gremium.parlament;
        let today = session.termin.date_naive();

        super::kal_date_put(
            server,
            &Claims(auth::APIScope::Collector, 1),
            &params::XScraperId(Uuid::nil()),
            &params::KalDatePutPathParams {
                datum: session.termin.date_naive(),
                parlament,
            },
            std::slice::from_ref(&session),
        )
        .await
        .expect("Failed to create calendar entry");

        // 2. Get calendar entry for non-existent date
        let response = super::kal_date_get(
            server,
            &params::IfModSince {
                if_modified_since: None,
            },
            &params::KalDateGetPathParams {
                datum: chrono::NaiveDate::from_ymd_opt(1950, 10, 10)
                    .expect("Failed to create date from year, month, day"),
                parlament,
            },
            &params::KalDateGetQueryParams {
                page: None,
                per_page: None,
            },
        )
        .await
        .expect("Failed to get calendar entry for non-existent date");
        assert!(matches!(response, super::SitzungListingResponse::NoContent));

        // successful GET
        let response = super::kal_date_get(
            server,
            &params::IfModSince {
                if_modified_since: None,
            },
            &params::KalDateGetPathParams {
                datum: today,
                parlament,
            },
            &params::KalDateGetQueryParams {
                page: None,
                per_page: None,
            },
        )
        .await
        .expect("Failed to get calendar entry");
        let normalized_session = super::st_to_uuiddoks(&session).with_round_timestamps();
        info!("expected: {:?}", vec![normalized_session.clone()]);
        if let super::SitzungListingResponse::Success { ref body, .. } = response {
            info!("actual  : {:?}", body);
        }
        assert!(matches!(
            response,
            super::SitzungListingResponse::Success { body, .. }
            if body.iter().map(super::super::RoundTimestamp::with_round_timestamps).collect::<Vec<_>>() == vec![normalized_session]
        ));

        // see if if-mod-since works
        let response = super::kal_date_get(
            server,
            &params::IfModSince {
                if_modified_since: Some(chrono::Utc::now()),
            },
            &params::KalDateGetPathParams {
                datum: today,
                parlament,
            },
            &params::KalDateGetQueryParams {
                page: None,
                per_page: None,
            },
        )
        .await
        .expect("Failed to get calendar entry with if-modified-since header");
        assert!(
            matches!(response, super::SitzungListingResponse::NoContent),
            "Expected 204, got {response:?}"
        );

        // Cleanup
        setup.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_kal_get() {
        let setup = TestSetup::new("kal_get").await;
        let server = &setup.server;
        let session = generate::default_sitzung();
        let parlament = session.gremium.parlament;
        let date = session.termin;

        let response = super::sid_put(
            server,
            &Claims(auth::APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: Uuid::nil(),
            },
            &session.clone(),
        )
        .await
        .expect("Failed to create session");
        assert!(matches!(response, PutResponse::Success));

        // Test cases for kal_get:
        // 1. Get calendar entries with valid parameters
        {
            let response = super::kal_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::KalGetQueryParams {
                    page: None,
                    per_page: None,
                    y: Some(date.year()),
                    m: Some(date.month()),
                    dom: None,
                    gr: None,
                    p: Some(parlament),
                    since: None,
                    until: None,
                    wp: Some(session.gremium.wahlperiode),
                },
            )
            .await
            .expect("Failed to get calendar entries with valid parameters");
            match &response {
                super::SitzungListingResponse::Success { body, .. } => {
                    assert!(
                        !body.is_empty(),
                        "Expected 204 no content, got 200 OK with empty body"
                    );
                }
                _ => panic!(
                    "Expected to find sessions with valid filters, got: {:?}",
                    &response
                ),
            }
        }

        // 2. Get calendar entries with invalid parameters (since > until)
        {
            let response = super::kal_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::KalGetQueryParams {
                    page: None,
                    per_page: None,
                    y: None,
                    m: None,
                    dom: None,
                    gr: None,
                    p: None,
                    since: Some(chrono::Utc::now()),
                    until: Some(chrono::Utc::now() - chrono::Duration::days(1)), // until is before since
                    wp: None,
                },
            )
            .await
            .expect("Failed to get calendar entries with invalid parameters");
            assert!(
                matches!(response, super::SitzungListingResponse::BadReq),
                "Got: {response:?}"
            );
        }

        // 3. Get calendar entries with date range
        {
            let start_date = session
                .termin
                .checked_sub_days(chrono::Days::new(1))
                .expect("Failed to subtract days from session date");
            let end_date = session
                .termin
                .checked_add_days(chrono::Days::new(1))
                .expect("Failed to add days to session date");
            let response = super::kal_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::KalGetQueryParams {
                    page: None,
                    per_page: None,
                    y: None,
                    m: None,
                    dom: None,
                    gr: None,
                    p: Some(parlament),
                    since: Some(start_date),
                    until: Some(end_date),
                    wp: None,
                },
            )
            .await
            .expect("Failed to get calendar entries with date range");
            match response {
                super::SitzungListingResponse::Success { body, .. } => {
                    assert!(!body.is_empty(), "Expected to find sessions in date range");
                    for session in body {
                        assert!(
                            session.termin >= start_date && session.termin <= end_date,
                            "Found session outside requested date range"
                        );
                    }
                }
                _ => panic!("Expected to find sessions in date range"),
            }
        }

        let response = super::kal_get(
            server,
            &params::IfModSince {
                if_modified_since: Some(
                    chrono::Utc::now()
                        .checked_add_days(chrono::Days::new(1))
                        .expect("Failed to add days to current date"),
                ),
            },
            &params::KalGetQueryParams {
                page: None,
                per_page: None,
                y: None,
                m: None,
                dom: None,
                gr: None,
                p: Some(parlament),
                since: None,
                until: None,
                wp: None,
            },
        )
        .await
        .expect("Failed to get calendar entries with if-modified-since header");
        assert!(
            matches!(response, super::SitzungListingResponse::NotMod),
            "{response:?}"
        );

        let response = super::kal_get(
            server,
            &params::IfModSince {
                if_modified_since: None,
            },
            &params::KalGetQueryParams {
                page: None,
                per_page: None,
                y: None,
                m: None,
                dom: None,
                gr: None,
                p: Some(parlament),
                since: Some(
                    chrono::Utc::now()
                        .checked_add_days(chrono::Days::new(1))
                        .expect("Failed to add days to current date"),
                ),
                until: None,
                wp: None,
            },
        )
        .await
        .expect("Failed to get calendar entries with future date range");
        assert!(matches!(response, super::SitzungListingResponse::NoContent));

        setup.teardown().await;
    }

    #[tokio::test]
    pub(crate) async fn test_session_get_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_session_get").await;
        let server = &scenario.server;

        let test_session = generate::default_sitzung();
        // First create the session
        let response = super::sid_put(
            server,
            &Claims(auth::APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: test_session
                    .api_id
                    .expect("Failed to get API ID from test session"),
            },
            &test_session,
        )
        .await
        .expect("Failed to create session");
        assert!(matches!(response, PutResponse::Success));
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // Test cases for s_get_by_id:
        // 1. Get existing session
        {
            let response = super::s_get_by_id(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::ApiIdPathParam {
                    api_id: test_session
                        .api_id
                        .expect("Failed to get API ID from test session"),
                },
            )
            .await
            .expect("Failed to get existing session");
            match response {
                GetSingleResponse::Success(body) => {
                    assert_eq!(body.api_id, test_session.api_id);
                    assert_eq!(body.titel, test_session.titel);
                }
                _ => panic!("Expected successful operation response"),
            }
        }

        // 2. Get non-existent session
        {
            let non_existent_id = Uuid::now_v7();
            let response = super::s_get_by_id(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::ApiIdPathParam {
                    api_id: non_existent_id,
                },
            )
            .await
            .expect("Failed to get non-existent session");
            assert!(matches!(response, GetSingleResponse::NotFound));
        }

        // 3. Get session with invalid ID
        {
            let invalid_id = Uuid::nil();
            let response = super::s_get_by_id(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::ApiIdPathParam { api_id: invalid_id },
            )
            .await
            .expect("Failed to get session with invalid ID");
            assert!(matches!(response, GetSingleResponse::NotFound));
        }

        // Test if-modified-since header
        {
            let response = super::s_get_by_id(
                server,
                &params::IfModSince {
                    if_modified_since: Some(chrono::Utc::now()),
                },
                &params::ApiIdPathParam {
                    api_id: test_session
                        .api_id
                        .expect("Failed to get API ID from test session"),
                },
            )
            .await
            .expect("Failed to get session with if-modified-since header");
            assert!(matches!(response, GetSingleResponse::NotMod));
        }

        // Test cases for s_get:
        // 1. Get sessions with valid parameters
        {
            let response = super::s_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::SGetQueryParams {
                    page: None,
                    gr: None,
                    per_page: None,
                    p: Some(test_session.gremium.parlament),
                    since: None,
                    until: None,
                    wp: Some(20),
                    vgid: None,
                    vgtyp: None,
                },
            )
            .await
            .expect("Failed to get sessions with valid parameters");
            assert!(
                matches!(response, super::SitzungListingResponse::Success { body, ..} if !body.is_empty())
            );
        }

        // 2. Get sessions with invalid parameters
        {
            let response = super::s_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::SGetQueryParams {
                    page: None,
                    per_page: None,
                    gr: None,
                    p: None,
                    since: Some(Utc::now()),
                    until: Some(Utc::now() - chrono::Duration::days(365)),
                    wp: None,
                    vgid: None,
                    vgtyp: None,
                },
            )
            .await
            .expect("Failed to get sessions with invalid parameters");
            assert!(matches!(response, super::SitzungListingResponse::BadReq));
        }

        // 3. Get sessions with filters
        {
            let response = super::s_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::SGetQueryParams {
                    page: None,
                    per_page: None,
                    p: Some(test_session.gremium.parlament),
                    since: None,
                    gr: None,
                    until: None,
                    wp: Some(20),
                    vgid: None,
                    vgtyp: None,
                },
            )
            .await
            .expect("Failed to get sessions with filters");
            assert!(
                matches!(response, super::SitzungListingResponse::Success { body, ..} if !body.is_empty())
            );
        }

        // Test if-modified-since header for s_get
        {
            let response = super::s_get(
                server,
                &params::IfModSince {
                    if_modified_since: Some(chrono::Utc::now()),
                },
                &params::SGetQueryParams {
                    page: None,
                    per_page: None,
                    p: Some(models::Parlament::Bt),
                    since: None,
                    gr: None,
                    until: None,
                    wp: Some(20),
                    vgid: None,
                    vgtyp: None,
                },
            )
            .await
            .expect("Failed to get sessions with if-modified-since header");
            assert!(matches!(response, super::SitzungListingResponse::NotMod));
        }

        // Test getting sessions with different wahlperiode
        {
            let response = super::s_get(
                server,
                &params::IfModSince {
                    if_modified_since: None,
                },
                &params::SGetQueryParams {
                    page: None,
                    gr: None,
                    per_page: None,
                    p: Some(models::Parlament::Bt),
                    since: None,
                    until: None,
                    wp: Some(22),
                    vgid: None,
                    vgtyp: None,
                },
            )
            .await
            .expect("Failed to get sessions with different wahlperiode");
            assert!(matches!(response, super::SitzungListingResponse::NoContent));
        }

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
            &Claims(auth::APIScope::Admin, 1),
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
            &Claims(auth::APIScope::Admin, 1),
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
            &Claims(auth::APIScope::Admin, 1),
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
    async fn test_session_delete() {
        let scenario = TestSetup::new("test_session_delete").await;
        let server = &scenario.server;
        let sitzung = generate::random::sitzung(12);

        // Test case: Input non-existing session
        let response = super::sid_put(
            server,
            &Claims(auth::APIScope::Admin, 1),
            &params::ApiIdPathParam {
                api_id: sitzung
                    .api_id
                    .expect("Failed to get API ID from test session"),
            },
            &sitzung,
        )
        .await
        .expect("Failed to create non-existing session");
        assert!(matches!(response, PutResponse::Success));

        // Test cases for sitzung_delete:
        // Test case: Delete existing session with proper permissions
        {
            super::sitzung_delete(
                server,
                &params::ApiIdPathParam {
                    api_id: sitzung
                        .api_id
                        .expect("Failed to get API ID from test session"),
                },
            )
            .await
            .expect("Failed to delete existing session");
        }

        // Test case: Delete non-existent session
        {
            super::sitzung_delete(
                server,
                &params::ApiIdPathParam {
                    api_id: sitzung
                        .api_id
                        .expect("Failed to get API ID from test session"),
                },
            )
            .await
            .expect("Failed to attempt to delete non-existent session");
        }

        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_malformed_req_data() {
        // dokumente uniqueness konflikt
    }
}
