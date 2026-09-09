use super::PutResponse;
use super::find_applicable_date_range;
use crate::api::{GetSingleResponse, ListingGetResponse, RoundTimestamp};
use crate::db;
use crate::db::retrieve::vorgang_dbid;
use crate::db::{delete, insert, merge, retrieve};
use crate::error::{DataValidationError, LTZFError};
use crate::models::{APIScope, params};
use crate::{LTZFServer, Result, models};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

/// Deletes a single Vorgang
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields( vg_id=%path_params.api_id))]
pub async fn vorgang_delete(
    server: &LTZFServer,
    path_params: &params::ApiIdPathParam,
) -> Result<()> {
    db::delete::delete_vorgang_by_api_id(path_params.api_id, &server.sqlx_db).await?;
    info!(target: "obj", "Deleted Vorgang {}", path_params.api_id);
    Ok(())
}

/// Inserts a single vorgang, bypassing the merge algorithm.
/// If called, the state of the request body is then the exact state of the database
/// Only available to admin level permissions
///
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields( vg_id=%path_params.api_id))]
pub async fn vorgang_id_put(
    server: &LTZFServer,
    creator_index: i32,
    path_params: &params::ApiIdPathParam,
    body: &models::Vorgang,
) -> Result<PutResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    let api_id = path_params.api_id;
    let db_id = sqlx::query!("SELECT id FROM vorgang WHERE api_id = $1", api_id)
        .map(|x| x.id)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(db_id) = db_id {
        debug!("Match found: {db_id}");
        let db_cmpvg = retrieve::vorgang_by_id(db_id, false, &mut tx).await?;

        if body.with_round_timestamps() == db_cmpvg.with_round_timestamps() {
            return Ok(PutResponse::NotMod);
        }
        debug!("Found to be different enough to not be exactly the same. Deleting...");
        delete::delete_vorgang_by_api_id(api_id, &mut *tx).await?;
        debug!("Delete successful, now inserting...");
    } else {
        debug!("No Match found");
    }
    match insert::insert_vorgang(body, Uuid::nil(), creator_index, &mut tx, server).await {
        Ok(_) => {
            info!(target: "obj", "PUT by ID Vorgang {}", path_params.api_id);
            info!("Successful insert or replace");
            tx.commit().await?;
            Ok(PutResponse::Success)
        }
        Err(e) => match &e {
            LTZFError::Validation { source } => match &(**source) {
                DataValidationError::MalformedVorgang { reason } => {
                    tx.rollback().await?;
                    Ok(PutResponse::BadReq(reason.clone()))
                }
                _ => Err(e),
            },
            _ => Err(e),
        },
    }
}

/// Main endpoint collectors call to drop off their payloads.
///
/// # Errors
/// Can fail due to:
/// - Database Errors
/// - Ambiguous merges (Conflict)
/// - Errors in the merge process
#[instrument(skip_all, fields( scraper=%header_params.0))]
pub async fn vorgang_put(
    server: &LTZFServer,
    creator_index: i32,
    header_params: &params::XScraperId,
    body: &models::Vorgang,
) -> Result<PutResponse> {
    let rval = merge::execute::run_integration(body, header_params.0, creator_index, server).await;
    match rval {
        Ok(()) => {
            info!("Integration Successful");
            Ok(PutResponse::Success)
        }
        Err(e) => {
            warn!("Unsuccessful Integration Attempt: {e}");
            match &e {
                LTZFError::Validation { source } => match &(**source) {
                    DataValidationError::Conflict { .. }
                    | DataValidationError::AmbiguousMatch { .. } => Ok(PutResponse::Conflict),
                    DataValidationError::MalformedVorgang { reason } => {
                        Ok(PutResponse::BadReq(reason.clone()))
                    }
                    _ => Err(e),
                },
                _ => Err(e),
            }
        }
    }
}

/// Get a Vorgang by its `api_id`
/// used extensively, because documents are usually only listed as their ids, and this endpoint is
/// meant to fetch the function
///
/// # Errors
/// Can fail due to database errors
#[instrument(skip_all, fields(vg=%path_params.api_id))]
pub async fn vorgang_get_by_id(
    server: &LTZFServer,
    header_params: &params::IfModSince,
    path_params: &params::ApiIdPathParam,
    expand: bool,
) -> Result<GetSingleResponse<models::Vorgang>> {
    // TODO: find a better way to handle admin-only info for every object
    // in the past this was done by optional authentication, but that
    // turned out not to neatly fit into the oapi spec.
    // for now this is just a disabled feature
    let claims = (APIScope::Collector, 0);

    let mut tx = server.sqlx_db.begin().await?;
    let Some(dbid) = vorgang_dbid(path_params.api_id, &mut tx).await? else {
        info!(
            "Vorgang with API ID {} is not in the database",
            path_params.api_id
        );
        return Ok(GetSingleResponse::NotFound);
    };
    let Some(mut result) =
        retrieve::vorgang_by_id_ims(dbid, expand, header_params.if_modified_since, &mut tx).await?
    else {
        info!(
            "Successful Retrieval, was unchanged since {:?}",
            header_params.if_modified_since
        );
        return Ok(GetSingleResponse::NotMod);
    };
    if claims.0 == APIScope::Admin || claims.0 == APIScope::KeyAdder {
        result.touched_by = sqlx::query!(
            "SELECT * FROM scraper_touched_vorgang sts
                            INNER JOIN api_keys ON api_keys.id = sts.collector_key
                            WHERE vg_id = $1",
            dbid
        )
        .map(|r| models::TouchedByInner {
            key: Some(r.key_hash),
            scraper_id: Some(r.scraper),
        })
        .fetch_all(&mut *tx)
        .await?;
    }
    info!("Successful retrieval");
    tx.commit().await?;
    Ok(GetSingleResponse::Success(Box::new(result)))
}

/// Used to get a filtered list of Vorgang objects.
/// # Errors
/// Can fail due to database errors or invalid filter constraints
#[instrument(skip_all)]
pub async fn vorgang_get(
    server: &LTZFServer,
    header_params: &params::IfModSince,
    query_params: &params::VorgangGetQueryParams,
    expand: bool,
) -> Result<ListingGetResponse<models::Vorgang>> {
    let mut tx = server.sqlx_db.begin().await?;
    if let Some(range) = find_applicable_date_range(
        None,
        None,
        None,
        query_params.since,
        query_params.until,
        None,
    ) {
        let parameters = retrieve::VGGetParameters {
            lower_date: range.since,
            parlament: query_params.p,
            if_mod_since: header_params.if_modified_since,
            upper_date: range.until,
            vgtyp: query_params.vgtyp,
            wp: query_params.wp.map(i32::from),
            inifch: query_params.fach.clone(),
            iniorg: query_params.org.clone(),
            inipsn: query_params.person.clone(),
        };
        let result = retrieve::vorgang_by_parameter(
            parameters,
            query_params.page,
            query_params.per_page,
            expand,
            &mut tx,
        )
        .await?;
        if result.1.is_empty() {
            tx.rollback().await?;
            if let Some(if_modified_since) = header_params.if_modified_since {
                info!("No modification to result set since {}", if_modified_since);
                Ok(ListingGetResponse::NotMod)
            } else {
                info!(
                    "Parameters did not yield any content: {:?}, ims=None",
                    query_params
                );
                Ok(ListingGetResponse::NoContent)
            }
        } else {
            tx.commit().await?;
            let (prp, body) = result;
            info!("{} Objects matched query Parameters", body.len());
            Ok(ListingGetResponse::Success {
                body,
                prp,
                path_prefix: "/api/v2/vorgang",
            })
        }
    } else {
        tx.rollback().await?;
        warn!(
            "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
            query_params, header_params.if_modified_since
        );
        Ok(ListingGetResponse::BadReq)
    }
}

#[cfg(test)]
mod test_endpoints {
    use crate::api::{GetSingleResponse, ListingGetResponse, PutResponse};
    use crate::models::{self, params};
    use crate::utils::testing::{TestSetup, generate};
    use chrono::Utc;
    use uuid::Uuid;

    // Procedure (Vorgang) tests
    #[tokio::test]
    pub async fn test_vorgang_get_by_id_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_vorgang_by_id_get").await;
        let server = &scenario.server;
        let test_vorgang = generate::default_vorgang();
        // First create the procedure
        super::vorgang_put(
            server,
            1,
            &params::XScraperId(test_vorgang.api_id),
            &test_vorgang,
        )
        .await
        .expect("Failed to get valid server respone when trying to push new Vorgang.");

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        // Test cases for vorgang_get_by_id:
        // 1. Get existing procedure
        {
            let response = super::vorgang_get_by_id(
                server,
                &params::IfModSince::default(),
                &params::ApiIdPathParam {
                    api_id: test_vorgang.api_id,
                },
                false,
            )
            .await
            .expect(
                "Failed to get valid server respone when trying to get existing Vorgang by ID.",
            );
            match response {
                super::GetSingleResponse::Success(body) => {
                    assert_eq!(body.api_id, test_vorgang.api_id);
                    assert_eq!(body.titel, test_vorgang.titel);
                }
                super::GetSingleResponse::NotMod => panic!(
                    "Test server returned \"304: Not Modified\", even though \"if_modified_since\" was set to `None`."
                ),
                super::GetSingleResponse::NotFound => panic!(
                    "Test server returned \"404: Not Found\", even though Vorgang was just pushed with positive response code."
                ),
                _ => panic!("Unexpected response"),
            }
        }

        // 2. Get non-existent procedure
        {
            let non_existent_id = Uuid::now_v7();
            let response = super::vorgang_get_by_id(
                server,
                &params::IfModSince::default(),
                &params::ApiIdPathParam {
                    api_id: non_existent_id,
                },
                false,
            )
            .await
            .expect(
                "Failed to get valid server respone when trying to get existing Vorgang by ID.",
            );
            assert!(matches!(response, GetSingleResponse::NotFound));
        }

        // 3. Get procedure with invalid ID
        {
            let invalid_id = Uuid::nil();
            let response = super::vorgang_get_by_id(server,
                    &params::IfModSince::default(),
                    &params::ApiIdPathParam {
                        api_id: invalid_id,
                    },
                false,
                )
                .await
                .expect("Failed to get valid server respone when trying to get Vorgang by an invalid ID (nil).");
            assert!(matches!(response, GetSingleResponse::NotFound));
        }
        let response = super::vorgang_get_by_id(
            server,
                &params::IfModSince {
                    if_modified_since: Some(chrono::Utc::now()),
                },
                &params::ApiIdPathParam {
                    api_id: test_vorgang.api_id,
                },
                false,
            )
            .await
                .expect("Failed to get valid server respone when trying to get existing Vorgang by ID with update time limitations.");
        assert!(matches!(response, GetSingleResponse::NotMod));
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn vorgang_get_nocontent_on_page_overflow() {
        let scenario = TestSetup::new("vorgang_get_nocontent_page_overflow").await;
        let vorgang = generate::default_vorgang();
        super::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        let response = super::vorgang_get(
                &scenario.server,
                    &params::IfModSince::default(),
                    &params::VorgangGetQueryParams {
                        page: Some(500),
                        ..Default::default()
                    },
                false,
                )
                .await
                .expect("Failed to get valid server respone when trying to get existing Vorgang with invalid date range.");
        assert_eq!(response, ListingGetResponse::NoContent);
        scenario.teardown().await;
    }
    #[tokio::test]
    pub async fn test_vorgang_get_filtered_endpoints_empty() {
        let scenario = TestSetup::new("test_vorgang_get_filtered_empty").await;
        let server = &scenario.server;
        let test_vorgang = generate::default_vorgang();
        // First create the procedure
        super::vorgang_put(
            server,
            1,
            &params::XScraperId(test_vorgang.api_id),
            &test_vorgang,
        )
        .await
        .expect("Failed to get valid server respone when trying to push new Vorgang.");
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // 2. Get procedures with invalid parameters
        {
            let response = super::vorgang_get(
                server,
                    &params::IfModSince::default(),
                    &params::VorgangGetQueryParams {
                        since: Some(Utc::now()),
                        until: Some(Utc::now() - chrono::Duration::days(365)), // invalid: until is before since
                       ..Default::default()
                    },
                false,
                )
                .await
                .expect("Failed to get valid server respone when trying to get existing Vorgang with invalid date range.");
            assert!(matches!(response, ListingGetResponse::BadReq));
            let response = super::vorgang_get(
                server,
                    &params::IfModSince::default(),
                    &params::VorgangGetQueryParams {
                        since: Some(Utc::now() + chrono::Duration::days(365)),
                        until: Some(Utc::now() + chrono::Duration::days(366)),
                       ..Default::default()
                    },
                    false,
                )
                .await
                .expect("Failed to get valid server respone when trying to get existing Vorgang with date range in the future.");
            assert!(matches!(response, ListingGetResponse::NoContent));
        }
        scenario.teardown().await;
    }

    #[tokio::test]
    pub async fn test_vorgang_get_filtered_endpoints_success() {
        let scenario = TestSetup::new("test_vorgang_get_filtered_success").await;
        let server = &scenario.server;
        let test_vorgang = models::Vorgang {
            stationen: vec![models::Station {
                gremium: models::Gremium {
                    parlament: models::Parlament::Bt,
                    ..generate::default_gremium()
                },
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        {
            // First create a procedure with specific parameters
            let create_response = super::vorgang_put(
                server,
                1,
                &params::XScraperId(test_vorgang.api_id),
                &test_vorgang,
            )
            .await
            .expect("Failed to get valid server respone when trying to push new Vorgang.");
            assert!(matches!(create_response, PutResponse::Success));
        }
        {
            // Then get it with matching filters
            let response = super::vorgang_get(
                server,
                    &params::IfModSince::default(),
                    &params::VorgangGetQueryParams {
                        page: Some(0),
                        per_page: Some(32),
                        p: Some(models::Parlament::Bt),
                        vgtyp: Some(test_vorgang.typ),
                        wp: Some(test_vorgang.wahlperiode),
                       ..Default::default()
                    },
                    false,
                )
                .await
                .expect("Failed to get valid server respone when trying to get existing Vorgang with valid filters.");
            assert!(matches!(response, ListingGetResponse::Success {body, ..} if !body.is_empty()));
        }

        // Cleanup
        scenario.teardown().await;
    }

    #[tokio::test]
    pub async fn test_vorgang_put_endpoint() {
        // Setup test server and database
        let scenario = TestSetup::new("test_vorgang_put").await;
        let server = &scenario.server;

        // Test cases for vorgang_id_put:
        // 1. Update existing procedure with valid data and admin permissions
        {
            let test_vorgang = generate::default_vorgang();
            let response = super::vorgang_id_put(
                server,
                1,
                &params::ApiIdPathParam {
                    api_id: test_vorgang.api_id,
                },
                &test_vorgang,
            )
            .await
            .expect("Failed to get valid server respone when trying to push new Vorgang.");
            assert!(matches!(response, PutResponse::Success));
        }

        // Test cases for vorgang_put:
        // 1. Create new procedure with valid data and collector permissions
        {
            let test_vorgang = generate::default_vorgang();
            let response = super::vorgang_put(
                server,
                1,
                &params::XScraperId(test_vorgang.api_id),
                &test_vorgang,
            )
            .await
            .expect("Failed to get valid server respone when trying to push new Vorgang.");
            assert!(matches!(response, PutResponse::Success));
        }

        // 2. Handle ambiguous matches (conflict)
        {
            let vg1 = generate::default_vorgang();
            let mut vg2 = vg1.clone();
            let mut vg3 = vg1.clone();
            vg2.api_id = Uuid::now_v7();
            vg2.stationen[0].api_id = Some(uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeefeeed"));
            vg3.api_id = Uuid::now_v7();
            vg3.stationen[0].api_id = Some(uuid::uuid!("b18bde64-c0ff-eeee-ff0c-deadbeefeeec"));

            let rsp1 = super::vorgang_id_put(
                server,
                1,
                &params::ApiIdPathParam { api_id: vg1.api_id },
                &vg1,
            )
            .await
            .expect("Failed to get valid server respone when trying to push new Vorgang.");
            assert!(matches!(rsp1, PutResponse::Success));

            let rsp2 = super::vorgang_id_put(
                server,
                1,
                &params::ApiIdPathParam { api_id: vg2.api_id },
                &vg2,
            )
            .await
            .expect("Failed to get valid server respone when trying to push new Vorgang.");
            assert!(matches!(rsp2, PutResponse::Success));

            let conflict_resp = super::vorgang_put(
                server,
                1,
                &params::XScraperId(Uuid::nil()),
                &vg3,
            )
            .await
            .expect("Failed to get valid server respone when trying to push conflicting Vorgang.");
            assert!(matches!(conflict_resp, PutResponse::Conflict));
        }

        // Cleanup
        scenario.teardown().await;
    }

    #[tokio::test]
    pub async fn test_vorgang_delete_endpoints() {
        // Setup test server and database
        let scenario = TestSetup::new("test_vorgang_delete").await;
        let server = &scenario.server;

        // Test cases for vorgang_delete:
        // 1. Delete existing procedure with proper permissions
        {
            let test_vorgang = generate::default_vorgang();
            // First create the procedure
            let create_response = super::vorgang_put(
                server,
                1,
                &params::XScraperId(Uuid::now_v7()),
                &test_vorgang,
            )
            .await
            .expect("Failed to get valid server respone when trying to push new Vorgang.");
            assert!(matches!(create_response, PutResponse::Success));

            // Then delete it
            super::vorgang_delete(
                server,
                &params::ApiIdPathParam {
                    api_id: test_vorgang.api_id,
                },
            )
            .await
            .expect("Failed to get valid server respone when trying to delete Vorgang.");
        }

        // 2. Delete non-existent procedure
        {
            let non_existent_id = Uuid::now_v7();
            super::vorgang_delete(
                server,
                &params::ApiIdPathParam {
                    api_id: non_existent_id,
                },
            )
            .await
            .expect(
                "Failed to get valid server respone when trying to delete non-existent Vorgang.",
            );
        }

        // Cleanup
        scenario.teardown().await;
    }
    #[tokio::test]
    pub async fn test_malformed_data_vorgang() {
        // TODO test multiple conflicting stations
    }

    #[tokio::test]
    pub async fn test_malformed_data_station() {
        // TODO test multiple conflicting dokumente / stellungnahmen
    }
}

#[cfg(test)]
pub(crate) mod test_failed_irl_scenarios {
    use crate::api::RoundTimestamp;
    use crate::models::enumerations::{Ressort, Sachgebiet};
    use crate::models::params;
    use crate::utils::testing::{TestSetup, generate};
    use crate::{api, models};
    use std::path::{Path, PathBuf};
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tracing_test::traced_test;
    use uuid::uuid;
    use uuid::{Timestamp, Uuid};

    pub async fn read_jsonl(path: &Path) -> Vec<models::Vorgang> {
        let file = tokio::fs::File::open(path)
            .await
            .expect("Failed to open JSON lines file.");
        let buf_reader = BufReader::new(file);
        let mut records = vec![];
        let mut lines = buf_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            records.push(
                serde_json::from_str(&line).unwrap_or_else(|_| {
                    panic!("Failed to deserialize JSON line in file: {path:?}")
                }),
            );
        }
        records
    }

    macro_rules! irl_scenario {
        ($path:expr, $name:ident) => {
            #[traced_test]
            #[tokio::test]
            async fn $name() {
                let path = PathBuf::from($path);
                let test_setup =
                    TestSetup::new(&concat!("scenario_test_", stringify!($name))).await;
                let objects = read_jsonl(&path).await;
                for obj in &objects {
                    tracing::trace!("next to be inserted: {}", obj.api_id);
                    let response = super::vorgang_put(
                        &test_setup.server,
                        1,
                        &crate::models::params::XScraperId(Uuid::nil()),
                        obj,
                    )
                    .await
                    .expect("Expected no internal failure after put");
                    assert!(matches!(response, super::super::PutResponse::Success));
                }
                test_setup.teardown().await;
            }
        };
        ($path:expr, $name:ident, $ltzf_error_variant:path, $error_source_matches:path) => {
            #[traced_test]
            #[tokio::test]
            async fn $name() {
                let path = PathBuf::from($path);
                let test_setup =
                    TestSetup::new(&concat!("scenario_test_", stringify!($name))).await;
                let objects = read_jsonl(&path).await;
                for obj in &objects {
                    tracing::trace!("next to be inserted: {}", obj.api_id);
                    let response = super::vorgang_put(
                        &test_setup.server,
                        1,
                        &crate::models::params::XScraperId(Uuid::nil()),
                        obj,
                    )
                    .await
                    .expect_err("Expected a failure, got success instead");
                    assert!(matches!(
                        response,
                        $ltzf_error_variant { source }
                        if matches!(
                            *source,
                            $error_source_matches {..}
                            )
                    ));
                }
                test_setup.teardown().await;
            }
        };
    }
    irl_scenario!(
        "tests/scenarios/on_onflict_upd_nodouble.jsonl",
        test_scenario_on_onflict_upd_nodouble
    );
    irl_scenario!(
        "tests/scenarios/500_sqlx_query.jsonl",
        sqlx_ret_500_query_returned_no_rows
    );
    irl_scenario!(
        "tests/scenarios/500_on_rel_station_merge.jsonl",
        sqlx_500_on_rel_station_merge
    );

    /// This test does not assert anything, and crashes as failure case.
    /// this reproduces #110, where a null byte in the full text caused issues when retrieving it
    /// from postgres' VARCHAR.
    /// This test is meant to test whether other control chars lead to similar crashes
    #[tokio::test]
    #[allow(
        clippy::disallowed_macros,
        reason = "eprintln is meant here only to give some consumer for errors"
    )]
    async fn issue110_crash() {
        let test_setup = TestSetup::new("scenario_test_issue110_crash").await;
        let pre1 = r#"{"api_id": ""#;
        let pre2 = r#"",
  "titel": "NUL repro",
  "wahlperiode": 21,
  "verfassungsaendernd": false,
  "typ": "gg-einspruch",
  "ids": [{ "id": "20/99999", "typ": "initdrucks" }],
  "links": ["https://example.com"],
  "initiatoren": [{ "person": "Test", "organisation": "Test", "fachgebiet": "Test", "lobbyregister":
"https://www.lobbyregister.bundestag.de/suche/experte/12345" }],
  "stationen": [{
    "api_id": "88211bdc-60b9-4422-8b4f-ba78408aac4f",
    "titel": "Station",
    "zp_start": "2024-01-01T00:00:00+00:00",
    "zp_modifiziert": "2024-01-01T00:00:00+00:00",
    "gremium": { "parlament": "BT", "wahlperiode": 21, "link": "https://example.com", "name": "Ausschuss" },
    "link": "https://example.com",
    "typ": "preparl-regent",
    "dokumente": [{
      "api_id": "88211bdc-60b9-4422-8b4f-ba78408aac4f",
      "titel": "NULDOC",
      "typ": "preparl-entwurf",
      "volltext": "Vorher "#;
        let post = r#" Nachher",
      "link": "https://example.com",
      "hash": "ed9ceab0cb76eabc0ecba0ecba0e8c7ba0e8r7c6ba0e876cb",
      "zp_referenz": "2024-01-01T00:00:00+00:00",
      "zp_modifiziert": "2024-01-01T00:00:00+00:00",
      "autoren": []
    }]
  }]
}"#;
        let mut errors = vec![];
        for i in 0u16..128 {
            let uuid = Uuid::new_v7(Timestamp::from_gregorian_time(128_379_123_791_283, i));
            let vgstr = format!("{pre1}{uuid}{pre2}\\u{i:04x}{post}");
            let vg = serde_json::from_str(&vgstr);
            let vg: models::Vorgang = match vg {
                Ok(vg) => vg,
                Err(e) => {
                    errors.push((i, e));
                    continue;
                }
            };
            super::vorgang_put(
                &test_setup.server,
                1,
                &crate::models::params::XScraperId(Uuid::nil()),
                &vg,
            )
            .await
            .expect("Expected no internal failure after put");
            let _obj = super::vorgang_get_by_id(
                &test_setup.server,
                &models::params::IfModSince::default(),
                &models::params::ApiIdPathParam { api_id: uuid },
                false,
            )
            .await;
        }
        eprintln!("{errors:#?}");
        test_setup.teardown().await;
    }
    #[tokio::test]
    async fn vorgang_get_success_new_members() {
        let scenario = TestSetup::new("test_vorgang_get_success_new_members").await;

        let api_id = uuid!("06836a6d-30e6-4200-b968-ca362af92d2e");
        let vorgang = models::Vorgang {
            api_id,
            ressort: Some(Ressort::Justiz),
            sachgebiete: vec![Sachgebiet::Normung, Sachgebiet::JagdFischerei],
            ..generate::default_vorgang()
        };
        let _insert = super::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");

        let vorgang = super::vorgang_get_by_id(
            &scenario.server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id },
            false,
        )
        .await;

        assert!(matches!(vorgang,
        Ok(crate::api::GetSingleResponse::Success(v))
            if v.ressort == Some(Ressort::Justiz) &&
            v.sachgebiete.contains(&Sachgebiet::Normung) &&
            v.sachgebiete.contains(&Sachgebiet::JagdFischerei)
        ));
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn issue184_vorgang_id_put_fails_if_vorgang_is_malformed() {
        let scenario =
            TestSetup::new("test_issue184_vorgang_id_put_fails_if_vorgang_is_malformed").await;

        let api_id = uuid!("06836a6d-30e6-4200-b968-ca362af92d2e");
        let vorgang = models::Vorgang {
            api_id,
            ..generate::default_vorgang()
        };

        let _insert = super::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");
        let malformed_vorgang = models::Vorgang {
            stationen: vec![models::Station {
                typ: models::Stationstyp::ParlAblehnung,
                ..generate::default_station()
            }],
            ..vorgang.clone()
        };
        let fail = super::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: malformed_vorgang.api_id,
            },
            &malformed_vorgang,
        )
        .await
        .expect("Expected bad request");
        assert!(matches!(fail, api::PutResponse::BadReq(..)));

        let fetched_vorgang = super::vorgang_get_by_id(
            &scenario.server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            true,
        )
        .await;
        let Ok(api::GetSingleResponse::Success(fetched_vorgang)) = fetched_vorgang else {
            panic!()
        };

        assert_eq!(
            vorgang.with_round_timestamps(),
            fetched_vorgang.with_round_timestamps()
        );
    }

    #[tokio::test]
    async fn station_schlagworte_are_merged_into_vorgang() {
        let scenario = TestSetup::new("test_station_schlagworte_are_merged").await;
        let api_id = uuid!("06836a6d-30e6-4200-bbbb-ca362af92d2e");

        let vorgang = models::Vorgang {
            api_id,
            schlagworte: [
                "kakaotee",
                "computervirus",
                "exploit",
                "kenspaltung",
                "dirty bomb",
            ]
            .iter()
            .map(|&x| x.to_owned())
            .collect(),
            stationen: vec![models::Station {
                schlagworte: vec!["neues schlagwort".to_owned(), "kartenspiele".to_owned()],
                ..generate::default_station()
            }],
            ..generate::default_vorgang()
        };
        let _insert = super::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");

        let retr_vorgang = super::vorgang_get_by_id(
            &scenario.server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id },
            false,
        )
        .await
        .expect("");
        let api::GetSingleResponse::Success(retr_vorgang) = retr_vorgang else {
            panic!("unexpected response")
        };
        let mut output_sw = retr_vorgang.schlagworte.clone();
        output_sw.sort();

        let mut expected_sw: Vec<String> = vorgang
            .schlagworte
            .iter()
            .chain(vorgang.stationen[0].schlagworte.iter())
            .cloned()
            .collect();
        expected_sw.sort_unstable();
        assert_eq!(expected_sw, output_sw);

        scenario.teardown().await;
    }

    #[tokio::test]
    async fn vorgang_schlagworte() {
        let scenario = TestSetup::new("test_vorgang_schlagworte").await;
        let api_id = uuid!("06836a6d-30e6-4200-bbbb-ca362af92d2e");
        let mut schlagworte = vec![
            "kakaotee",
            "computervirus",
            "exploit",
            "kenspaltung",
            "dirty bomb",
        ];
        schlagworte.sort_unstable();

        let vorgang = models::Vorgang {
            api_id,
            schlagworte: schlagworte.iter().map(|&x| x.to_owned()).collect(),
            stationen: vec![generate::default_station()],
            ..generate::default_vorgang()
        };
        let _insert = super::vorgang_id_put(
            &scenario.server,
            1,
            &params::ApiIdPathParam {
                api_id: vorgang.api_id,
            },
            &vorgang,
        )
        .await
        .expect("Failed to create Vorgang.");

        let retr_vorgang = super::vorgang_get_by_id(
            &scenario.server,
            &params::IfModSince::default(),
            &params::ApiIdPathParam { api_id },
            false,
        )
        .await
        .expect("");
        let api::GetSingleResponse::Success(retr_vorgang) = retr_vorgang else {
            panic!("unexpected response")
        };
        let mut output_sw = retr_vorgang.schlagworte.clone();
        output_sw.sort();
        assert_eq!(schlagworte, output_sw);

        scenario.teardown().await;
    }
}
