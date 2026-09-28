use crate::PazufaServer;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName};
use axum::response::Json;
use pazufa_backend_lib::api::PutResponse;
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::api::sitzung::{self as libsitz, SitzungListingResponse};
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::APIScope;
use pazufa_backend_lib::models::params::{self, XScraperId};
use pazufa_backend_lib::utils::header_params::HeaderParams;
use std::sync::Arc;
use tracing::warn;
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

#[tracing::instrument(skip_all)]
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
async fn kal_date_get(
    headers: HeaderMap,
    Path(path_params): Path<params::KalDateGetPathParams>,
    Query(query_params): Query<params::KalDateGetQueryParams>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> SitzungListingResponse {
    // Header parameters
    let Ok(ims) =
        params::IfModSince::try_from(headers.get(HeaderName::from_static("if-modified-since")))
    else {
        warn!("If-mod-since header was malformed");
        return SitzungListingResponse::BadReq;
    };

    libsitz::kal_date_get(
        api_impl.as_ref(),
        &ims,
        &path_params,
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
    // Header parameters
    let Ok(ims) =
        params::IfModSince::try_from(headers.get(HeaderName::from_static("if-modified-since")))
    else {
        warn!("If-mod-since header was malformed");
        return SitzungListingResponse::BadReq;
    };

    libsitz::kal_get(
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
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::models::APIScope;
    use pazufa_backend_lib::models::params::{self, XScraperId};
    use pazufa_backend_lib::utils::header_params::HeaderParams;
    use std::sync::Arc;

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
}
