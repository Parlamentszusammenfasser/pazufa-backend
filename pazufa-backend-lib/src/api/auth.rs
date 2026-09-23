use crate::api::{AuthListingGetResponse, PaginationResponsePart};
use crate::models::APIScope;
use crate::models::auth::{
    APIKey, APIKeyStatus, Argon2KeyGenerator, HashAlgorithm, NoopKeyGenerator, Sha256KeyGenerator,
};
use crate::models::params::{self, AuthListingKeytagPathParams, ListingQueryParams, XAPIKey};
use crate::utils::as_option;
use crate::utils::header_params::HeaderParams;
use crate::{LTZFServer, Result, error::LTZFError, models};
use argon2::password_hash::SaltString;
use axum::extract::FromRequestParts;
use axum::http::{StatusCode, request::Parts};
use axum::response::{IntoResponse, Json, Response};
use chrono::{DateTime, Utc};
use models::KeyTag;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};
use utoipa::IntoResponses;
use utoipa::ToSchema;

#[derive(Debug)]
pub struct Claims(pub APIScope, pub i32);

impl std::fmt::Display for APIScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Admin => "admin",
                Self::Collector => "collector",
                Self::KeyAdder => "keyadder",
            }
        )
    }
}

impl Claims {
    /// takes a raw header map, and extracts whichever claims are available.
    /// should only be called if `x-api-key` authentication is required. If it is optional, ignore the
    /// result of this call and proceed unauthenticated.
    /// # Errors
    /// Can fail if:
    /// - there is no key in the header map
    /// - there is a key in the header map, but it cannot be matched with a key from the database
    /// - there is both of those but database errors say no
    pub async fn extract(server: &LTZFServer, XAPIKey(key): XAPIKey) -> Result<Self> {
        #[derive(FromRow)]
        struct Response {
            pub id: i32,
            pub deleted_by: Option<i32>,
            pub expires_at: DateTime<Utc>,
            pub scope: APIScope,
            pub salt: Vec<u8>,
            pub key_hash: String,
            pub algo: HashAlgorithm,
        }
        let api_key: APIKey<NoopKeyGenerator> = key.clone().try_into()?;

        debug!("Authenticating Key: `{}`", api_key.keytag());

        if let Some(Response {id, deleted_by, expires_at, scope, salt, key_hash, algo}) = sqlx::query_as!(Response,
            r#"SELECT k.id, k.deleted_by, k.expires_at, s.value as "scope: APIScope", k.salt, k.key_hash, ha.value as "algo: HashAlgorithm"
            FROM api_keys k
            INNER JOIN api_scope s ON s.id = k.scope
            INNER JOIN hash_algorithm ha ON ha.id = k.algo
            WHERE keytag = $1"#,
            api_key.keytag()
        )
        .fetch_optional(&server.sqlx_db)
        .await?
        {
            let incoming_hash = match algo {
                HashAlgorithm::Sha256 => {
                    let key : APIKey<Sha256KeyGenerator> = key.clone().try_into()?;
                    key.hash(SaltString::encode_b64(&salt)?.as_salt())?
                },
                HashAlgorithm::Argon2id => {
                    let key : APIKey<Argon2KeyGenerator> = key.clone().try_into()?;
                    key.hash(SaltString::encode_b64(&salt)?.as_salt())?

                }
            };
            if key_hash != incoming_hash {
                warn!("Hash is not matching");
                Err(crate::error::DataValidationError::Unauthorized {
                        reason: format!("API Key is not valid. Tag: {}", api_key.keytag()),
                    }.into())
            } else if let Some(deleted_by) = deleted_by {
                if deleted_by == id {
                    warn!("API Key was valid but was either rotated or expired");
                    Err(crate::error::DataValidationError::Unauthorized {
                            reason: format!(
                                "API Key was valid but was either rotated or expired. Tag: {}", api_key.keytag()
                            )
                    }.into())
                } else {
                    let delkeytag = sqlx::query!("SELECT keytag FROM api_keys WHERE id=$1", deleted_by)
                        .map(|r| r.keytag)
                        .fetch_one(&server.sqlx_db)
                        .await?;
                    warn!("Key was deleted by {delkeytag}");
                    Err(LTZFError::Validation {
                        source: Box::new(crate::error::DataValidationError::Unauthorized {
                            reason: format!(
                                "API Key was valid but is deleted. Tag: {}\nAdministrator Key: {delkeytag}", api_key.keytag()
                            ),
                        }),
                    })
                }
            } else if expires_at < chrono::Utc::now() {
                sqlx::query!("UPDATE api_keys SET deleted_by = $1 WHERE id = $1", id)
                    .execute(&server.sqlx_db)
                    .await?;
                warn!("Key was valid but has expired since last use");
                Err(LTZFError::Validation {
                    source: Box::new(crate::error::DataValidationError::Unauthorized {
                        reason: format!("API Key was valid but has expired. Tag: {}", api_key.keytag()),
                    }),
                })
            } else {
                let scope = Claims(scope, id);
                sqlx::query!(
                    "UPDATE api_keys SET last_used = $1 WHERE id = $2",
                    chrono::Utc::now(),
                    id
                )
                .execute(&server.sqlx_db)
                .await?;
                debug!("Scope of key with tag`{}`: {:?}", api_key.keytag(), scope.0);
                Ok(scope)
            }
        } else {
            warn!("Key was not found in the database");
            Err(LTZFError::Validation {
                source: Box::new(crate::error::DataValidationError::Unauthorized {
                    reason: "API Key was not found in the Database".to_string(),
                }),
            })
        }
    }
}

impl FromRequestParts<Arc<LTZFServer>> for Claims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<LTZFServer>,
    ) -> std::result::Result<Self, Self::Rejection> {
        let HeaderParams(key) = HeaderParams::<XAPIKey>::from_request_parts(parts, state)
            .await
            .map_err(|_err| {
                (StatusCode::UNAUTHORIZED, "Missing X-API-Key header.").into_response()
            })?;
        Self::extract(state, key)
            .await
            .map_err(|err| (StatusCode::UNAUTHORIZED, format!("{err}")).into_response())
    }
}
#[derive(IntoResponses, PartialEq, Debug)]
pub enum AuthPostResponse {
    #[response(status = 201)]
    KeyCreated(String),

    #[response(status = 400)]
    BadRequest,
    #[response(status = 401)]
    Unauth,
    #[response(status = 500)]
    Error(String),
}
impl IntoResponse for AuthPostResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
            Self::BadRequest => StatusCode::BAD_REQUEST.into_response(),
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Self::KeyCreated(created_key) => (StatusCode::CREATED, created_key).into_response(),
        }
    }
}

#[derive(IntoResponses, PartialEq, Debug)]
pub enum AuthStatusResponse {
    #[response(status = 200, content_type = "application/json")]
    Status(APIKeyStatus),
    #[response(status = 401)]
    Unauth,
    #[response(status = 500)]
    Error(String),
}
impl IntoResponse for AuthStatusResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Status(status) => (StatusCode::OK, Json(status)).into_response(),
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoResponses, PartialEq)]
pub enum AuthRotateResponse {
    #[response(status = 201, content_type = "application/json")]
    Created {
        /// The newly created API key (shown only once)
        new_api_key: String,

        /// Confirmed date when the old key will be invalidated
        rotation_complete_date: chrono::DateTime<chrono::Utc>,
    },

    #[response(status = 401)]
    Unauth,
    #[response(status = 500)]
    Error(String),
}
impl IntoResponse for AuthRotateResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Created {
                new_api_key,
                rotation_complete_date,
            } => (
                StatusCode::CREATED,
                axum::response::Json(Self::Created {
                    new_api_key,
                    rotation_complete_date,
                }),
            )
                .into_response(),
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
        }
    }
}

#[derive(Debug, IntoResponses, PartialEq)]
pub enum AuthListingKeytagResponse {
    #[response(status = 200, content_type = "application/json")]
    KeytagListing(KeytagListing),
    #[response(status = 401)]
    Unauth,
    #[response(status = 500)]
    Error(String),
}
impl IntoResponse for AuthListingKeytagResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Error(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Self::Unauth => StatusCode::UNAUTHORIZED.into_response(),
            Self::KeytagListing(body) => (StatusCode::OK, Json(body)).into_response(),
        }
    }
}

/// Function that rotates a key for a fresh one, keeping track so that only ever two keys are active
/// at any one time.
/// # Errors
/// Can fail if the database operations fail
#[instrument(skip_all, fields(claim=%claims.0))]
pub async fn auth_rotate(server: &LTZFServer, claims: &Claims) -> Result<AuthRotateResponse> {
    let mut tx = server.sqlx_db.begin().await?;
    let old_key_entry = sqlx::query!(
        "SELECT scope,value as named_scope, expires_at, created_at, rotated_for, keytag
            FROM api_keys INNER JOIN api_scope ON scope=api_scope.id 
            WHERE api_keys.id = $1",
        claims.1
    )
    .fetch_one(&mut *tx)
    .await?;

    // new key, replacing the old one
    let api_key = crate::models::auth::APIKey::<crate::models::auth::Argon2KeyGenerator>::default();
    let new_salt = crate::models::auth::generate_salt();
    let new_hash = api_key.hash(new_salt.as_salt())?;
    let mut vec_data = vec![0; new_salt.len()];
    let salt_bytes: Vec<_> = new_salt.decode_b64(&mut vec_data)?.into();

    let new_id = sqlx::query!(
        "INSERT INTO api_keys(key_hash, created_by, expires_at, scope, salt, keytag, algo)
            VALUES
            ($1, $2, $3, $4,$5, $6, ( SELECT id FROM hash_algorithm WHERE value = $7 ))
            RETURNING id",
        new_hash,
        claims.1,
        chrono::Utc::now() + (old_key_entry.expires_at - old_key_entry.created_at),
        old_key_entry.scope,
        salt_bytes,
        api_key.keytag(),
        HashAlgorithm::Argon2id.to_string()
    )
    .map(|r| r.id)
    .fetch_one(&mut *tx)
    .await?;

    // at this point, fix up some failure cases:
    // 1. if the old key is in rotation state (rotated_for is not NULL): invalidate the one it is rotated for
    if let Some(rf) = old_key_entry.rotated_for {
        debug!("#1: old key is in rotation state: Invalidate the one it is rotated for");
        sqlx::query!("UPDATE api_keys SET deleted_by = id WHERE id = $1", rf)
            .execute(&mut *tx)
            .await?;
    }

    // 2. if the old key has a corresponding rotated_for entry: invalidate that one, and handle the old one like default
    // this prohibits creating any number of successor keys in an infinite chain. That way there is at most one pair of [old key] -> [new key]
    let r = sqlx::query!("SELECT id FROM api_keys WHERE rotated_for = $1", claims.1)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(inv_id) = r {
        debug!("#2: old key has a 'fresh_key' role for a different key: Invalidate that one");
        sqlx::query!(
            "UPDATE api_keys SET deleted_by = id WHERE id = $1",
            inv_id.id
        )
        .execute(&mut *tx)
        .await?;
    }
    // set expiry to min(1d, existing) (basically: prohibit overriding the expiration date of an existing key)
    let rot_expiration_date = chrono::Utc::now() + chrono::Duration::days(1);
    sqlx::query!(
        "UPDATE api_keys 
            SET expires_at = LEAST($2::timestamptz, expires_at), rotated_for = $3
            WHERE id = $1",
        claims.1,
        rot_expiration_date.clone(),
        new_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    info!(
        "Rotated API Key {} with Scope: {:?}",
        old_key_entry.keytag, old_key_entry.named_scope
    );
    Ok(AuthRotateResponse::Created {
        new_api_key: api_key.to_string(),
        rotation_complete_date: rot_expiration_date,
    })
}

/// Deletes a Key with a given keytag
/// # Errors
/// Can fail if the database operations fail
#[instrument(skip_all, fields(claim=%claims.0, keytag=%delete_tag.0))]
pub async fn auth_delete(
    server: &LTZFServer,
    claims: &Claims,
    delete_tag: &params::ApiKeyDelete,
) -> Result<()> {
    sqlx::query!(
        "UPDATE api_keys SET deleted_by=$1 WHERE keytag=$2",
        claims.1,
        delete_tag.0
    )
    .execute(&server.sqlx_db)
    .await?;

    info!("Successfully deleted keys");
    Ok(())
}

/// Lists keytags that exist within the database. A keyadder-only feature.
///
/// # Errors
/// Can fail if the database operations fail, or if there is a pagination error
#[instrument(skip_all, fields(claim=%claims.0))]
pub async fn auth_listing(
    server: &LTZFServer,
    claims: &Claims,
    query_params: &ListingQueryParams,
) -> Result<AuthListingGetResponse<KeyTag>> {
    let mut tx = server.sqlx_db.begin().await?;
    let total_count = sqlx::query_scalar!(
        "SELECT COUNT(1) FROM api_keys WHERE deleted_by IS NULL AND expires_at > NOW()"
    )
    .fetch_one(&mut *tx)
    .await?
    .unwrap_or_default();
    let prp = PaginationResponsePart::new(
        u32::try_from(total_count).map_err(|e| LTZFError::Other {
            message: format!("Error during integer conversion: {e}"),
        })?,
        query_params.page,
        query_params.per_page,
    );
    let body = sqlx::query!(
            "SELECT keytag FROM api_keys WHERE deleted_by IS NULL AND expires_at > NOW() ORDER BY expires_at DESC
            OFFSET $1
            LIMIT $2", prp.offset(), prp.limit())
        .map(|r| models::KeyTag::from(r.keytag))
        .fetch_all(&mut *tx).await?;

    tx.commit().await?;
    info!("Listing {} keys", &body.len());
    Ok(AuthListingGetResponse::Success {
        body,
        prp,
        path_prefix: "/api/v2/auth/keys",
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct KeytagListing {
    #[serde(rename = "dokumente")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dokumente: Option<Vec<String>>,

    #[serde(rename = "sitzungen")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sitzungen: Option<Vec<String>>,

    #[serde(rename = "vorgaenge")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vorgaenge: Option<Vec<String>>,

    #[serde(rename = "stationen")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stationen: Option<Vec<String>>,
}

/// Lists the Objects touched by the supplied keytag
/// # Errors
/// Can fail if the database operations fail
pub async fn auth_listing_keytag(
    server: &LTZFServer,
    path_params: &AuthListingKeytagPathParams,
) -> Result<KeytagListing> {
    let dings = sqlx::query!(
        "SELECT 1 AS disc, api_id FROM scraper_touched_vorgang tb
            INNER JOIN vorgang v ON v.id= tb.vg_id
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1
            
            UNION ALL
            
            SELECT 2, api_id FROM scraper_touched_station tb
            INNER JOIN station s ON s.id=tb.stat_id
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1

            UNION ALL

            SELECT 3, api_id FROM scraper_touched_sitzung tb
            INNER JOIN sitzung s ON s.id=tb.sid
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1

            UNION ALL

            SELECT 4, api_id FROM scraper_touched_dokument tb
            INNER JOIN dokument d ON d.id = tb.dok_id
			INNER JOIN api_keys ak ON ak.id = tb.collector_key
            WHERE ak.keytag = $1",
        path_params.keytag.clone()
    )
    .fetch_all(&server.sqlx_db)
    .await?;

    let vorgaenge = as_option(
        dings
            .iter()
            .filter_map(|record| match (record.disc, record.api_id) {
                (Some(1), Some(api_id)) => Some(api_id.to_string()),
                _ => None,
            })
            .collect(),
    );
    let stationen = as_option(
        dings
            .iter()
            .filter_map(|record| match (record.disc, record.api_id) {
                (Some(2), Some(api_id)) => Some(api_id.to_string()),
                _ => None,
            })
            .collect(),
    );
    let sitzungen = as_option(
        dings
            .iter()
            .filter_map(|record| match (record.disc, record.api_id) {
                (Some(3), Some(api_id)) => Some(api_id.to_string()),
                _ => None,
            })
            .collect(),
    );
    let dokumente = as_option(
        dings
            .iter()
            .filter_map(|record| match (record.disc, record.api_id) {
                (Some(4), Some(api_id)) => Some(api_id.to_string()),
                _ => None,
            })
            .collect(),
    );

    let result = KeytagListing {
        dokumente,
        sitzungen,
        vorgaenge,
        stationen,
    };
    let r = &result;
    info!(
        "Successfully fetched {}/{}/{}/{} (d/v/si/st) entries",
        r.dokumente.as_ref().map_or(0, Vec::len),
        r.vorgaenge.as_ref().map_or(0, Vec::len),
        r.sitzungen.as_ref().map_or(0, Vec::len),
        r.stationen.as_ref().map_or(0, Vec::len),
    );
    Ok(result)
}

#[cfg(test)]
mod auth_test {
    use super::{APIScope, Claims, KeytagListing};
    use super::{auth_delete, auth_listing, auth_listing_keytag, auth_rotate};
    use crate::api::auth::AuthRotateResponse;
    use crate::api::{AuthListingGetResponse, vorgang};
    use crate::db::retrieve::auth::auth_status;
    use crate::models::auth::{
        APIKey, APIKeyStatus, Argon2KeyGenerator, NoopKeyGenerator, Sha256KeyGenerator,
    };
    use crate::models::params::{self, AuthListingKeytagPathParams, ListingQueryParams};
    use crate::utils::testing::{TestSetup, generate};
    use sqlx::PgExecutor;

    async fn fetch_key_index<'e, E: PgExecutor<'e>>(keytag: &str, ex: E) -> i32 {
        fetch_key_row(keytag, ex).await.id
    }

    #[allow(unused, reason = "We want to model structs close to database model")]
    #[derive(sqlx::Type)]
    struct KeyRow {
        id: i32,
        created_by: i32,
        deleted_by: Option<i32>,
        key_hash: String,
        created_at: chrono::DateTime<chrono::Utc>,
        expires_at: chrono::DateTime<chrono::Utc>,
        last_used: Option<chrono::DateTime<chrono::Utc>>,
        scope: i32,
        algo: i32,
        rotated_for: Option<i32>,
        salt: Vec<u8>,
        keytag: String,
    }

    async fn fetch_key_row<'e, E: PgExecutor<'e>>(keytag: &str, ex: E) -> KeyRow {
        sqlx::query_as!(KeyRow, "SELECT * FROM api_keys WHERE keytag = $1", keytag)
            .fetch_one(ex)
            .await
            .expect("Failed to get key row.")
    }

    #[tokio::test]
    async fn test_extract_claims_crcfail() {
        let scenario = TestSetup::new("test_extract_claims_crc").await;
        let key = "invalid key";
        let result = Claims::extract(&scenario.server, params::XAPIKey(key.to_owned())).await;
        assert!(result.is_err());
        let key = "invalidbutofasufficientlengthtobecheckedforcyclicredundancyproblemsinthematerialandhopefullyfailthistestsincethisisitsresontobe.";
        let result = Claims::extract(&scenario.server, params::XAPIKey(key.to_owned())).await;
        assert!(result.is_err());
    }

    // GET /auth/status
    #[tokio::test]
    async fn test_auth_status() {
        let scenario = TestSetup::new("test_auth_status").await;
        let server = &scenario.server;
        let expiry_date = chrono::Utc::now() + chrono::Duration::days(2);
        let original_key: APIKey<Argon2KeyGenerator> = crate::db::auth::create_apikey(
            1,
            APIScope::KeyAdder,
            Some(expiry_date),
            None,
            &server.sqlx_db,
        )
        .await
        .expect("");

        let original_key_idx = fetch_key_index(original_key.keytag(), &server.sqlx_db).await;

        let resp = auth_status(original_key_idx, &server.sqlx_db).await;

        assert!(
            matches!(&resp, Ok(APIKeyStatus{scope, expires_at, is_being_rotated})
            if *expires_at - expiry_date < chrono::Duration::milliseconds(1) && scope == "keyadder" && !is_being_rotated),
            "Expected Successful response, got {resp:?}"
        );

        let fresh_key = auth_rotate(server, &Claims(APIScope::KeyAdder, original_key_idx))
            .await
            .expect("Failed to rotate key.");
        let AuthRotateResponse::Created { new_api_key, .. } = fresh_key else {
            panic!()
        };

        let new_api_key: APIKey<NoopKeyGenerator> = APIKey::try_from(new_api_key).expect("");
        let fresh_key_index = fetch_key_index(new_api_key.keytag(), &server.sqlx_db).await;
        let fresh_key_status = auth_status(fresh_key_index, &server.sqlx_db).await.unwrap();
        assert!(!fresh_key_status.is_being_rotated && fresh_key_status.scope == "keyadder");

        let key_status_rot = auth_status(original_key_idx, &server.sqlx_db).await;
        assert!(
            matches!(&key_status_rot,
                Ok(APIKeyStatus{scope, is_being_rotated, expires_at}) if (*expires_at - chrono::Utc::now()) - chrono::Duration::days(1)
                < chrono::Duration::seconds(1) && scope == "keyadder" && *is_being_rotated
            ),
            "Expected Successful response, got {key_status_rot:?}"
        );

        scenario.teardown().await;
    }

    // GET /auth/keys
    #[tokio::test]
    async fn test_list_keys() {
        let scenario = TestSetup::new("test_list_keys").await;
        let server = &scenario.server;
        let mut keys = vec![];

        let kkey1: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::KeyAdder, None, None, &server.sqlx_db)
                .await
                .expect("");
        let ckey: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::Collector, None, None, &server.sqlx_db)
                .await
                .expect("");
        let kkey2: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::KeyAdder, None, None, &server.sqlx_db)
                .await
                .expect("");

        let akey: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::Admin, None, None, &server.sqlx_db)
                .await
                .expect("");

        keys.push(kkey1);
        keys.push(ckey);
        keys.push(kkey2);
        keys.push(akey);
        // ------------------------------------------------------------------------------------------------------------
        let response = auth_listing(
            server,
            &Claims(APIScope::KeyAdder, 1),
            &ListingQueryParams {
                page: None,
                per_page: None,
                since: None,
                until: None,
            },
        )
        .await;
        match response {
            Ok(AuthListingGetResponse::Success { body, .. }) => {
                let mut bodykeys: Vec<_> = body
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .strip_prefix("\"")
                            .unwrap_or(x.as_str())
                            .strip_suffix("\"")
                            .unwrap_or(x.as_str())
                    })
                    .collect();
                bodykeys.sort_unstable();
                let mut keys = keys
                    .iter()
                    .map(|x| x.keytag().to_owned())
                    .chain(vec!["total-nutzlos".to_string()].drain(..))
                    .collect::<Vec<_>>();
                keys.sort();
                assert_eq!(keys, bodykeys);
            }
            _ => unreachable!("unreachable"),
        }

        scenario.teardown().await;
    }

    // GET /auth/keys/{key_tag}
    #[tokio::test]
    async fn test_administer_key_details() {
        let scenario = TestSetup::new("test_administer_key_details").await;
        let server = &scenario.server;
        let key: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::KeyAdder, None, None, &server.sqlx_db)
                .await
                .expect("");

        let key_idx = fetch_key_index(key.keytag(), &server.sqlx_db).await;

        let _ = vorgang::vorgang_put(
            server,
            key_idx,
            &params::XScraperId(uuid::Uuid::nil()),
            &generate::default_vorgang(),
        )
        .await
        .expect("Failed to add a new Vorgang.");
        // enough setup, here it comes:
        let rsp = auth_listing_keytag(
            server,
            &AuthListingKeytagPathParams {
                keytag: key.keytag().to_owned(),
            },
        )
        .await;

        match rsp {
            Ok(KeytagListing {
                dokumente,
                sitzungen,
                stationen,
                vorgaenge,
            }) => {
                assert_eq!(dokumente.map(|d| d.len()), Some(2));
                assert_eq!(sitzungen, None);
                assert_eq!(stationen.map(|s| s.len()), Some(1));
                assert_eq!(vorgaenge.map(|v| v.len()), Some(1));
            }
            _ => unreachable!(),
        }
        scenario.teardown().await;
    }

    // DELETE /auth
    #[tokio::test]
    async fn test_revoke_key() {
        let scenario = TestSetup::new("test_revoke_key").await;
        let server = &scenario.server;
        let key: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::KeyAdder, None, None, &server.sqlx_db)
                .await
                .expect("");
        let tag = key.keytag();
        let row = fetch_key_row(tag, &server.sqlx_db).await;
        assert_eq!(row.deleted_by, None);

        // delete successfully
        auth_delete(
            server,
            &Claims(APIScope::KeyAdder, 1),
            &params::ApiKeyDelete(tag.to_owned()),
        )
        .await
        .expect("Failed to delete key.");
        let row = fetch_key_row(tag, &server.sqlx_db).await;
        assert_eq!(row.deleted_by, Some(1));

        // delete already deleted key
        auth_delete(
            server,
            &Claims(APIScope::KeyAdder, 1),
            &params::ApiKeyDelete(tag.to_owned()),
        )
        .await
        .expect("Failed to delete key.");

        let row = fetch_key_row(tag, &server.sqlx_db).await;
        assert_eq!(row.deleted_by, Some(1));

        // delete without permission
        auth_delete(
            server,
            &Claims(APIScope::Admin, 1),
            &params::ApiKeyDelete(tag.to_owned()),
        )
        .await
        .expect("Failed to delete key.");

        scenario.teardown().await;
    }
    #[tokio::test]
    async fn sha256_and_argon2_side_by_side_auth() {
        let scenario = TestSetup::new("test_sha256_and_argon2_side_by_side_auth").await;
        let server = &scenario.server;
        let mut tx = server.sqlx_db.begin().await.expect("sdkf");
        let key1: APIKey<Sha256KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::KeyAdder, None, None, &mut *tx)
                .await
                .expect("failed key cr");
        let key2: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::Collector, None, None, &mut *tx)
                .await
                .expect("failed key cr");
        tx.commit().await.expect("commit failed");

        // authenticate
        let claims = Claims::extract(server, params::XAPIKey(key1.to_string())).await;
        assert!(
            matches!(claims, Ok(Claims(APIScope::KeyAdder, 2))),
            "{claims:?}"
        );
        let claims = Claims::extract(server, params::XAPIKey(key2.to_string())).await;
        assert!(
            matches!(claims, Ok(Claims(APIScope::Collector, 3))),
            "{claims:?}"
        );

        scenario.teardown().await;
    }

    // POST /auth/rotate
    #[tokio::test]
    async fn test_rotate_own_key() {
        let scenario = TestSetup::new("test_rotate_own_key").await;
        let server = &scenario.server;
        let key0: APIKey<Argon2KeyGenerator> =
            crate::db::auth::create_apikey(1, APIScope::KeyAdder, None, None, &server.sqlx_db)
                .await
                .expect("");
        let key0_idx = fetch_key_index(key0.keytag(), &server.sqlx_db).await;

        // first test: status of the key is not in rotation
        // then rotate: The key is in rotation at index 2
        let original_key_status = auth_status(2, &server.sqlx_db).await.unwrap();
        assert!(!original_key_status.is_being_rotated);
        assert_eq!(original_key_status.scope, "keyadder");
        let key1 = auth_rotate(server, &Claims(APIScope::KeyAdder, 2))
            .await
            .expect("Failed to rotate key.");
        let AuthRotateResponse::Created { new_api_key, .. } = key1 else {
            panic!();
        };
        let key1: APIKey<NoopKeyGenerator> = APIKey::try_from(new_api_key).unwrap();
        // state of the db:
        // 1. superadmin key
        // 2. key in rotation (1d)
        // 3. freshly generated key (1y)

        let key1_idx = fetch_key_index(key1.keytag(), &server.sqlx_db).await;

        // testing that the time limits work and a "standalone" rotation is possible
        let key0_status = auth_status(key0_idx, &server.sqlx_db).await.unwrap(); // old key
        assert!(
            key0_status.is_being_rotated
                && key0_status.scope == "keyadder"
                && key0_status
                    .expires_at
                    .checked_sub_days(chrono::Days::new(1))
                    .expect("Failed to sub day from expiry datetime.")
                    <= chrono::Utc::now()
        );
        let key1_status = auth_status(key1_idx, &server.sqlx_db).await.unwrap(); // new key
        assert!(
            !key1_status.is_being_rotated
                && key1_status.scope == "keyadder"
                && key1_status
                    .expires_at
                    .checked_sub_months(chrono::Months::new(11))
                    .expect("Failed to sub 11 months from expiry datetime.")
                    >= chrono::Utc::now()
        );

        // rotate the new key and expect the key in rotation to be invalidated
        let key2 = auth_rotate(server, &Claims(APIScope::KeyAdder, 3))
            .await
            .expect("Failed to rotate key.");
        let AuthRotateResponse::Created { new_api_key, .. } = key2 else {
            panic!()
        };
        let key2: APIKey<NoopKeyGenerator> = APIKey::try_from(new_api_key).expect("");
        // state of the db:
        // 1. superadmin key
        // key0: ~~key in rotation (1d)~~ -> invalidated key
        // key1: ~~freshly generated key (1y)~~ -> key in rotation (1d)
        // key2: freshly generated key (1y)
        let key2_idx = fetch_key_index(key2.keytag(), &server.sqlx_db).await;
        let key2_status = auth_status(key2_idx, &server.sqlx_db).await.unwrap(); // new key
        assert!(!key2_status.is_being_rotated);
        let key0_row = fetch_key_row(key0.keytag(), &server.sqlx_db).await;
        assert_eq!(key0_row.deleted_by, Some(key0_idx));
        let key1_status = auth_status(key1_idx, &server.sqlx_db).await.unwrap();
        assert!(key1_status.is_being_rotated);

        // rotate the key in rotation again and expect the previously generated fresh key to be invalidated.
        // the old key's lifetime does not change from the first rotation (otherwise it could be extended -> secu risk)
        let key3 = auth_rotate(server, &Claims(APIScope::KeyAdder, 3))
            .await
            .expect("Failed to rotate key.");
        let AuthRotateResponse::Created { new_api_key, .. } = key3 else {
            panic!()
        };
        let key3: APIKey<NoopKeyGenerator> = APIKey::try_from(new_api_key).expect("");
        // state of the db:
        // 1. superadmin key
        // key0: ~~key in rotation (1d)~~ -> invalidated
        // key1: ~~freshly generated key (1y)~~ -> key in rotation (original timestamp)
        // key2: ~~freshly generated key (1y)~~ -> invalidated
        // key3: freshly generated key (1y)

        let key3_idx = fetch_key_index(key3.keytag(), &server.sqlx_db).await;
        let key3_status = auth_status(key3_idx, &server.sqlx_db).await.unwrap();
        let key1_status_new = auth_status(key1_idx, &server.sqlx_db).await.unwrap();
        let key2_row = fetch_key_row(key2.keytag(), &server.sqlx_db).await;

        assert_eq!(key1_status_new.expires_at, key1_status.expires_at);
        assert!(!key3_status.is_being_rotated);
        assert_eq!(key2_row.deleted_by, Some(key2_idx));

        scenario.teardown().await;
    }
}
