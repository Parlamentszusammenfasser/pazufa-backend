use crate::Result;
use crate::models::auth::{
    APIKey, Argon2KeyGenerator, HashAlgorithm, Sha256KeyGenerator, generate_keytag, generate_salt,
};
use crate::models::enumerations::APIScope;

/// Creates an API Key with the specified parameters in the database
/// # Errors
/// Returns Error if the database operations fail
pub async fn create_apikey(
    creator_key: i32,
    with_scope: APIScope,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    keytag_prefix: Option<&str>,
    algorithm: HashAlgorithm,
    tx: &mut sqlx::PgTransaction<'_>,
) -> Result<String> {
    let salt = generate_salt();
    let mut vec_data = vec![0; salt.len()];
    let salt_bytes: Vec<_> = salt.decode_b64(&mut vec_data)?.into();
    let keytag = generate_keytag(keytag_prefix);

    let (keytag, hash, key) = match algorithm {
        HashAlgorithm::Sha256 => {
            let k = APIKey::<Sha256KeyGenerator>::new(&keytag);
            let h = k.hash(salt.as_salt())?;
            (k.keytag().to_owned(), h, k.to_string())
        }
        HashAlgorithm::Argon2id => {
            let k = APIKey::<Argon2KeyGenerator>::new(&keytag);
            let h = k.hash(salt.as_salt())?;
            (k.keytag().to_owned(), h, k.to_string())
        }
    };

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
        keytag,
        algorithm.to_string()
    )
    .execute(&mut **tx)
    .await?;

    tracing::info!("Generated Fresh API Key with Scope: {with_scope} and keytag {keytag}",);
    Ok(key)
}
