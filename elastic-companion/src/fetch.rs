use crate::{Config, es::EsClient};
use anyhow::Result;
use pazufa_backend_lib::db::retrieve;
use sqlx::{PgPool, PgTransaction};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

type TSType = chrono::DateTime<chrono::Utc>;
const PER_PAGE_MAX: u32 = 10;
const CHANNEL_BUFFER_SIZE: usize = 100;

pub async fn fetch_loop<BackendCall, Object: Sized + Sync + Send + 'static>(
    db_pool: Arc<PgPool>,
    tx: Sender<Vec<Object>>,
    mut closure: BackendCall,
) -> Result<()>
where
    BackendCall: AsyncFnMut(u32, &mut PgTransaction<'_>) -> Result<Vec<Object>>,
{
    let mut ta = db_pool.begin().await?;
    for i in 1.. {
        let objects: Vec<Object> = closure(i, &mut ta).await?;
        if objects.is_empty() {
            break;
        }
        tx.send(objects).await?;
    }
    ta.commit().await?;
    Ok(())
}

pub async fn sync_vorgang(
    config: &Config,
    last_mod: TSType,
    es_client: Arc<EsClient>,
    db_pool: Arc<PgPool>,
) -> Result<()> {
    if let Some(ref index_name) = config.index_vorgang {
        tracing::info!("Indexing Vorgang Objects");
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        // vorgang fetch join handle
        let vfetchjh = tokio::spawn(fetch_loop(db_pool, tx, async move |page: u32, ta| {
            let (_, vector) = retrieve::vorgang_by_parameter(
                retrieve::VGGetParameters {
                    if_mod_since: Some(last_mod),
                    ..Default::default()
                },
                Some(page),
                Some(PER_PAGE_MAX),
                true,
                ta,
            )
            .await?;
            Ok(vector)
        }));
        // vorgang index join handle
        let vindexjh = tokio::spawn(crate::es::receive_batches(
            index_name.clone(),
            rx,
            Arc::clone(&es_client),
        ));
        tokio::join!(vfetchjh, vindexjh).1??;
        tracing::info!("Indexing finished");
    } else {
        tracing::warn!("Vorgang was not configured for fetching, skipping...");
    }

    Ok(())
}
pub async fn sync_sitzung(
    config: &Config,
    last_mod: TSType,
    es_client: Arc<EsClient>,
    db_pool: Arc<PgPool>,
) -> Result<()> {
    if let Some(ref index_name) = config.index_vorgang {
        tracing::info!("Indexing Vorgang Objects");
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        // vorgang fetch join handle
        let vfetchjh = tokio::spawn(fetch_loop(db_pool, tx, async move |page: u32, ta| {
            let (_, vector) = retrieve::sitzung_by_parameter(
                &retrieve::SitzungFilterParameters {
                    since: Some(last_mod),
                    ..Default::default()
                },
                Some(page),
                Some(PER_PAGE_MAX),
                true,
                ta,
            )
            .await?;
            Ok(vector)
        }));
        // vorgang index join handle
        let vindexjh = tokio::spawn(crate::es::receive_batches(
            index_name.clone(),
            rx,
            Arc::clone(&es_client),
        ));
        tokio::join!(vfetchjh, vindexjh).1??;
        tracing::info!("Indexing finished");
    } else {
        tracing::warn!("Sitzung was not configured for fetching, skipping...");
    }

    Ok(())
}
pub async fn sync_dokument(
    config: &Config,
    last_mod: TSType,
    es_client: Arc<EsClient>,
    db_pool: Arc<PgPool>,
) -> Result<()> {
    if let Some(ref index_name) = config.index_vorgang {
        tracing::info!("Indexing Dokument Objects");
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        // vorgang fetch join handle
        let vfetchjh = tokio::spawn(fetch_loop(db_pool, tx, async move |page: u32, ta| {
            let (_, vector) = retrieve::get_dokument_by_parameter(
                retrieve::DokumentRetrivalParameters {
                    if_mod_since: Some(last_mod),
                    page: Some(page),
                    per_page: Some(PER_PAGE_MAX),
                    ..Default::default()
                },
                ta,
            )
            .await?;
            Ok(vector)
        }));
        // vorgang index join handle
        let vindexjh = tokio::spawn(crate::es::receive_batches(
            index_name.clone(),
            rx,
            Arc::clone(&es_client),
        ));
        tokio::join!(vfetchjh, vindexjh).1??;
        tracing::info!("Indexing finished");
    } else {
        tracing::warn!("Dokument was not configured for fetching, skipping...");
    }

    Ok(())
}
