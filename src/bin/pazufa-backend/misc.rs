use axum::extract::{Json, Path, Query, State};
use pazufa_backend_lib::LTZFServer;
use pazufa_backend_lib::api::auth::{APIScope, Claims};
use pazufa_backend_lib::api::{
    DeleteResponse, GetSingleResponse, ListingGetResponse, PutResponse, misc as libmisc,
    misc_auth as libmisca,
};
use pazufa_backend_lib::models;
use pazufa_backend_lib::models::params;
use std::sync::Arc;
use tracing::{error, warn};

#[tracing::instrument(skip_all)]
#[utoipa::path(
    method(delete),
    path = "/api/v2/autoren",
    operation_id = "autoren_delete_by_param",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::AutorenDeleteByParamQueryParams),
    responses(DeleteResponse)
)]
pub async fn autoren_delete_by_param(
    claims: Claims,
    Query(query_params): Query<params::AutorenDeleteByParamQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
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
    method(delete),
    path = "/api/v2/gremien",
    operation_id = "gremien_delete_by_param",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::GremienDeleteByParamQueryParams),
    responses(DeleteResponse)
)]
pub async fn gremien_delete_by_param(
    claims: Claims,
    Query(query_params): Query<params::GremienDeleteByParamQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
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
    method(delete),
    path = "/api/v2/enumeration/{name}/{item}",
    operation_id = "enum_delete",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::EnumDeletePathParams),
    responses(DeleteResponse)
)]
pub async fn enum_delete(
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
    delete,
    path = "/api/v2/dokument/{api_id}",
    description = "Administrative endpoint to remove a document from the system. This operation cannot be undone and may affect other resources referencing this document.",
    tags = ["Miscellaneous", "Data-Administration"],
    operation_id = "dokument_delete_id",
    security(("apiKey" = [  "admin", "keyadder"])),
    params(params::ApiIdPathParam),
    responses(DeleteResponse)
)]
pub async fn dokument_delete_id(
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
    path = "/api/v2/dokument/{api_id}",
    description = "Administrative endpoint to upload or update documents for future reference. Creates a new document with the specified ID or replaces an existing one.",
    operation_id = "dokument_put_id",
    tags = ["Miscellaneous", "Data-Administration"],
    params(params::ApiIdPathParam),
    security(("apiKey" = [  "admin", "keyadder"])),
    request_body = models::Dokument,
    responses(PutResponse)
)]
#[tracing::instrument(skip_all)]
pub async fn dokument_put_id(
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

#[tracing::instrument(skip_all)]
#[utoipa::path(
    put,
    path = "/api/v2/autoren",
    description = "Administrative endpoint to add or update multiple authors at once. Creates new authors or replaces existing ones based on matching names and organizations.",
    operation_id = "autoren_put",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    request_body = models::ReplacementPutRequest<models::Autor>,
    responses(PutResponse)
)]
pub async fn autoren_put(
    claims: Claims,
    State(api_impl): State<Arc<LTZFServer>>,
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
    put,
    path = "/api/v2/enumeration/{name}",
    description = "Administrative endpoint to add or update values in a specified enumeration type. Replaces the entire set of values for the enumeration.",
    operation_id = "enum_put",
    tags = ["Miscellaneous", "Data-Administration"],
    security(("apiKey" = [  "admin", "keyadder"])),
    params(
        params::EnumPathParams
        ),
    request_body = models::ReplacementPutRequest<String>,
    responses(PutResponse)
)]
pub async fn enum_put(
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
    put,
    path = "/api/v2/gremien",
    description = "Administrative endpoint to add or update multiple committees at once. Creates new committees or replaces existing ones based on matching parliament, electoral period, and name.",
    operation_id = "gremien_put",
    security(("apiKey" = [  "admin", "keyadder"])),
    tags = ["Miscellaneous", "Data-Administration"],
    request_body = models::ReplacementPutRequest<models::Gremium>,
    responses(PutResponse)
)]
pub async fn gremien_put(
    claims: Claims,
    State(api_impl): State<Arc<LTZFServer>>,
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
    path = "/api/v2/autoren",
    description = "Retrieves a list of authors filtered by optional parameters. Returns authors matching the specified criteria including name fragments, professional field, and organization.",
    operation_id = "autoren_get",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::AutorenGetQueryParams),
    responses(ListingGetResponse<models::Autor>)
)]
pub async fn autoren_get(
    Query(query_params): Query<params::AutorenGetQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> ListingGetResponse<models::Autor> {
    libmisc::autoren_get(api_impl.as_ref(), &query_params)
        .await
        .unwrap_or_else(|e| {
            error!(error = ?e);
            ListingGetResponse::Error(e.to_string())
        })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v2/enumeration/{name}",
    description = "Retrieves values from a specified enumeration type. Returns a list of valid values that can be used in other API operations, optionally filtered by a substring.",
    operation_id = "enum_get",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::EnumGetQueryParams),
    responses(ListingGetResponse<String>)
)]
pub async fn enum_get(
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

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v2/gremien",
    description = "Retrieves a list of committees filtered by optional parameters. Returns committees matching the specified criteria from parliament bodies and electoral periods.",
    operation_id = "gremien_get",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::GremienGetQueryParams),
    responses(ListingGetResponse<models::Gremium>)
)]
pub async fn gremien_get(
    Query(query_params): Query<params::GremienGetQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> ListingGetResponse<models::Gremium> {
    libmisc::gremien_get(api_impl.as_ref(), &query_params)
        .await
        .unwrap_or_else(|e| {
            error!(error = ?e);
            ListingGetResponse::Error(e.to_string())
        })
}

#[utoipa::path(
    get,
    path = "/api/v2/dokument/{api_id}",
    description =  "Retrieves a specific document by its unique identifier. Returns complete document data including title, full text, metadata, and author information. If called by admin or higher, this returns a list of last scrapers/collectors touching the object",
    operation_id = "dokument_get_by_id",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(params::ApiIdPathParam),
    responses(GetSingleResponse<models::Dokument>)
)]
#[tracing::instrument(skip_all)]
pub async fn dokument_get_by_id(
    Path(path_params): Path<params::ApiIdPathParam>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> GetSingleResponse<models::Dokument> {
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
