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
#[openapi(paths(gremien_delete_by_param, gremien_put, gremien_get,))]
pub(super) struct GremienApi;

pub(super) fn router() -> OpenApiRouter<Arc<PazufaServer>> {
    OpenApiRouter::new().routes(routes!(gremien_delete_by_param, gremien_put, gremien_get))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    method(delete),
    path = "",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::GremienDeleteByParamQueryParams),
    responses(DeleteResponse)
)]
async fn gremien_delete_by_param(
    claims: Claims,
    Query(query_params): Query<params::GremienDeleteByParamQueryParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> DeleteResponse {
    // Authentication
    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Unauthorized access to autoren_delete");
        return DeleteResponse::Unauth;
    }
    let empty_qp = params::GremienDeleteByParamQueryParams {
        gr: None,
        p: None,
        wp: None,
    };
    if query_params == empty_qp {
        warn!(
            "You tried to delete all Gremien with an empty filter. This is not possible for safety reasons. Try to give me at least one filter"
        );
        return DeleteResponse::Forbid;
    }

    match libmisca::gremien_delete_by_param(api_impl.as_ref(), &query_params).await {
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
    description = "Administrative endpoint to add or update multiple committees at once. Creates new committees or replaces existing ones based on matching parliament, electoral period, and name.",
    security(("apiKey" = [  "admin", "keyadder"])),
    tags = ["Miscellaneous", "Data-Administration"],
    request_body = models::ReplacementPutRequest<models::Gremium>,
    responses(PutResponse)
)]
async fn gremien_put(
    claims: Claims,
    State(api_impl): State<Arc<PazufaServer>>,
    Json(body): Json<models::ReplacementPutRequest<models::Gremium>>,
) -> PutResponse {
    // Authentication

    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libmisca::gremien_put(api_impl.as_ref(), &body)
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
    description = "Retrieves a list of committees filtered by optional parameters. Returns committees matching the specified criteria from parliament bodies and electoral periods.",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::GremienGetQueryParams),
    responses(ListingGetResponse<models::Gremium>)
)]
async fn gremien_get(
    Query(query_params): Query<params::GremienGetQueryParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> ListingGetResponse<models::Gremium> {
    libmisc::gremien_get(api_impl.as_ref(), &query_params)
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
    use pazufa_models::{self as models, params};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_gremien_put() {
        let mut scenario = TestSetup::new("test_gremien_put").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::gremien_put(
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
    async fn test_gremien_delete() {
        let mut scenario = TestSetup::new("test_gremien_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::gremien_delete_by_param(
                hm,
                axum::extract::Query(params::GremienDeleteByParamQueryParams::default()),
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
