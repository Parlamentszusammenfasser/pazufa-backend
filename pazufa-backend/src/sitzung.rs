use crate::LTZFServer;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName};
use axum::response::Json;
use pazufa_backend_lib::api::auth::{APIScope, Claims};
use pazufa_backend_lib::api::sitzung::{self as libsitz, SitzungListingResponse};
use pazufa_backend_lib::api::{DeleteResponse, GetSingleResponse, PutResponse};
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::params;
use std::sync::Arc;
use tracing::warn;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(sitzung_delete, sid_put, s_get, s_get_by_id))]
pub(super) struct SitzungApi;

pub(super) fn router() -> OpenApiRouter<Arc<LTZFServer>> {
    OpenApiRouter::new()
        .routes(routes!(sitzung_delete))
        .routes(routes!(sid_put))
        .routes(routes!(s_get))
        .routes(routes!(s_get_by_id))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "/{api_id}",
    description = "Administrative endpoint to completely remove a parliamentary session from the system. This operation cannot be undone.",
    tags = ["Sitzung", "Data-Administration"],
    security(("apiKey" = [ "admin", "keyadder"])),
    params(params::ApiIdPathParam),
    responses(DeleteResponse)
)]
async fn sitzung_delete(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        return DeleteResponse::Unauth;
    }

    match libsitz::sitzung_delete(api_impl.as_ref(), &path_params).await {
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
    State(api_impl): State<Arc<LTZFServer>>,
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

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "",
    description = "Retrieves a filterable list of parliamentary sessions. Returns up to 64 sessions per request, which can be filtered by various criteria including time frame, parliament, and electoral period.",
    tags = ["Sitzung", "Unauthorisiert"],
    params(
        params::SGetQueryParams,
        params::IfModSince,
    ),
    responses(SitzungListingResponse)
)]
async fn s_get(
    headers: HeaderMap,
    Query(query_params): Query<params::SGetQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> SitzungListingResponse {
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

    libsitz::s_get(api_impl.as_ref(), &ims, &query_params)
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
    ),
    responses(GetSingleResponse<models::Sitzung>)
)]
async fn s_get_by_id(
    headers: HeaderMap,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
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

    libsitz::s_get_by_id(api_impl.as_ref(), &ims, &path_params)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            GetSingleResponse::Error(e.to_string())
        })
}

#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::api::auth::APIScope;
    use pazufa_backend_lib::models::params::{self};

    use std::sync::Arc;

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
}
