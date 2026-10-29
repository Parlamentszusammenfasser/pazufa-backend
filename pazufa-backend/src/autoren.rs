use axum::extract::{Json, Query, State};
use pazufa_backend_lib::PazufaServer;
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::api::{
    DeleteResponse, ListingGetResponse, PutResponse, misc as libmisc, misc_auth as libmisca,
};
use pazufa_models::{self as models, APIScope, params};
use std::sync::Arc;
use tracing::{error, warn};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(paths(autoren_delete_by_param, autoren_put, autoren_get,))]
pub(super) struct AutorenApi;

pub(super) fn router() -> OpenApiRouter<Arc<PazufaServer>> {
    OpenApiRouter::new().routes(routes!(autoren_delete_by_param, autoren_put, autoren_get))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    method(delete),
    path = "",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::AutorenDeleteByParamQueryParams),
    responses(DeleteResponse)
)]
async fn autoren_delete_by_param(
    claims: Claims,
    Query(query_params): Query<params::AutorenDeleteByParamQueryParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Unauthorized access to autoren_delete");
        return DeleteResponse::Unauth;
    }

    let empty_qp = params::AutorenDeleteByParamQueryParams {
        person: None,
        fach: None,
        org: None,
    };
    if query_params == empty_qp {
        warn!(
            "You tried to delete all Authors with an empty filter. This is not possible for safety reasons. Try to give me at least one filter"
        );
        return DeleteResponse::Forbid;
    }
    match libmisca::autoren_delete_by_param(api_impl.as_ref(), &query_params).await {
        Ok(()) => DeleteResponse::Success,
        Err(e) => {
            warn!("Application was unable to delete autoren: {e}");
            DeleteResponse::Error(e.to_string())
        }
    }
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "",
    description = "Administrative endpoint to add or update multiple authors at once. Creates new authors or replaces existing ones based on matching names and organizations.",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    request_body = models::ReplacementPutRequest<models::Autor>,
    responses(PutResponse)
)]
async fn autoren_put(
    claims: Claims,
    State(api_impl): State<Arc<PazufaServer>>,
    Json(body): Json<models::ReplacementPutRequest<models::Autor>>,
) -> PutResponse {
    // Authentication

    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libmisca::autoren_put(api_impl.as_ref(), &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "",
    description = "Retrieves a list of authors filtered by optional parameters. Returns authors matching the specified criteria including name fragments, professional field, and organization.",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::AutorenGetQueryParams),
    responses(ListingGetResponse<models::Autor>)
)]
pub async fn autoren_get(
    Query(query_params): Query<params::AutorenGetQueryParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> ListingGetResponse<models::Autor> {
    libmisc::autoren_get(api_impl.as_ref(), &query_params)
        .await
        .unwrap_or_else(|e| {
            error!(error = ?e);
            ListingGetResponse::Error(e.to_string())
        })
}

#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_models::APIScope;
    use pazufa_models::{self, params};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_autoren_put() {
        let mut scenario = TestSetup::new("test_autoren_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::autoren_put(
                hm,
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
    async fn test_autoren_delete() {
        let mut scenario = TestSetup::new("test_autoren_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::autoren_delete_by_param(
                hm,
                axum::extract::Query(params::AutorenDeleteByParamQueryParams::default()),
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
