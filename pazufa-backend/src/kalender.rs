use crate::PazufaServer;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName};
use axum::response::Json;
use chrono::Datelike;
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::api::sitzung::{
    self as libsitz, SitzungListingResponse, get_sitzung_by_parameter,
};
use pazufa_backend_lib::api::{PutResponse, find_applicable_date_range};
use pazufa_backend_lib::db::retrieve::SitzungFilterParameters;
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::APIScope;
use pazufa_backend_lib::models::params::{self, XScraperId};
use pazufa_backend_lib::utils::header_params::HeaderParams;
use std::sync::Arc;
use tracing::{info, instrument, warn};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(kal_date_put, kal_date_get, kal_get))]
pub(super) struct KalenderApi;

pub(super) fn router() -> OpenApiRouter<Arc<PazufaServer>> {
    OpenApiRouter::new()
        .routes(routes!(kal_date_put, kal_date_get))
        .routes(routes!(kal_get))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "/{parlament}/{datum}",
    description= "Collector interface for adding or updating sessions for a specific date and parliament. Completely replaces all sessions for the given date, with restrictions based on how far in the past the date is. Admin API keys can override the time restriction.",
    tags = ["Sitzung", "Collector-Schnittstellen"],
    security(("apiKey" = [ "collector", "admin", "keyadder"])),
    params(
        params::KalDatePutPathParams,
        params::XScraperId
    ),
    request_body(content_type = "application/json", content = Vec<models::Sitzung>, description ="Array of parliamentary sessions to set for the specified date" ),
    responses(PutResponse)
)]
async fn kal_date_put(
    claims: Claims,
    HeaderParams(scraper_id): HeaderParams<XScraperId>,
    Path(path_params): Path<params::KalDatePutPathParams>,
    State(api_impl): State<Arc<PazufaServer>>,
    Json(body): Json<Vec<models::Sitzung>>,
) -> PutResponse {
    // Authentication

    if !matches!(
        claims.0,
        APIScope::KeyAdder | APIScope::Admin | APIScope::Collector
    ) {
        return PutResponse::Unauth;
    }

    match libsitz::kal_date_put(api_impl.as_ref(), &claims, &scraper_id, &path_params, &body).await
    {
        Ok(()) => PutResponse::Success,
        Err(e) => {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        }
    }
}

#[utoipa::path(
    get,
    path = "/{parlament}/{datum}",
    description = "Retrieves a list of parliamentary sessions for a specific date and parliament. Provides all sessions scheduled for the given day in the specified parliamentary body.",
    tags = ["Sitzung", "Unauthorisiert"],
    params(
        params::KalDateGetPathParams,
        params::IfModSince,
        params::ExpandQueryParam,
    ),
    responses(SitzungListingResponse)
)]
#[instrument(skip_all, fields(date=%path_params.datum))]
async fn kal_date_get(
    headers: HeaderMap,
    Path(path_params): Path<params::KalDateGetPathParams>,
    Query(query_params): Query<params::KalDateGetQueryParams>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> SitzungListingResponse {
    async fn inner(
        server: &PazufaServer,
        ims: &params::IfModSince,
        datum: chrono::NaiveDate,
        parlament: models::Parlament,
        query_params: &params::KalDateGetQueryParams,
        expand: bool,
    ) -> anyhow::Result<SitzungListingResponse> {
        let Some(dr) = find_applicable_date_range(
            Some(datum.year()),
            Some(datum.month()),
            Some(datum.day()),
            None,
            None,
            None,
        ) else {
            info!("Date Range too narrow or invalid");
            return Ok(SitzungListingResponse::NoContent);
        };

        let params = SitzungFilterParameters {
            parlament: Some(parlament),
            gremium_like: None,
            since: dr.since,
            until: dr.until,
            if_mod_since: ims.if_modified_since,
            vgid: None,
            wp: None,
        };
        let mut tx = server.sqlx_db.begin().await?;
        Ok(get_sitzung_by_parameter(
            params,
            query_params.page,
            query_params.per_page,
            expand,
            &format!("/api/v2/kalender/{parlament}/{datum}"),
            &mut tx,
        )
        .await?)
    }
    // Header parameters
    let Ok(ims) =
        params::IfModSince::try_from(headers.get(HeaderName::from_static("if-modified-since")))
    else {
        warn!("If-mod-since header was malformed");
        return SitzungListingResponse::BadReq;
    };

    inner(
        api_impl.as_ref(),
        &ims,
        path_params.datum,
        path_params.parlament,
        &query_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("Error: {e}");
        SitzungListingResponse::Error(e.to_string())
    })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "",
    description = "Retrieves a filterable list of parliamentary sessions for calendar display. Returns up to 64 sessions per request, which can be filtered by various criteria including year, month, day, parliament, and committee.",
    tags = ["Sitzung", "Unauthorisiert"],
    params(
        params::KalGetQueryParams,
        params::IfModSince,
        params::ExpandQueryParam,
    ),
    responses(SitzungListingResponse)
)]
async fn kal_get(
    headers: HeaderMap,
    Query(query_params): Query<params::KalGetQueryParams>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> SitzungListingResponse {
    async fn inner(
        server: &PazufaServer,
        header_params: &params::IfModSince,
        query_params: &params::KalGetQueryParams,
        expand: bool,
    ) -> anyhow::Result<SitzungListingResponse> {
        let qparams = query_params;
        let hparams = header_params;
        let mut tx = server.sqlx_db.begin().await?;
        let result = find_applicable_date_range(
            qparams.y,
            qparams.m,
            qparams.dom,
            qparams.since,
            qparams.until,
            None,
        );
        let Some(result) = result else {
            warn!(
                "Parameters were chosen such that the request is unsatisfiable: {:?}, ims={:?}",
                query_params, header_params.if_modified_since
            );
            return Ok(SitzungListingResponse::BadReq);
        };

        let params = SitzungFilterParameters {
            gremium_like: qparams.gr.clone(),
            parlament: qparams.p,
            if_mod_since: hparams.if_modified_since,
            vgid: None,
            wp: qparams.wp,
            since: result.since,
            until: result.until,
        };

        Ok(get_sitzung_by_parameter(
            params,
            query_params.page,
            query_params.per_page,
            expand,
            "/api/v2/kalender",
            &mut tx,
        )
        .await?)
    }
    // Header parameters
    let Ok(ims) =
        params::IfModSince::try_from(headers.get(HeaderName::from_static("if-modified-since")))
    else {
        warn!("If-mod-since header was malformed");
        return SitzungListingResponse::BadReq;
    };

    inner(
        api_impl.as_ref(),
        &ims,
        &query_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("Error: {e}");
        SitzungListingResponse::Error(e.to_string())
    })
}

#[cfg(test)]
mod test {
    use crate::testing::{
        TestClient, TestSetup, authtesting, rfc3339, seed_sitzungen, test_sitzung, tomorrow,
        yesterday,
    };
    use chrono::Datelike;
    use pazufa_backend_lib::models::params::{self, XScraperId};
    use pazufa_backend_lib::models::{self, APIScope};
    use pazufa_backend_lib::utils::header_params::HeaderParams;
    use std::sync::Arc;
    use tracing_test::traced_test;

    #[tokio::test]
    async fn test_auth_kal_date_put() {
        let mut scenario = TestSetup::new("test_auth_kal_date_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |api_key, server| async {
            super::kal_date_put(
                api_key,
                HeaderParams(XScraperId(uuid::Uuid::nil())),
                axum::extract::Path(params::KalDatePutPathParams {
                    parlament: pazufa_backend_lib::models::Parlament::Bt,
                    datum: chrono::Utc::now().date_naive(),
                }),
                axum::extract::State(server),
                axum::Json(vec![]),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::Collector, APIScope::Admin, APIScope::KeyAdder],
            call_closure,
            pazufa_backend_lib::api::PutResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_kal_date_get() {
        let mut scenario = TestSetup::new("test_kal_date_get").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));
        let client = TestClient::new(Arc::clone(&server)).await;

        let now = chrono::Utc::now();
        let today = now.date_naive();
        let session = test_sitzung(now, 1);
        seed_sitzungen(&client, &server, std::slice::from_ref(&session)).await;

        // successful GET
        let rsp = client
            .get(&format!("/api/v2/kalender/BT/{today}"), &[])
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.header("x-total-count"), Some("1"), "{rsp:?}");
        assert!(rsp.header("link").is_some(), "{rsp:?}");
        let body = rsp.json::<Vec<models::Sitzung>>();
        assert_eq!(body.len(), 1, "{body:?}");
        assert_eq!(body[0].titel, session.titel);
        assert_eq!(body[0].nummer, session.nummer);
        assert_eq!(body[0].gremium, session.gremium);
        assert_eq!(body[0].termin.date_naive(), today);

        // expand does not change what is found
        let rsp = client
            .get(&format!("/api/v2/kalender/BT/{today}?expand=true"), &[])
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 1);

        // other parliament, same day
        let rsp = client
            .get(&format!("/api/v2/kalender/BR/{today}"), &[])
            .await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // date without sessions
        let rsp = client.get("/api/v2/kalender/BT/1950-10-10", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // date before 1945 is outside of any valid range
        let rsp = client.get("/api/v2/kalender/BT/1940-10-10", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // if-modified-since in the future
        let future = rfc3339(tomorrow(now));
        let rsp = client
            .get(
                &format!("/api/v2/kalender/BT/{today}"),
                &[("if-modified-since", &future)],
            )
            .await;
        assert_eq!(rsp.status, 304, "{rsp:?}");

        // if-modified-since in the past
        let past = rfc3339(yesterday(now));
        let rsp = client
            .get(
                &format!("/api/v2/kalender/BT/{today}"),
                &[("if-modified-since", &past)],
            )
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 1);

        // malformed if-modified-since
        let rsp = client
            .get(
                &format!("/api/v2/kalender/BT/{today}"),
                &[("if-modified-since", "gestern")],
            )
            .await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        // malformed path parameters
        let rsp = client
            .get(&format!("/api/v2/kalender/XX/{today}"), &[])
            .await;
        assert_eq!(rsp.status, 400, "{rsp:?}");
        let rsp = client.get("/api/v2/kalender/BT/2024-13-45", &[]).await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        drop(client);
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_kal_get() {
        let mut scenario = TestSetup::new("test_kal_get").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));
        let client = TestClient::new(Arc::clone(&server)).await;

        let now = chrono::Utc::now();
        let sessions = [test_sitzung(now, 1), test_sitzung(now, 2)];
        seed_sitzungen(&client, &server, &sessions).await;

        // filter by year, month, parliament and electoral period
        let rsp = client
            .get(
                &format!(
                    "/api/v2/kalender?y={}&m={}&p=BT&wp=20",
                    now.year(),
                    now.month()
                ),
                &[],
            )
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 2);

        // electoral period without sessions
        let rsp = client.get("/api/v2/kalender?p=BT&wp=19", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // parliament without sessions
        let rsp = client.get("/api/v2/kalender?p=BR", &[]).await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // date range
        let since = yesterday(now);
        let until = tomorrow(now);
        let rsp = client
            .get(
                &format!(
                    "/api/v2/kalender?p=BT&since={}&until={}",
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

        // pagination
        let rsp = client.get("/api/v2/kalender?p=BT&per_page=1", &[]).await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.header("x-total-count"), Some("2"), "{rsp:?}");
        assert_eq!(rsp.header("x-per-page"), Some("1"), "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 1);

        // since after until is unsatisfiable
        let rsp = client
            .get(
                &format!(
                    "/api/v2/kalender?since={}&until={}",
                    rfc3339(until),
                    rfc3339(since)
                ),
                &[],
            )
            .await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        // years before 1945 are unsatisfiable
        let rsp = client.get("/api/v2/kalender?y=1940", &[]).await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        // since in the future
        let rsp = client
            .get(
                &format!("/api/v2/kalender?p=BT&since={}", rfc3339(until)),
                &[],
            )
            .await;
        assert_eq!(rsp.status, 204, "{rsp:?}");

        // if-modified-since in the future
        let rsp = client
            .get(
                "/api/v2/kalender?p=BT",
                &[("if-modified-since", &rfc3339(until))],
            )
            .await;
        assert_eq!(rsp.status, 304, "{rsp:?}");

        // if-modified-since in the past
        let rsp = client
            .get(
                "/api/v2/kalender?p=BT",
                &[("if-modified-since", &rfc3339(since))],
            )
            .await;
        assert_eq!(rsp.status, 200, "{rsp:?}");
        assert_eq!(rsp.json::<Vec<models::Sitzung>>().len(), 2);

        // malformed if-modified-since
        let rsp = client
            .get("/api/v2/kalender", &[("if-modified-since", "gestern")])
            .await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        // malformed query parameters
        let rsp = client.get("/api/v2/kalender?p=XX", &[]).await;
        assert_eq!(rsp.status, 400, "{rsp:?}");

        drop(client);
        scenario.teardown().await;
    }
}
