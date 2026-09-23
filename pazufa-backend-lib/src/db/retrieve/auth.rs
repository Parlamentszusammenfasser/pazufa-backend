use crate::{Result, models::auth::APIKeyStatus};
use sqlx::PgExecutor;
use tracing::{debug, instrument};

/// Retrieves Information about one's own key.
/// For example, how long it is still valid, what the scope is, and if it is authenticated as
/// intended.
/// Can also be used as testing endpoint to check if the key you supplied works
/// # Errors
/// Can fail if the database operations fail
#[instrument(skip_all, fields(key_idx=%key_idx))]
pub async fn auth_status<'e, E: PgExecutor<'e>>(key_idx: i32, ex: E) -> Result<APIKeyStatus> {
    let db_row = sqlx::query!(
        "SELECT expires_at, rotated_for, keytag, s.value as scope
        FROM api_keys a
        INNER JOIN api_scope s ON s.id = a.scope
        WHERE a.id = $1",
        key_idx
    )
    .fetch_one(ex)
    .await?;

    let is_being_rotated = db_row.rotated_for.is_some() && db_row.expires_at > chrono::Utc::now();
    debug!("Key {} status requested", db_row.keytag);
    Ok(APIKeyStatus {
        expires_at: db_row.expires_at,
        scope: db_row.scope,
        is_being_rotated,
    })
}
