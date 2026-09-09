use crate::Result;
use sqlx::PgExecutor;
use uuid::Uuid;

pub(crate) async fn delete_vorgang_by_api_id<'e, E>(api_id: Uuid, executor: E) -> Result<()>
where
    E: PgExecutor<'e>,
{
    sqlx::query!("DELETE FROM vorgang WHERE api_id = $1", api_id)
        .execute(executor)
        .await?;

    Ok(())
}

pub(crate) async fn delete_sitzung_by_api_id<'e, E>(api_id: Uuid, executor: E) -> Result<()>
where
    E: PgExecutor<'e>,
{
    sqlx::query!("DELETE FROM sitzung WHERE api_id = $1", api_id)
        .execute(executor)
        .await?;
    Ok(())
}
