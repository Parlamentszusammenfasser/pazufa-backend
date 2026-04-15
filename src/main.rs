#![cfg_attr(test, deny(unsafe_code))]
#![cfg_attr(not(test), forbid(unsafe_code))]

pub(crate) mod api;
pub(crate) mod db;
pub(crate) mod error;
pub(crate) mod utils;

use std::sync::Arc;

use axum::{extract::DefaultBodyLimit, http::Method, routing::get};
use clap::Parser;

use error::LTZFError;
use lettre::{SmtpTransport, transport::smtp::authentication::Credentials};
use tokio::net::TcpListener;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use tower_http::{compression::CompressionLayer, cors, limit};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub use api::{LTZFArc, LTZFServer};
pub use error::Result;
use std::process::exit;
use utils::shutdown_signal;
use utils::tracing::Logging;

pub type DateTime = chrono::DateTime<chrono::Utc>;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Parser, Clone, Debug, Default)]
#[command(author, version, about)]
pub struct Configuration {
    #[arg(long, env = "MAIL_SERVER")]
    pub mail_server: Option<String>,
    #[arg(long, env = "MAIL_USER")]
    pub mail_user: Option<String>,
    #[arg(long, env = "MAIL_PASSWORD")]
    pub mail_password: Option<String>,
    #[arg(long, env = "MAIL_SENDER")]
    pub mail_sender: Option<String>,
    #[arg(long, env = "MAIL_RECIPIENT")]
    pub mail_recipient: Option<String>,
    #[arg(long, env = "LTZF_HOST", default_value = "0.0.0.0")]
    pub host: String,
    #[arg(long, env = "LTZF_PORT", default_value = "80")]
    pub port: u16,
    #[arg(long, short, env = "DATABASE_URL", help = "URL to the database")]
    pub db_url: String,

    #[arg(
        long,
        env = "LTZF_KEYADDER_KEY",
        help = "The API Key that is used to add new Keys. This is saved in the database."
    )]
    pub keyadder_key: String,

    #[arg(long, env = "MERGE_TITLE_SIMILARITY", default_value = "0.8")]
    pub merge_title_similarity: f32,
    #[arg(
        long,
        env = "REQUEST_LIMIT_COUNT",
        help = "global request count that is per interval",
        default_value = "4096"
    )]
    pub req_limit_count: u32,
    #[arg(
        long,
        env = "REQUEST_LIMIT_INTERVAL",
        help = "(whole) number of seconds",
        default_value = "2"
    )]
    pub req_limit_interval: u32,
    #[arg(
        long,
        env = "PER_OBJECT_SCRAPER_LOG_SIZE",
        help = "Size of the queue keeping track of which scraper touched an object",
        default_value = "5"
    )]
    pub per_object_scraper_log_size: u32,

    #[arg(
        long,
        env = "PAZUFA_TRACKFILE",
        default_value = "tracks.toml",
        help = "The Trackfile describing which tracks are applicable. Usually hosted next to the tracks."
    )]
    track_path: String,
    #[arg(
        long,
        env = "LTZF_ERROR_LOG",
        default_value = "/var/log/pazufa-backend/error.log"
    )]
    pub error_log_path: String,

    #[arg(
        long,
        env = "LTZF_OBJECT_LOG",
        help = "If you want to log object lifecycle operations(create/delete/merge/...), enter a path"
    )]
    pub object_log_path: Option<String>,

    #[arg(
        long,
        help = "If you want to check the config before executing the server, run this and 
        the server will print it's configuration considering all inputs and then exit."
    )]
    pub dump_config: bool,
}

impl Configuration {
    pub fn build_mailer(&self) -> Result<SmtpTransport> {
        if let (
            Some(mail_server),
            Some(mail_user),
            Some(mail_password),
            Some(_mail_sender),
            Some(_mail_recipient),
        ) = (
            &self.mail_server,
            &self.mail_user,
            &self.mail_password,
            &self.mail_sender,
            &self.mail_recipient,
        ) {
            let mailer = SmtpTransport::relay(mail_server)?
                .credentials(Credentials::new(mail_user.clone(), mail_password.clone()))
                .build();
            Ok(mailer)
        } else {
            Err(LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message: "Mail Configuration is incomplete".into(),
                    config: Box::new(self.clone()),
                }),
            })
        }
    }
}
async fn handle_root() -> impl axum::response::IntoResponse {
    let s = include_str!("index.html");
    (axum::http::StatusCode::OK, axum::response::Html(s))
}

async fn swagger_ui() -> impl axum::response::IntoResponse {
    let s = include_str!("swagger-ui.html");
    (axum::http::StatusCode::OK, axum::response::Html(s))
}

async fn init_db_conn(db_url: &str) -> Result<sqlx::PgPool> {
    let sqlx_db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await?;

    let mut available = false;
    for i in 0..14 {
        let r = sqlx_db.acquire().await;
        match r {
            Ok(_) => {
                available = true;
                break;
            }
            Err(sqlx::Error::PoolTimedOut) => {
                tracing::warn!("Connection to Database `{}` timed out", db_url);
            }
            _ => {
                let _ = r?;
            }
        }
        let milliseconds = 2i32.pow(i) as u64;
        tracing::info!("DB Unavailable, Retrying in {} ms...", milliseconds);
        tokio::time::sleep(std::time::Duration::from_millis(milliseconds)).await;
    }
    if !available {
        return Err(LTZFError::Other {
            message: "Server Connection failed after 10 retries".to_owned(),
        });
    }
    tracing::debug!("Started Database Pool");
    sqlx::migrate!().run(&sqlx_db).await?;
    tracing::debug!("Executed Migrations");
    Ok(sqlx_db)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let config = Configuration::parse();
    if config.dump_config {
        #[allow(clippy::disallowed_macros, reason = "Explicitly requested by user.")]
        {
            println!("{config:#?}");
        }
        exit(0);
    }
    let logging = Logging::new(
        config.error_log_path.clone().into(),
        config.object_log_path.as_ref().map(Into::into),
    );
    tracing_subscriber::registry()
        .with(logging.error_layer())
        .with(logging.object_log_layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "RUST_LOG=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::debug!("Configuration: {:?}", &config);

    tracing::info!("Starting the Initialisation process");
    let listener = TcpListener::bind(format!("{}:{}", config.host, config.port)).await?;

    tracing::debug!("Started Listener");
    let sqlx_db = init_db_conn(&config.db_url).await?;

    // Run Key Administrative Functions

    let mut tx = sqlx_db.begin().await?;
    let key = &config.keyadder_key;
    let tag = utils::auth::keytag_of(key);
    let salt = utils::auth::generate_salt();
    let hash = utils::auth::hash_full_key(&salt, key);
    tracing::info!("Master key of this session has keytag {}", tag);

    sqlx::query!(
        "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag)
        VALUES
        ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq), $2, $3)
        ON CONFLICT DO NOTHING;", hash, salt, tag)
    .execute(&mut *tx).await?;

    tx.commit().await?;
    let mailbundle = crate::utils::notify::MailBundle::new(&config)?;

    let state = Arc::new(LTZFServer::new(sqlx_db, config, mailbundle, logging));
    tracing::debug!("Constructed Server State");

    // Init Axum router
    let (iv, cnt) = (
        u64::from(state.config.req_limit_interval),
        state.config.req_limit_count,
    );

    let govenor_config = GovernorConfigBuilder::default()
        .const_per_second(iv)
        .const_burst_size(cnt)
        .key_extractor(GlobalKeyExtractor)
        .finish();

    let rl_config = if let Some(govenor_config) = govenor_config {
        Arc::new(govenor_config)
    } else if iv == 0 {
        panic!("Invalid configuration. REQUEST_LIMIT_INTERVAL is zero.");
    } else if cnt == 0 {
        panic!("Invalid configuration. REQUEST_LIMIT_COUNT is zero.");
    } else {
        panic!(
            "Unexpected error. GovernorConfig did not build correctly, even though config seems valid."
        )
    };
    let limiter = Arc::clone(rl_config.limiter());
    let interval = std::time::Duration::from_secs(60);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            tracing::info!("rate limiting storage size: {}", limiter.len());
            limiter.retain_recent();
        }
    });
    let rate_limiter = GovernorLayer { config: rl_config };
    let body_size_limit = 1024 * 1024 * 1024 * 16; // 16 GB
    let request_size_limit = limit::RequestBodyLimitLayer::new(body_size_limit);
    let cors_layer = cors::CorsLayer::new()
        .allow_methods(vec![Method::GET])
        .allow_origin(cors::AllowOrigin::any())
        .expose_headers(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let compression_layer = CompressionLayer::new()
        .br(true)
        .deflate(true)
        .gzip(true)
        .zstd(true);

    let app = openapi::server::new(Arc::clone(&state))
        .route("/", get(handle_root))
        .route("/api/v2", get(swagger_ui))
        .layer(DefaultBodyLimit::max(body_size_limit))
        .layer(request_size_limit)
        .layer(rate_limiter)
        .layer(cors_layer)
        .layer(compression_layer);

    tracing::debug!("Constructed Router");
    tracing::info!(
        "Starting Server on {}:{}",
        state.config.host,
        state.config.port
    );
    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
