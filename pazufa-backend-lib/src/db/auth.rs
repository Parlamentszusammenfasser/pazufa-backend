use crate::Result;
use pazufa_models::auth::{APIKey, APIKeyGenerator, generate_keytag, generate_salt};
use pazufa_models::enumerations::APIScope;
use sqlx::PgExecutor;

/// Creates an API Key with the specified parameters in the database
/// # Errors
/// Returns Error if the database operations fail
pub async fn create_apikey<'e, KeyGenerator: APIKeyGenerator, E: PgExecutor<'e>>(
    creator_key: i32,
    with_scope: APIScope,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    keytag_prefix: Option<&str>,
    ex: E,
) -> Result<APIKey<KeyGenerator>> {
    let salt = generate_salt();
    let mut vec_data = vec![0; salt.len()];
    let salt_bytes: Vec<_> = salt.decode_b64(&mut vec_data)?.into();
    let keytag = generate_keytag(keytag_prefix);

    let key = APIKey::<KeyGenerator>::new(&keytag);
    let hash = key.hash(salt.as_salt())?;

    sqlx::query!(
        "INSERT INTO api_keys(key_hash, created_by, expires_at, scope, salt, keytag, algo)
            VALUES
            ($1, $2, $3, (SELECT id FROM api_scope WHERE value = $4), 
             $5, $6, (SELECT id FROM hash_algorithm WHERE value = $7))",
        hash,
        creator_key,
        expires_at.unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::days(365)),
        with_scope.to_string(),
        &salt_bytes,
        key.keytag(),
        key.hash_algorithm().to_string()
    )
    .execute(ex)
    .await?;

    tracing::info!(
        "Generated Fresh API Key with Scope: {with_scope} and keytag {}",
        key.keytag()
    );
    Ok(key)
}
