#![forbid(unsafe_code)]
use anyhow::{Result, bail};
use clap::{Parser, value_parser};
use es::FetchId;
use serde::Serialize;
use std::{path::PathBuf, str::FromStr, sync::Arc};
use tokio::sync::mpsc::Receiver;

mod es;
mod sync;

const DEFAULT_TIMESTAMP_PATH: &str = "./.last_es_sync";
const CHANNEL_BUFFER_SIZE: usize = 100;
const MAX_HALVED: i64 = (u32::MAX / 2) as i64;

#[allow(
    clippy::struct_excessive_bools,
    reason = "Is a config struct, all combinations are valuable"
)]
#[derive(Parser, Debug)]
#[command(author, version, about = "Syncs pazufa-backend data to Elasticsearch")]
pub struct Config {
    #[arg(long, env = "DATABASE_URL")]
    pub db_url: String,

    #[arg(
        long,
        env = "ELASTICSEARCH_URL",
        default_value = "http://localhost:9200"
    )]
    pub es_url: String,

    #[arg(long, env = "ELASTICSEARCH_APIKEY")]
    pub es_apikey: Option<String>,
    #[arg(long, env = "ELASTICSEARCH_USER")]
    pub es_user: Option<String>,
    #[arg(long, env = "ELASTICSEARCH_PASS")]
    pub es_pass: Option<String>,

    #[arg(
        long("iv"),
        default_value = "false",
        help = "Index Vorgang Objects into index `vorgang`"
    )]
    pub index_vorgang: bool,

    #[arg(
        long("is"),
        default_value = "false",
        help = "Index Sitzung Objects into index `sitzung`"
    )]
    pub index_sitzungen: bool,

    #[arg(long, help = "Reindex as-if run last at the supplied timestamp")]
    pub reindex_since: Option<chrono::DateTime<chrono::Utc>>,

    #[arg(
        long,
        help = "Path to the file in which to store the timestamp of last sync"
    )]
    pub timestamp_store: Option<std::path::PathBuf>,

    #[arg(
        long("id"),
        default_value = "false",
        help = "Index Document Objects into index `dokument`"
    )]
    pub index_documents: bool,

    #[arg(
        long,
        value_parser = value_parser!(u32).range(0i64..=MAX_HALVED),
        env = "REINDEX_SECONDS",
        help = "Time between syncs to ES in seconds. If a sync takes longer than that, the next reindexing is skipped. If omitted runs once and exits"
    )]
    pub reindex_seconds: Option<u32>,
}

fn store_last_synced(
    path: Option<&std::path::PathBuf>,
    last_run_start: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let default = std::path::PathBuf::from_str(DEFAULT_TIMESTAMP_PATH)?;
    let path = path.unwrap_or(&default);
    let Some(parent) = path.parent() else {
        bail!("Path {} does not have a parent", path.display());
    };
    if parent.exists() {
        std::fs::write(path, last_run_start.to_rfc3339())?;
    } else {
        bail!("Directory {:?} does not exist", path.parent())
    }
    Ok(())
}

fn fetch_last_synced(
    path: Option<&std::path::PathBuf>,
    default: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<chrono::DateTime<chrono::Utc>> {
    let default_path = std::path::PathBuf::from_str(DEFAULT_TIMESTAMP_PATH)?;
    let path = path.unwrap_or(&default_path);
    if !path.exists() {
        tracing::info!("Last timestamp marker was not found");
        return Ok(default
            .unwrap_or(chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.to_utc()));
    }
    let contents = std::fs::read_to_string(path)?;
    let timestamp = chrono::DateTime::parse_from_rfc3339(&contents)?.to_utc();
    Ok(timestamp)
}
async fn handle_batches<T: Serialize + Sized + Sync + Send + FetchId>(
    index_name: &'static str,
    mut rx: Receiver<Vec<T>>,
    es_client: Arc<es::EsClient>,
) -> Result<()> {
    while let Some(batch) = rx.recv().await {
        es_client.batch_index(index_name, batch).await?;
    }
    Ok(())
}
async fn sync(config: &Config) -> Result<()> {
    let last_synced = fetch_last_synced(config.timestamp_store.as_ref(), config.reindex_since)?;
    let es_client = Arc::new(es::EsClient::new(&config.es_url, config).await?);
    let db_pool: Arc<sqlx::PgPool> = Arc::new(sqlx::PgPool::connect(&config.db_url).await?);

    tracing::info!(
        "Run Started. Syncing since last mod > {}",
        last_synced.to_rfc3339()
    );
    let run_start = chrono::Utc::now();
    if config.index_vorgang {
        tracing::info!("Indexing Vorgang Objects");
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        // vorgang fetch join handle
        let vfetchjh = tokio::spawn(sync::index_vorgang(last_synced, tx, Arc::clone(&db_pool)));
        // vorgang index join handle
        let vindexjh = tokio::spawn(handle_batches("vorgang", rx, Arc::clone(&es_client)));
        tokio::join!(vfetchjh, vindexjh).1??;
        tracing::info!("Indexing finished");
    }

    if config.index_sitzungen {
        tracing::info!("Indexing Sitzungen Objects");
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        // sitzung fetch join handle
        let sfetchjh = tokio::spawn(sync::index_sitzung(last_synced, tx, Arc::clone(&db_pool)));
        // sitzung index join handle
        let sindexjh = tokio::spawn(handle_batches("sitzung", rx, Arc::clone(&es_client)));
        tokio::join!(sfetchjh, sindexjh).1??;
        tracing::info!("Indexing finished");
    }
    if config.index_documents {
        tracing::info!("Indexing Dokument Objects");
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        // dokument fetch join handle
        let dfetchjh = tokio::spawn(sync::index_dokument(last_synced, tx, Arc::clone(&db_pool)));
        // dokument index join handle
        let dindexjh = tokio::spawn(handle_batches("dokument", rx, Arc::clone(&es_client)));
        tokio::join!(dfetchjh, dindexjh).1??;
        tracing::info!("Indexing finished");
    }
    store_last_synced(config.timestamp_store.as_ref(), run_start)?;
    Ok(())
}

#[allow(clippy::cast_sign_loss, reason = "is checked and floored")]
#[allow(clippy::cast_possible_truncation, reason = "is checked")]
#[allow(clippy::cast_precision_loss, reason = "is checked")]
fn f32_floor(x: f32) -> Result<u32> {
    if x < 0. {
        bail!("f32 < 0");
    }
    if x > u32::MAX as f32 {
        bail!("f32 > u32::MAX");
    }
    Ok(x.floor() as u32)
}

#[allow(
    clippy::arithmetic_side_effects,
    reason = "seconds should behave, and are checked"
)]
fn next_occurrence(
    start: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
    s_per_epoch: u32,
) -> Result<(u32, chrono::DateTime<chrono::Utc>)> {
    let diff = now - start;
    let diff_s = f32_floor(diff.as_seconds_f32())?;
    let epoch = diff_s.div_ceil(s_per_epoch);
    let next_occ = start + std::time::Duration::from_secs(u64::from((epoch + 1) * s_per_epoch));
    Ok((epoch, next_occ))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::parse();
    // if some timestamp is supplied, remove the last stored if exists
    // and force reindexing since supplied timestamp on first run
    if config.reindex_since.is_some() {
        if let Some(ref f) = config.timestamp_store
            && f.exists()
        {
            std::fs::remove_file(f)?;
        } else if PathBuf::from(DEFAULT_TIMESTAMP_PATH).exists() {
            std::fs::remove_file(PathBuf::from(DEFAULT_TIMESTAMP_PATH))?;
        }
    }
    let start = chrono::Utc::now();

    loop {
        sync(&config).await?;
        let now = chrono::Utc::now();
        if let Some(rs) = config.reindex_seconds {
            let (epoch, next_occ) = next_occurrence(start, now, rs)?;
            let sleep_time = next_occ - now;

            tracing::info!(
                "Run Done, epoch {epoch}, next occurrence: {}",
                next_occ.to_rfc3339()
            );
            tokio::time::sleep(sleep_time.to_std()?).await;
        } else {
            return Ok(());
        }
    }
}
