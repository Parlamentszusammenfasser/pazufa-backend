use crate::Result;
use crate::models::{APIScope, auth::APIKeyMetadata};
use sqlx::PgExecutor;

/// Retrieves a full Metadata row for any API Key.
/// # Errors
/// Can fail if the database operations fail
pub async fn get_api_key_metadata<'e, E: PgExecutor<'e>>(
    key_idx: i32,
    ex: E,
) -> Result<APIKeyMetadata> {
    Ok(sqlx::query_as!(
        APIKeyMetadata,
        "SELECT 
        value as \"scope!: APIScope\", 
        expires_at, created_at, rotated_for, keytag
            FROM api_keys INNER JOIN api_scope ON scope=api_scope.id 
            WHERE api_keys.id = $1",
        key_idx
    )
    .fetch_one(ex)
    .await?)
}
