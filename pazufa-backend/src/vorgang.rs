use axum::extract::{Json, Path, Query, State};
use axum::http::{HeaderMap, HeaderName};
use pazufa_backend_lib::LTZFServer;
use pazufa_backend_lib::api::auth::{APIScope, Claims};
use pazufa_backend_lib::api::{
    DeleteResponse, GetSingleResponse, ListingGetResponse, PutResponse, vorgang as libvg,
};
use pazufa_backend_lib::models::params::{IfModSince, XScraperId};
use pazufa_backend_lib::models::{self, params};
use pazufa_backend_lib::utils::header_params::HeaderParams;
use std::sync::Arc;
use tracing::warn;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(
    vorgang_put,
    vorgang_get,
    vorgang_get_by_id,
    vorgang_delete,
    vorgang_id_put
))]
pub(super) struct VorgangApi;

pub(super) fn router() -> OpenApiRouter<Arc<LTZFServer>> {
    OpenApiRouter::new()
        .routes(routes!(vorgang_put, vorgang_get))
        .routes(routes!(vorgang_get_by_id, vorgang_id_put, vorgang_delete,))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    method(put),
    path = "",
    tags = ["Vorgang", "Collector-Schnittstellen"],
    description = "Collector interface for inserting a new legislative process. Processes are automatically deduplicated against existing data and merged if necessary. Used by data collection services to add new legislative processes to the system.",
    security(("apiKey" = [ "collector", "admin", "keyadder"])),
    params(params::XScraperId),
    request_body = models::Vorgang,
    responses(PutResponse)
)]
async fn vorgang_put(
    claims: Claims,
    HeaderParams(scraper_id): HeaderParams<XScraperId>,
    State(api_impl): State<Arc<LTZFServer>>,
    Json(body): Json<models::Vorgang>,
) -> PutResponse {
    // Authentication
    if !matches!(
        claims.0,
        APIScope::Collector | APIScope::KeyAdder | APIScope::Admin
    ) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libvg::vorgang_put(api_impl.as_ref(), claims.1, &scraper_id, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}

#[utoipa::path(
    method(get),
    path = "",
    tags = ["Vorgang", "Unauthorisiert"],
    description = "Retrieves a filterable list of legislative processes. Returns up to 64 processes per request, which can be filtered by various criteria including time range, parliament, and electoral period.",
    params(
        params::VorgangGetQueryParams,
        params::IfModSince,
        params::ExpandQueryParam
        ),
    responses(ListingGetResponse<models::Vorgang>)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_get(
    headers: HeaderMap,
    Query(query_params): Query<params::VorgangGetQueryParams>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> ListingGetResponse<models::Vorgang> {
    // Header parameters
    let ims = match IfModSince::try_from(headers.get(HeaderName::from_static("if-modified-since")))
    {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return ListingGetResponse::BadReq;
        }
    };

    libvg::vorgang_get(
        api_impl.as_ref(),
        &ims,
        &query_params,
        expand.expand.unwrap_or(false),
    )
    .await
    .unwrap_or_else(|e| {
        warn!("{e}");
        ListingGetResponse::Error(e.to_string())
    })
}

#[utoipa::path(
    method(get),
    path = "/{api_id}",
    tags = ["Vorgang", "Unauthorisiert"],
    description = "Retrieves a specific legislative process by its unique identifier. Returns comprehensive details about the process including all its stations, associated documents, and metadata. If called by admin or higher, this returns a list of last scrapers/collectors touching the object",
    params(
        params::ApiIdPathParam,
        params::IfModSince,
        params::ExpandQueryParam,
        ),
    responses(GetSingleResponse<models::Vorgang>)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_get_by_id(
    headers: HeaderMap,
    Path(path_params): Path<params::ApiIdPathParam>,
    Query(expand): Query<params::ExpandQueryParam>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> GetSingleResponse<models::Vorgang> {
    // Header parameters
    let ims = match IfModSince::try_from(headers.get(HeaderName::from_static("if-modified-since")))
    {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return GetSingleResponse::BadReq;
        }
    };

    libvg::vorgang_get_by_id(
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

#[utoipa::path(
    method(delete),
    path = "/{api_id}",
    tags = ["Vorgang", "Data-Administration"],
    description = "Administrative endpoint to completely remove a legislative process from the system. This operation cannot be undone.",
    params(params::ApiIdPathParam),
    security(("apiKey" = [ "admin", "keyadder"])),
    responses(DeleteResponse)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_delete(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        warn!("Not authorized");
        return DeleteResponse::Unauth;
    }

    match libvg::vorgang_delete(api_impl.as_ref(), &path_params).await {
        Ok(()) => DeleteResponse::Success,
        Err(e) => {
            warn!("{e}");
            DeleteResponse::Error(e.to_string())
        }
    }
}

#[utoipa::path(
    put,
    path = "/{api_id}",
    tags = ["Vorgang", "Data-Administration"],
    description = "Administrative endpoint to directly set or replace a legislative process without merging or matching. Replaces the entire process data if it exists, or creates a new one if it doesn't.",
    params(params::ApiIdPathParam),
    security(("apiKey" = [ "admin", "keyadder"])),
    responses(PutResponse)
)]
#[tracing::instrument(skip_all)]
async fn vorgang_id_put(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
    Json(body): Json<models::Vorgang>,
) -> PutResponse {
    // Authentication
    if !matches!(claims.0, APIScope::Admin | APIScope::KeyAdder) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libvg::vorgang_id_put(api_impl.as_ref(), claims.1, &path_params, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}
#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::api::auth::APIScope;
    use pazufa_backend_lib::models;
    use pazufa_backend_lib::models::params::{self, XScraperId};
    use pazufa_backend_lib::utils::header_params::HeaderParams;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_auth_vorgang_id_put() {
        let mut scenario = TestSetup::new("test_auth_vorgang_id_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::vorgang_id_put(
                hm,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
                axum::Json(models::Vorgang::default()),
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
    async fn test_auth_vorgang_delete() {
        let mut scenario = TestSetup::new("test_auth_vorgang_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |api_key, server| async {
            super::vorgang_delete(
                api_key,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin],
            call_closure,
            pazufa_backend_lib::api::DeleteResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_auth_vorgang_put() {
        let mut scenario = TestSetup::new("test_auth_vorgang_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |api_key, server| async {
            super::vorgang_put(
                api_key,
                HeaderParams(XScraperId(uuid::Uuid::nil())),
                axum::extract::State(server),
                axum::Json(models::Vorgang::default()),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder, APIScope::Admin, APIScope::Collector],
            call_closure,
            pazufa_backend_lib::api::PutResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
}
