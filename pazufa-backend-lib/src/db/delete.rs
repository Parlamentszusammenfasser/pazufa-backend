use crate::{LTZFServer, Result};
use uuid::Uuid;

pub(crate) async fn delete_vorgang_by_api_id(api_id: Uuid, server: &LTZFServer) -> Result<()> {
    sqlx::query!("DELETE FROM vorgang WHERE api_id = $1", api_id)
        .execute(&server.sqlx_db)
        .await?;

    Ok(())
}
pub(crate) async fn delete_sitzung_by_api_id(api_id: Uuid, server: &LTZFServer) -> Result<()> {
    sqlx::query!("DELETE FROM sitzung WHERE api_id = $1", api_id)
        .execute(&server.sqlx_db)
        .await?;
    Ok(())
}
