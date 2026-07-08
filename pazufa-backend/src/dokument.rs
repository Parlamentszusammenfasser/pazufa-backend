use axum::extract::{Json, Path, Query, State};
use pazufa_backend_lib::LTZFServer;
use pazufa_backend_lib::api::auth::{APIScope, Claims};
use pazufa_backend_lib::api::{
    DeleteResponse, GetSingleResponse, PutResponse, misc as libmisc, misc_auth as libmisca,
};
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::enumerations::{DokumentFormat, Mime};
use pazufa_backend_lib::models::params;
use std::sync::Arc;
use tracing::{error, warn};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(
    paths(dokument_delete_id, dokument_put_id, dokument_get_by_id),
    components(schemas(DokumentFormat, Mime))
)]
pub(super) struct DokumentApi;

pub(super) fn router() -> OpenApiRouter<Arc<LTZFServer>> {
    OpenApiRouter::new().routes(routes!(
        dokument_delete_id,
        dokument_put_id,
        dokument_get_by_id
    ))
    //        .routes(routes!(dokumente_get))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "/{api_id}",
    description = "Administrative endpoint to remove a document from the system. This operation cannot be undone and may affect other resources referencing this document.",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::ApiIdPathParam),
    responses(DeleteResponse)
)]
async fn dokument_delete_id(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> DeleteResponse {
    // Authentication

    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Not authorized");
        return DeleteResponse::Unauth;
    }

    libmisca::dokument_delete_id(api_impl.as_ref(), &path_params)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            DeleteResponse::Error(e.to_string())
        })
}
#[utoipa::path(
    put,
    path = "/{api_id}",
    description = "Administrative endpoint to upload or update documents for future reference. Creates a new document with the specified ID or replaces an existing one.",
    tags = ["Miscellaneous", "Data-Administration"],
    params(params::ApiIdPathParam),
    security(("apiKey" = ["admin", "keyadder"])),
    request_body = models::Dokument,
    responses(PutResponse)
)]
#[tracing::instrument(skip_all)]
async fn dokument_put_id(
    claims: Claims,
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
    Json(body): Json<models::Dokument>,
) -> PutResponse {
    // Authentication
    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Not authorized");
        return PutResponse::Unauth;
    }

    libmisca::dokument_put_id(api_impl.as_ref(), claims.1, &path_params, &body)
        .await
        .unwrap_or_else(|e| {
            warn!("{e}");
            PutResponse::Error(e.to_string())
        })
}

//#[utoipa::path(
//    get,
//    path = "/dokumente",
//    description = "Retrieves a Paginated list of documents from the Backend. Ordered by zp_modifiziert",
//    tags = ["Miscellaneous", "Unauthorisiert"],
//    params(
//        params::IfModSince,
//        params::ListingQueryParams,
//        ("format" = models::enumerations::DokumentFormat, Query, description = "Format of return of the documents. Defaults to pazufa")
//        ),
//    responses(
//        PazufaOrFtmListingResponse
//    )
//)]
//async fn dokumente_get(
//    Query(q_params): Query<models::enumerations::DokumentFormat>,
//    State(api_impl): State<Arc<LTZFServer>>,
//) -> PazufaOrFtmListingResponse {
//    PazufaOrFtmListingResponse::NotImpl
//}

#[utoipa::path(
    get,
    path = "/{api_id}",
    description =  "Retrieves a specific document by its unique identifier. Returns complete document data including title, full text, metadata, and author information. If called by admin or higher, this returns a list of last scrapers/collectors touching the object",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(
        params::ApiIdPathParam,
        ("format" = models::enumerations::DokumentFormat, Query, description = "Format of return of the documents. Defaults to pazufa")
        ),
    responses(GetSingleResponse<models::Dokument>)
)]
#[tracing::instrument(skip_all)]
async fn dokument_get_by_id(
    Path(path_params): Path<params::ApiIdPathParam>,
    Query(q_params): Query<models::enumerations::DokumentFormat>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> GetSingleResponse<models::Dokument> {
    if q_params == DokumentFormat::FTM {
        return GetSingleResponse::NotImpl;
    }
    libmisc::dokument_get_by_id(api_impl.as_ref(), &path_params)
        .await
        .unwrap_or_else(|e| {
            error!(error = ?e);
            GetSingleResponse::Error(e.to_string())
        })
}

#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::api::auth::APIScope;
    use pazufa_backend_lib::models::{self, params};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_dokument_put() {
        let mut scenario = TestSetup::new("test_dok_put_id_auth").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::dokument_put_id(
                hm,
                axum::extract::Path(params::ApiIdPathParam {
                    api_id: uuid::Uuid::nil(),
                }),
                axum::extract::State(server),
                axum::Json(models::Dokument::default()),
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
    async fn test_dokument_delete() {
        let mut scenario = TestSetup::new("test_dok_delete_auth").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::dokument_delete_id(
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
