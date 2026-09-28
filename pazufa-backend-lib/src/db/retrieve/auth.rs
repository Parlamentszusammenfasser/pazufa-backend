use crate::api::PaginationResponsePart;
use crate::models::{APIScope, auth::APIKeyMetadata};
use crate::{Result, models};
use sqlx::{PgExecutor, PgTransaction};

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

/// Lists keytags that exist within the database. A keyadder-only feature.
///
/// # Errors
/// Can fail if the database operations fail, or if there is a pagination error
pub async fn get_paginated_keytags(
    page: Option<u32>,
    per_page: Option<u32>,
    tx: &mut PgTransaction<'_>,
) -> Result<(PaginationResponsePart, Vec<models::KeyTag>)> {
    let total_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(1) FROM api_keys WHERE deleted_by IS NULL AND expires_at > NOW()"
    )
    .fetch_one(&mut **tx)
    .await?
    .unwrap_or_default();

    let Ok(total_count) = u32::try_from(total_count) else {
        unreachable!("sqls COUNT returned something < 0");
    };

    let prp = PaginationResponsePart::new(total_count, page, per_page);
    let body = sqlx::query!(
            "SELECT keytag FROM api_keys WHERE deleted_by IS NULL AND expires_at > NOW() ORDER BY expires_at DESC
            OFFSET $1
            LIMIT $2", prp.offset(), prp.limit())
        .map(|r| models::KeyTag::from(r.keytag))
        .fetch_all(&mut **tx).await?;

    Ok((prp, body))
}
