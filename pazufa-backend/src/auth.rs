use crate::LTZFServer;
use axum::extract::{Path, Query, State};
use axum::response::Json;
use pazufa_backend_lib::api::auth::{self as libauth, Claims, KeytagListing};
use pazufa_backend_lib::api::{AuthListingGetResponse, DeleteResponse};
use pazufa_backend_lib::models::auth::{APIKeyCreate, Argon2KeyGenerator};
use pazufa_backend_lib::models::{APIKey, APIKeyStatus, APIScope, KeyTag, params};
use pazufa_backend_lib::utils::header_params::HeaderParams;
use std::sync::Arc;
use tracing::warn;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[derive(OpenApi)]
#[openapi(
    paths(
        auth_post,
        auth_delete,
        auth_status,
        auth_rotate,
        auth_listing,
        auth_listing_keytag
    ),
    components(schemas(APIKeyStatus, KeytagListing))
)]
pub(super) struct AuthApi;

pub(super) fn router() -> OpenApiRouter<Arc<LTZFServer>> {
    OpenApiRouter::new()
        .routes(routes!(auth_post, auth_delete))
        .routes(routes!(auth_status,))
        .routes(routes!(auth_rotate,))
        .routes(routes!(auth_listing,))
        .routes(routes!(auth_listing_keytag,))
}

/// Entry point to create a new key with a given scope
/// returns the key as plain string
#[utoipa::path(
        post,
        path = "",
        tag = "Authentifizierung",
        security(("apiKey" = ["keyadder"])),
        request_body=APIKeyCreate,
        responses(libauth::AuthPostResponse)
    )]
#[tracing::instrument(skip_all, fields(claim=%scope))]
async fn auth_post(
    Claims(scope, index): Claims,
    State(api_impl): State<Arc<LTZFServer>>,
    Json(body): Json<APIKeyCreate>,
) -> libauth::AuthPostResponse {
    // Authentication
    if scope != APIScope::KeyAdder {
        return libauth::AuthPostResponse::Unauth;
    }
    async fn inner_with_result(
        server: &LTZFServer,
        claims: &Claims,
        body: &APIKeyCreate,
    ) -> pazufa_backend_lib::Result<String> {
        let key: APIKey<Argon2KeyGenerator> = pazufa_backend_lib::db::auth::create_apikey(
            claims.1,
            body.scope,
            body.expires_at,
            body.keytag_prefix.as_deref(),
            &server.sqlx_db,
        )
        .await?;
        Ok(key.to_string())
    }

    match inner_with_result(api_impl.as_ref(), &Claims(APIScope::KeyAdder, index), &body).await {
        Ok(key) => libauth::AuthPostResponse::KeyCreated(key),
        Err(e) => {
            warn!("Error in auth_post: {e}");
            libauth::AuthPostResponse::Error(e.to_string())
        }
    }
}

#[utoipa::path(
    get,
    path = "/status",
    tag = "Authentifizierung",
    description="Get some information about your own key",
    security(("apiKey" = ["collector", "admin", "keyadder"])),
    responses(libauth::AuthStatusResponse)
    )]
#[tracing::instrument(skip_all)]
async fn auth_status(
    claims: Claims,
    State(api_impl): State<Arc<LTZFServer>>,
) -> libauth::AuthStatusResponse {
    pazufa_backend_lib::db::retrieve::auth::auth_status(claims.1, &api_impl.sqlx_db)
        .await
        .map_or_else(
            |error| {
                warn!("Could not retrieve Auth Status: {error}");
                libauth::AuthStatusResponse::Error(error.to_string())
            },
            libauth::AuthStatusResponse::Status,
        )
}

/// `AuthRotate` - POST /api/v1/auth/rotate
/// Endpoint to rotate your own key for a fresh one. The old key is valid for a certain transition
/// period after which it automatically invalidates. If called repeatedly it makes sure that only
/// ever a maximum of two (1) keys is valid, a maximum of one of which is in rotation.
/// This endpoint requires authentication.
/// # Security
/// There should always ever be at most two keys in an active rotation relationship (`rotated_for!=NULL` && `deleted_by!=NULL`)
/// And the rotation date must never be expanded, otherwise you can just prolong your old key's life indefinitely
///
/// Names: `old_key`: The key that the request has been authenticated with
/// `new_key`: the key that is created and succeeds `old_key`
/// `some_key`: Any key in the database
/// Cases:
/// 0. `old_key.rotated_for` != None  => SET `old_key.rotated_for.deleted_by=himself`, `old_key.rotated_for=new_key`,
/// 1. `some_key.rotated_for=old_key` => `some_key.deleted_by=some_key`, `old_key.rotated_for=new_key`
#[utoipa::path(
    post,
    path = "/rotate",
    tag = "Authentifizierung",
    description = "Key rotation endpoint for creating a new API key while maintaining the existing one for a transition period of one day.  The old key remains valid until the specified rotation_complete_date, after which it is automatically revoked. Rotates only the own key if it is not invalid.",
    security(("apiKey" = ["collector", "admin", "keyadder"])),
    responses(libauth::AuthRotateResponse)
)]
async fn auth_rotate(
    claims: Claims,
    State(api_impl): State<Arc<LTZFServer>>,
) -> libauth::AuthRotateResponse {
    libauth::auth_rotate(Arc::clone(&api_impl).as_ref(), &claims)
        .await
        .unwrap_or_else(|why| {
            warn!("Error rotating: {why}");
            libauth::AuthRotateResponse::Error(why.to_string())
        })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    delete,
    path = "",
    tag = "Authentifizierung",
    security(("apiKey" = ["keyadder"])),
    description = "Key adder interface for removing API keys from the system. Allows administrators and key management services to revoke access by deleting an existing API key.",
    params(params::ApiKeyDelete),
    responses(DeleteResponse)
    )]
async fn auth_delete(
    HeaderParams(api_key_delete): HeaderParams<params::ApiKeyDelete>,
    claims: Claims,
    State(api_impl): State<Arc<LTZFServer>>,
) -> DeleteResponse {
    // Authentication
    if claims.0 != APIScope::KeyAdder {
        return DeleteResponse::Unauth;
    }

    libauth::auth_delete(api_impl.as_ref(), &claims, &api_key_delete)
        .await
        .map_or_else(
            |e| {
                warn!("Error: unable to delete key: {e}");
                DeleteResponse::Error(e.to_string())
            },
            |()| DeleteResponse::Success,
        )
}

#[utoipa::path(
    get,
    path = "/keys",
    tag = "Authentifizierung",
    security(("apiKey" = ["keyadder"])),
    params(params::ListingQueryParams),
    responses(AuthListingGetResponse<KeyTag>)
)]
#[tracing::instrument(skip_all)]
async fn auth_listing(
    claims: Claims,
    Query(query_params): Query<params::ListingQueryParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> AuthListingGetResponse<KeyTag> {
    // Authentication
    if claims.0 != APIScope::KeyAdder {
        return AuthListingGetResponse::Unauth;
    }

    libauth::auth_listing(api_impl.as_ref(), &claims, &query_params)
        .await
        .unwrap_or_else(|e| {
            warn!("Error during auth_listing: {e}");
            AuthListingGetResponse::Error(e.to_string())
        })
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/keys/{keytag}",
    tag = "Authentifizierung",
    security(("apiKey" = ["keyadder"])),
    params(params::AuthListingKeytagPathParams),
    responses(libauth::AuthListingKeytagResponse)
)]
async fn auth_listing_keytag(
    claims: Claims,
    Path(path_params): Path<params::AuthListingKeytagPathParams>,
    State(api_impl): State<Arc<LTZFServer>>,
) -> libauth::AuthListingKeytagResponse {
    // Authentication
    if claims.0 != APIScope::KeyAdder {
        return libauth::AuthListingKeytagResponse::Unauth;
    }

    match libauth::auth_listing_keytag(api_impl.as_ref(), &path_params).await {
        Ok(body) => libauth::AuthListingKeytagResponse::KeytagListing(body),
        Err(e) => {
            warn!("Unable to list keytag's actions: {e}");
            libauth::AuthListingKeytagResponse::Error(e.to_string())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::testing::{TestSetup, authtesting};
    use pazufa_backend_lib::api::auth::{self as libauth, Claims};
    use pazufa_backend_lib::models::params::{self, ApiKeyDelete};
    use pazufa_backend_lib::models::{APIScope, auth::APIKeyCreate};
    use pazufa_backend_lib::utils::header_params::HeaderParams;
    use std::sync::Arc;
    use tracing_test::traced_test;

    #[tokio::test]
    async fn test_auth_listing_keytag() {
        let mut scenario = TestSetup::new("test_auth_listing_keytag").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::auth_listing_keytag(
                hm,
                axum::extract::Path(params::AuthListingKeytagPathParams {
                    keytag: "ltzf_asd8fj9".to_owned(),
                }),
                axum::extract::State(server),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder],
            call_closure,
            libauth::AuthListingKeytagResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_auth_listing() {
        let mut scenario = TestSetup::new("test_auth_listing").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::auth_listing(
                hm,
                axum::extract::Query(params::ListingQueryParams::default()),
                axum::extract::State(server),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder],
            call_closure,
            pazufa_backend_lib::api::AuthListingGetResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    #[traced_test]
    async fn test_auth_delete() {
        let mut scenario = TestSetup::new("test_auth_delete").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |claims: Claims, server| async {
            let del_key = HeaderParams(ApiKeyDelete("nothing".to_owned()));
            super::auth_delete(del_key, claims, axum::extract::State(server)).await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder],
            call_closure,
            pazufa_backend_lib::api::DeleteResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }

    #[tokio::test]
    async fn test_auth_rotate() {
        let mut scenario = TestSetup::new("test_auth_rotate").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure =
            |hm, server| async { super::auth_rotate(hm, axum::extract::State(server)).await };
        authtesting(
            Arc::clone(&server),
            &[APIScope::Collector, APIScope::Admin, APIScope::KeyAdder],
            call_closure,
            libauth::AuthRotateResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_auth_status() {
        let mut scenario = TestSetup::new("test_auth_status").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure =
            |hm, server| async { super::auth_status(hm, axum::extract::State(server)).await };
        authtesting(
            Arc::clone(&server),
            &[APIScope::Collector, APIScope::Admin, APIScope::KeyAdder],
            call_closure,
            libauth::AuthStatusResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
    #[tokio::test]
    async fn test_auth_post() {
        let mut scenario = TestSetup::new("test_auth_post").await;
        let server = Arc::new(scenario.server.take().expect("Expected a server value"));

        let call_closure = |hm, server| async {
            super::auth_post(
                hm,
                axum::extract::State(server),
                axum::Json(APIKeyCreate {
                    expires_at: None,
                    scope: APIScope::Collector,
                    keytag_prefix: None,
                }),
            )
            .await
        };
        authtesting(
            Arc::clone(&server),
            &[APIScope::KeyAdder],
            call_closure,
            libauth::AuthPostResponse::Unauth,
        )
        .await;

        let mut server = Arc::into_inner(server);
        scenario.server = server.take();
        scenario.teardown().await;
    }
}
