use crate::PazufaServer;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName};
use axum::response::Json;
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::api::sitzung::{
    self as libsitz, SitzungListingResponse, get_sitzung_by_parameter,
};
use pazufa_backend_lib::api::{
    DeleteResponse, GetSingleResponse, PutResponse, find_applicable_date_range,
};
use pazufa_backend_lib::db::retrieve;
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::{APIScope, params};
use std::sync::Arc;
use tracing::warn;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(sitzung_delete, sid_put, s_get, s_get_by_id))]
pub(super) struct SitzungApi;

pub(super) fn router() -> OpenApiRouter<Arc<PazufaServer>> {
    OpenApiRouter::new()
        .routes(routes!(sitzung_delete))
        .routes(routes!(sid_put))
        .routes(routes!(s_get))
        .routes(routes!(s_get_by_id))
}

#[utoipa::path(
    delete,
    path = "/{api_id}",
    description = "Administrative endpoint to completely remove a parliamentary session from the system. This operation cannot be undone.",
    tags = ["Sitzung", "Data-Administration"],
    security(("apiKey" = [ "admin", "keyadder"])),
    params(params::ApiIdPathParam),
    responses(DeleteResponse)
)]
#[tracing::instrument(skip_all, fields(sid=%path_params.api_id))]
async fn sitzung_delete(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        return DeleteResponse::Unauth;
    }

    match pazufa_backend_lib::db::delete::delete_sitzung_by_api_id(
        path_params.api_id,
        &api_impl.sqlx_db,
    )
    .await
    {
        Ok(()) => DeleteResponse::Success,
        Err(e) => {
            warn!("Delete unsuccessful: {e}");
            DeleteResponse::Error(e.to_string())
        }
    }
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "/{api_id}",
    description = "Administrative endpoint to directly set or replace a parliamentary session without merging or matching. Replaces the entire session data if it exists, or creates a new one if it doesn't.",
    tags = ["Sitzung", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::ApiIdPathParam),
    request_body(content_type = "application/json", content = models::Sitzung),
    responses(PutResponse)
)]
async fn sid_put(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<PazufaServer>>,
    Json(body): Json<models::Sitzung>,
) -> PutResponse {
    // Authentication
    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        return PutResponse::Unauth;
    }

    libsitz::sid_put(api_impl.as_ref(), &claims, &path_params, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("Error: {e}");
            PutResponse::Error(e.to_string())
        })
}

/// Get Sitzung objects by filters
#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "",
    description = "Retrieves a filterable list of parliamentary sessions. Returns up to 64 sessions per request, which can be filtered by various criteria including time frame, parliament, and electoral period.",
    tags = ["Sitzung", "Unauthorisiert"],
    params(
        params::SGetQueryParams,
        params::IfModSince,
        params::ExpandQueryParam,
    ),
    responses(SitzungListingResponse)
)]
async fn s_get(
    headers: HeaderMap,
    Query(query_params): Query<params::SGetQueryParams>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> SitzungListingResponse {
    async fn inner(
        server: &PazufaServer,
        header_params: &params::IfModSince,
        query_params: &params::SGetQueryParams,
        expand: bool,
    ) -> anyhow::Result<SitzungListingResponse> {
        #[rustfmt::skip]
        let Some(range) = find_applicable_date_range(None, None, None, 
            query_params.since, query_params.until, None,
        ) else {
            warn!(
                "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
                query_params, header_params.if_modified_since
            );
            return Ok(SitzungListingResponse::BadReq);
        };
        let params = retrieve::SitzungFilterParameters {
            gremium_like: query_params.gr.clone(),
            parlament: query_params.p,
            if_mod_since: header_params.if_modified_since,
            wp: query_params.wp,
            since: range.since,
            until: range.until,
            vgid: query_params.vgid,
        };
        let mut tx = server.sqlx_db.begin().await?;
        Ok(get_sitzung_by_parameter(
            params,
            query_params.page,
            query_params.per_page,
            expand,
            "/api/v2/sitzung",
            &mut tx,
        )
        .await?)
    }
    // Header parameters
    let ims = match params::IfModSince::try_from(
        headers.get(HeaderName::from_static("if-modified-since")),
    ) {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return SitzungListingResponse::BadReq;
        }
    };

    inner(
        api_impl.as_ref(),
        &ims,
        &query_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("{e}");
        SitzungListingResponse::Error(e.to_string())
    })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/{api_id}",
    description = "Retrieves detailed information about a specific parliamentary session by its unique identifier. Returns complete session data including agenda items, documents, and metadata. If called by admin or higher, this returns a list of last scrapers/collectors touching the object",
    tags = ["Sitzung", "Unauthorisiert"],
    params(
        params::ApiIdPathParam,
        params::IfModSince,
        params::ExpandQueryParam,
    ),
    responses(GetSingleResponse<models::Sitzung>)
)]
async fn s_get_by_id(
    headers: HeaderMap,
    Query(expand): Query<params::ExpandQueryParam>,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> GetSingleResponse<models::Sitzung> {
    let ims = match params::IfModSince::try_from(
        headers.get(HeaderName::from_static("if-modified-since")),
    ) {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return GetSingleResponse::BadReq;
        }
    };

    libsitz::s_get_by_id(
        api_impl.as_ref(),
        &ims,
        &path_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("{e}");
        GetSingleResponse::Error(e.to_string())
    })
}

#[cfg(test)]
mod test {
    use crate::testing::{
        TestClient, TestSetup, authtesting, rfc3339, seed_sitzungen, test_sitzung, tomorrow,
        yesterday,
    };
    use pazufa_backend_lib::models::params::{self};
    use pazufa_backend_lib::models::{self, APIScope};

    use pazufa_backend_lib::api::PutResponse;
    use std::sync::Arc;
    use tracing_test::traced_test;

    #[tokio::test]
    async fn test_auth_sid_put() {
        let mut scenario = TestSetup::new("test_auth_sid_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::sid_put(
                hm,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
                axum::Json(pazufa_backend_lib::models::Sitzung::default()),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin],
            call_closure,
            pazufa_backend_lib::api::PutResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_auth_sitzung_delete() {
        let mut scenario = TestSetup::new("test_auth_sitzung_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::sitzung_delete(
                hm,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::Admin, APIScope::KeyAdder],
            call_closure,
            pazufa_backend_lib::api::DeleteResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_session_delete() {
        let mut scenario = TestSetup::new("test_session_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));
        let sitzung = pazufa_backend_lib::models::Sitzung {
            api_id: Some(uuid::uuid!("6142b111-f691-404d-aeff-521a7a530134")),
            ..pazufa_backend_lib::models::Sitzung::default()
        };

        // Test case: Input non-existing session
        let response = super::sid_put(
            pazufa_backend_lib::api::auth::Claims(APIScope::Admin, 1),
            axum::extract::Path(params::ApiIdPathParam {
                api_id: sitzung.api_id.expect("Failed"),
            }),
            axum::extract::State(Arc::clone(&server)),
            axum::Json(sitzung.clone()),
        )
        .await;
        assert!(matches!(response, PutResponse::Success));

        // Test cases for sitzung_delete:
        // Test case: Delete existing session with proper permissions
        let r = super::sitzung_delete(
            pazufa_backend_lib::api::auth::Claims(APIScope::Admin, 1),
            axum::extract::Path(params::ApiIdPathParam {
                api_id: sitzung.api_id.expect("Failed"),
            }),
            axum::extract::State(Arc::clone(&server)),
        )
        .await;
        assert!(matches!(
            r,
            pazufa_backend_lib::api::DeleteResponse::Success
        ));

        // Test case: Delete non-existent session
        let r = super::sitzung_delete(
            pazufa_backend_lib::api::auth::Claims(APIScope::Admin, 1),
            axum::extract::Path(params::ApiIdPathParam {
                api_id: sitzung.api_id.expect("Failed"),
            }),
            axum::extract::State(Arc::clone(&server)),
        )
        .await;
        assert!(matches!(
            r,
            pazufa_backend_lib::api::DeleteResponse::Success
        ));

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_s_get() {
        let mut scenario = TestSetup::new("test_s_get").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));
        let client = TestClient::new(Arc::clone(&server)).await;

        let now = chrono::Utc::now();
        let sessions = [test_sitzung(now, 1), test_sitzung(now, 2)];
        seed_sitzungen(&client, &server, &sessions).await;

        // no filters
        let rsp = client.get("/api/v2/sitzung", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.header("x-total-count"), Some("2"), "{rsp:?}");
        assert!(rsp.header("link").is_some(), "{rsp:?}");
        let mut body = rsp.json::<Vec<models::Sitzung>>();
        body.sort_by_key(|s| s.nummer);
        assert_eq!(
            body.iter().map(|s| s.titel.clone()).collect::<Vec<_>>(),
            sessions.iter().map(|s| s.titel.clone()).collect::<Vec<_>>()
        );
        assert!(body.iter().all(|s| s.gremium == sessions[0].gremium));

        // expand does not change what is found
        let rsp = client.get("/api/v2/sitzung?expand=true", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 2);

        // parliament and electoral period
        let rsp = client.get("/api/v2/sitzung?p=BT&wp=20", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 2);
        let rsp = client.get("/api/v2/sitzung?p=BT&wp=19", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");
        let rsp = client.get("/api/v2/sitzung?p=BR", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // committee name is matched as a case insensitive substring
        let rsp = client.get("/api/v2/sitzung?gr=tests", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 2);
        let rsp = client.get("/api/v2/sitzung?gr=gibtsnicht", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // unknown vorgang
        let rsp = client
            .get(&format!("/api/v2/sitzung?vgid={}", uuid::Uuid::nil()), &[])
            .await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // date range
        let since = yesterday(now);
        let until = tomorrow(now);
        let rsp = client
            .get(
                &format!(
                    "/api/v2/sitzung?since={}&until={}",
                    rfc3339(since),
                    rfc3339(until)
                ),
                &[],
            )
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        let body = rsp.json::<Vec<models::Sitzung>>();
        assert_eq!(body.len(), 2);
        for s in body {
            assert!(
                s.termin >= since && s.termin <= until,
                "Found session outside requested date range: {s:?}"
            );
        }

        // since after until is unsatisfiable
        let rsp = client
            .get(
                &format!(
                    "/api/v2/sitzung?since={}&until={}",
                    rfc3339(until),
                    rfc3339(since)
                ),
                &[],
            )
            .await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        // since in the future
        let rsp = client
            .get(&format!("/api/v2/sitzung?since={}", rfc3339(until)), &[])
            .await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        drop(client);
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_s_get_paging_and_headers() {
        let mut scenario = TestSetup::new("test_s_get_paging_and_headers").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));
        let client = TestClient::new(Arc::clone(&server)).await;

        let now = chrono::Utc::now();
        let sessions = [test_sitzung(now, 1), test_sitzung(now, 2)];
        seed_sitzungen(&client, &server, &sessions).await;
        let since = yesterday(now);
        let until = tomorrow(now);

        // pagination
        let rsp = client.get("/api/v2/sitzung?per_page=1", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.header("x-total-count"), Some("2"), "{rsp:?}");
        assert_eq!(rsp.header("x-per-page"), Some("1"), "{rsp:?}");
        let first = rsp.json::<Vec<models::Sitzung>>();
        assert_eq!(first.len(), 1);
        let rsp = client.get("/api/v2/sitzung?per_page=1&page=2", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        let second = rsp.json::<Vec<models::Sitzung>>();
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].nummer, second[0].nummer);

        // if-modified-since in the future
        let rsp = client
            .get("/api/v2/sitzung", &[("if-modified-since", &rfc3339(until))])
            .await;
        assert_eq!(rsp.status, 304, "{rsp:?}");

        // if-modified-since in the past
        let rsp = client
            .get("/api/v2/sitzung", &[("if-modified-since", &rfc3339(since))])
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 2);

        // malformed if-modified-since
        let rsp = client
            .get("/api/v2/sitzung", &[("if-modified-since", "gestern")])
            .await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        // malformed query parameters
        let rsp = client.get("/api/v2/sitzung?p=XX", &[]).await;
        assert_eq!(rsp.status, 400, "{rsp:?}");
        let rsp = client.get("/api/v2/sitzung?vgid=keine-uuid", &[]).await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        drop(client);
        scenario.teardown().await;
    }
}
