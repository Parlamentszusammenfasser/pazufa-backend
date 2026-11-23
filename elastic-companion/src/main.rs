#![forbid(unsafe_code)]
use anyhow::{Result, bail};
use clap::{Parser, value_parser};
use fetch::{sync_dokument, sync_sitzung, sync_vorgang};
use std::sync::Arc;

mod diffs;
mod es;
mod fetch;

/// Directory of run record for our indices
const LAST_RUN_RECORD_PATH: &str = ".last_run";
const MAX_HALVED: i64 = (u32::MAX / 2) as i64;

#[derive(Parser, Debug)]
#[command(author, version, about = "Syncs pazufa-backend Data to Elasticsearch")]
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

    #[arg(long("iv"), help = "Index Vorgang Objects into the named index")]
    pub index_vorgang: Option<String>,

    #[arg(long("is"), help = "Index Sitzungen Objects into the named index")]
    pub index_sitzungen: Option<String>,

    #[arg(long("id"), help = "Index Dokument Objects into the named index")]
    pub index_documents: Option<String>,

    #[arg(long, help = "Reindex as-if run last at the supplied timestamp")]
    pub reindex_since: Option<chrono::DateTime<chrono::Utc>>,

    #[arg(
        long,
        help = "Path to the file in which to store the timestamp of last sync"
    )]
    pub timestamp_store: Option<std::path::PathBuf>,

    #[arg(
        long,
        value_parser = value_parser!(u32).range(0i64..=MAX_HALVED),
        env = "REINDEX_SECONDS",
        help = "Time between syncs to ES in seconds. If a sync takes longer than that, the next reindexing is skipped. If omitted runs once and exits"
    )]
    pub reindex_seconds: Option<u32>,
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

    let config = Box::new(Config::parse());
    let config = Box::leak(config);
    let mut last_mod = config
        .reindex_since
        .unwrap_or(chrono::DateTime::parse_from_rfc3339("1940-01-01T00:00:00Z")?.to_utc());
    let es_client = Arc::new(es::EsClient::new(&config.es_url, config).await?);
    let db_pool: Arc<sqlx::PgPool> = Arc::new(sqlx::PgPool::connect(&config.db_url).await?);
    let start = chrono::Utc::now();
    loop {
        tracing::info!(
            "Run Started. Syncing since last mod > {}",
            last_mod.to_rfc3339()
        );
        tokio::join!(
            tokio::spawn(sync_vorgang(
                config,
                last_mod,
                Arc::clone(&es_client),
                Arc::clone(&db_pool)
            )),
            tokio::spawn(sync_sitzung(
                config,
                last_mod,
                Arc::clone(&es_client),
                Arc::clone(&db_pool)
            )),
            tokio::spawn(sync_dokument(
                config,
                last_mod,
                Arc::clone(&es_client),
                Arc::clone(&db_pool)
            )),
        )
        .1??;
        let now = chrono::Utc::now();
        last_mod = now;
        if let Some(rs) = config.reindex_seconds {
            let (epoch, next_occ) = next_occurrence(start, now, rs)?;
            let sleep_time = next_occ - now;

            tracing::info!(
                "Run Done, epoch {epoch}, next occurrence: {}",
                next_occ.to_rfc3339()
            );
            tokio::time::sleep(sleep_time.to_std()?).await;
        } else {
            tracing::info!("Single run finished, exiting");
            return Ok(());
        }
    }
}
