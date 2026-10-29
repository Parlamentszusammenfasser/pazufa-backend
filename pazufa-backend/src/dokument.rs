use axum::extract::{Json, Path, Query, State};
use axum::http::HeaderMap;
use pazufa_backend_lib::PazufaServer;
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::api::{
    DeleteResponse, GetSingleResponse, ListingGetResponse, PazufaOrFtmListingResponse, PutResponse,
    misc as libmisc, misc_auth as libmisca,
};
use pazufa_models::{
    self as models,
    enumerations::{APIScope, DokumentFormat, Mime},
    params,
};
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

pub(super) fn router() -> OpenApiRouter<Arc<PazufaServer>> {
    OpenApiRouter::new()
        .routes(routes!(
            dokument_delete_id,
            dokument_put_id,
            dokument_get_by_id
        ))
        .routes(routes!(dokumente_get))
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
    State(api_impl): State<Arc<PazufaServer>>,
) -> DeleteResponse {
    // Authentication

    if !matches!(claims.0, APIScope::KeyAdder | APIScope::Admin) {
        warn!("Not authorized");
        return DeleteResponse::Unauth;
    }

    pazufa_backend_lib::db::delete::delete_dokument_by_api_id(path_params.api_id, &api_impl.sqlx_db)
        .await
        .map_or_else(
            |e| {
                warn!("{e}");
                DeleteResponse::Error(e.to_string())
            },
            |()| DeleteResponse::Success,
        )
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
    State(api_impl): State<Arc<PazufaServer>>,
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

#[utoipa::path(
    get,
    path = "",
    description = "Retrieves a Paginated list of documents from the Backend. Ordered by zp_modifiziert",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(
        params::IfModSince,
        params::ListingQueryParams,
        params::DocumentFormatParams,
        ),
    responses(
        PazufaOrFtmListingResponse
    )
)]
async fn dokumente_get(
    headers: HeaderMap,
    Query(format_param): Query<params::DocumentFormatParams>,
    Query(listing_params): Query<params::ListingQueryParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> PazufaOrFtmListingResponse {
    let fmt = format_param.format.unwrap_or(DokumentFormat::Pazufa);
    if fmt == DokumentFormat::FTM {
        return PazufaOrFtmListingResponse::NotImpl;
    }
    let ims = match params::IfModSince::try_from(headers.get(axum::http::header::IF_MODIFIED_SINCE))
    {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return PazufaOrFtmListingResponse::BadReq;
        }
    };
    libmisc::dokumente_get_by_params(
        api_impl.as_ref(),
        fmt,
        listing_params,
        ims.if_modified_since,
    )
    .await
    .map_or_else(
        |e| {
            error!(error=?e);
            PazufaOrFtmListingResponse::Error(e.to_string())
        },
        |thing| match thing {
            ListingGetResponse::NotMod => PazufaOrFtmListingResponse::NotMod,
            ListingGetResponse::BadReq => PazufaOrFtmListingResponse::BadReq,
            ListingGetResponse::NoContent => PazufaOrFtmListingResponse::NoContent,
            ListingGetResponse::Error(e) => PazufaOrFtmListingResponse::Error(e),
            ListingGetResponse::Success {
                body,
                prp,
                path_prefix,
            } => PazufaOrFtmListingResponse::Pazufa {
                body,
                prp,
                path_prefix,
            },
        },
    )
}
#[utoipa::path(
    get,
    path = "/{api_id}",
    description =  "Retrieves a specific document by its unique identifier. Returns complete document data including title, full text, metadata, and author information. If called by admin or higher, this returns a list of last scrapers/collectors touching the object",
    tags = ["Miscellaneous", "Unauthorisiert"],
    params(
        params::IfModSince,
        params::ApiIdPathParam,
        params::DocumentFormatParams,
        ),
    responses(GetSingleResponse<models::Dokument>)
)]
#[tracing::instrument(skip_all)]
async fn dokument_get_by_id(
    headers: HeaderMap,
    Path(path_params): Path<params::ApiIdPathParam>,
    Query(q_params): Query<params::DocumentFormatParams>,
    State(api_impl): State<Arc<PazufaServer>>,
) -> GetSingleResponse<models::Dokument> {
    if let Some(fmt) = q_params.format
        && fmt == DokumentFormat::FTM
    {
        return GetSingleResponse::NotImpl;
    }
    let ims = match params::IfModSince::try_from(headers.get(axum::http::header::IF_MODIFIED_SINCE))
    {
        Ok(thing) => thing,
        Err(e) => {
            warn!("{e}");
            return GetSingleResponse::BadReq;
        }
    };
    libmisc::dokument_get_by_id(api_impl.as_ref(), &ims, &path_params)
        .await
        .unwrap_or_else(|e| {
            error!(error = ?e);
            GetSingleResponse::Error(e.to_string())
        })
}

#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use axum::extract::Query;
    use axum::http::Uri;
    use pazufa_models::APIScope;
    use pazufa_models::enumerations::DokumentFormat;
    use pazufa_models::{self as models, params};
    use std::sync::Arc;

    #[test]
    fn test_dokument_format_params_query_deserialization() {
        let uri: Uri = "http://example.com/dokument/id?format=ftm"
            .parse()
            .expect("static string");
        let result =
            Query::<params::DocumentFormatParams>::try_from_uri(&uri).expect("static string");
        assert_eq!(result.0.format, Some(DokumentFormat::FTM));

        let uri: Uri = "http://example.com/dokument/id?format=pazufa"
            .parse()
            .expect("static string");
        let result =
            Query::<params::DocumentFormatParams>::try_from_uri(&uri).expect("static string");
        assert_eq!(result.0.format, Some(DokumentFormat::Pazufa));

        let uri: Uri = "http://example.com/dokument/id"
            .parse()
            .expect("static string");
        let result =
            Query::<params::DocumentFormatParams>::try_from_uri(&uri).expect("static string");
        assert_eq!(result.0.format, None);

        let uri: Uri = "http://example.com/dokument/id?format=invalid"
            .parse()
            .expect("static string");
        assert!(
            Query::<params::DocumentFormatParams>::try_from_uri(&uri).is_err(),
            "unknown `format` value should fail to deserialize"
        );
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
