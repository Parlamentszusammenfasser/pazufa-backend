use crate::Result;
use sqlx::PgExecutor;
use tracing::{debug, instrument};
use uuid::Uuid;

/// Deletes a Key with a given keytag
/// # Errors
/// Can fail if the database operations fail
#[instrument(skip_all, fields(deleted_by=%deleted_by, keytag=%delete_tag))]
pub async fn delete_apikey_by_keytag<'e, E: PgExecutor<'e>>(
    deleted_by: i32,
    delete_tag: &str,
    ex: E,
) -> Result<()> {
    sqlx::query!(
        "UPDATE api_keys SET deleted_by=$1 WHERE keytag=$2",
        deleted_by,
        delete_tag
    )
    .execute(ex)
    .await?;

    debug!("Successfully deleted keys");
    Ok(())
}

/// removes the specified vorgang from the database
/// # Errors
/// Can fail due to database errors
pub async fn delete_vorgang_by_api_id<'e, E: PgExecutor<'e>>(
    api_id: Uuid,
    executor: E,
) -> Result<()> {
    sqlx::query!("DELETE FROM vorgang WHERE api_id = $1", api_id)
        .execute(executor)
        .await?;

    Ok(())
}

/// Bulk-Deletes the passed IDs from the Database
/// # Errors
/// Can fail due to database errors
pub async fn bulk_delete_vorgang<'e, E: PgExecutor<'e>>(
    database_ids: &[i32],
    executor: E,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM vorgang WHERE id = ANY($1::int4[])",
        &database_ids[..]
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// removes the specified sitzungfrom the database
/// # Errors
/// Can fail due to database errors
pub async fn delete_sitzung_by_api_id<'e, E: PgExecutor<'e>>(
    api_id: Uuid,
    executor: E,
) -> Result<()> {
    sqlx::query!("DELETE FROM sitzung WHERE api_id = $1", api_id)
        .execute(executor)
        .await?;
    Ok(())
}
