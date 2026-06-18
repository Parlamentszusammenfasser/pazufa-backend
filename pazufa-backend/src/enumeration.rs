use axum::extract::{Json, Path, Query, State};
use pazufa_backend_lib::LTZFServer;
use pazufa_backend_lib::api::auth::{APIScope, Claims};
use pazufa_backend_lib::api::{
    DeleteResponse, ListingGetResponse, PutResponse, misc as libmisc, misc_auth as libmisca,
};
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::params;
use std::sync::Arc;
use tracing::{error, warn};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(enum_delete, enum_put, enum_get,))]
pub(super) struct EnumApi;

pub(super) fn router() -> OpenApiRouter<Arc<LTZFServer>> {
    OpenApiRouter::new().routes(routes!(enum_delete, enum_put, enum_get))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    method(delete),
    path = "/{name}/{item}",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::EnumDeletePathParams),
    responses(DeleteResponse)
)]
async fn enum_delete(
    claims: Claims,
    Path(path_params): Path<params::EnumDeletePathParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Unauthorized access to autoren_delete");
        return DeleteResponse::Unauth;
    }

    match libmisca::enum_delete(api_impl.as_ref(), &path_params).await {
        Ok(()) => DeleteResponse::Success,
        Err(e) => {
            warn!("Unable to delete enumeration data: {e}");
            DeleteResponse::Error(e.to_string())
        }
    }
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "/{name}",
    description = "Administrative endpoint to add or update values in a specified enumeration type. Replaces the entire set of values for the enumeration.",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(
        params::EnumPathParams
        ),
    request_body = models::ReplacementPutRequest<String>,
    responses(PutResponse)
)]
async fn enum_put(
    claims: Claims,
    Path(path_params): Path<params::EnumPathParams>,
    State(api_impl): State<Arc<LTZFServer>>,
    Json(body): Json<models::ReplacementPutRequest<String>>,
) -> PutResponse {
    // Authentication

    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libmisca::enum_put(api_impl.as_ref(), &path_params, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/{name}",
    description = "Retrieves values from a specified enumeration type. Returns a list of valid values that can be used in other API operations, optionally filtered by a substring.",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::EnumGetQueryParams),
    responses(ListingGetResponse<String>)
)]
async fn enum_get(
    Path(path_params): Path<params::EnumPathParams>,
    Query(query_params): Query<params::EnumGetQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> ListingGetResponse<String> {
    libmisc::enum_get(api_impl.as_ref(), &path_params, &query_params)
        .await
        .unwrap_or_else(|e| {
            error!(error = ?e);
            ListingGetResponse::Error(e.to_string())
        })
}

#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::api::auth::APIScope;
    use pazufa_backend_lib::models::{self, params};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_enum_put() {
        let mut scenario = TestSetup::new("test_enum_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::enum_put(
                hm,
                axum::extract::Path(params::EnumPathParams {
                    name: models::EnumerationNames::Vgidtypen,
                }),
                axum::extract::State(server),
                axum::Json(models::ReplacementPutRequest {
                    objects: vec![],
                    replacing: vec![],
                }),
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
    async fn test_enum_delete() {
        let mut scenario = TestSetup::new("test_enum_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::enum_delete(
                hm,
                axum::extract::Path(params::EnumDeletePathParams {
                    name: models::EnumerationNames::Vgidtypen,
                    item: String::new(),
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
}
