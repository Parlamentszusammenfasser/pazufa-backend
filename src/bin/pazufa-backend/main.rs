#![cfg_attr(test, deny(unsafe_code))]
#![cfg_attr(test, allow(clippy::expect_used, reason = "testing is a free-for-all"))]
#![cfg_attr(not(test), forbid(unsafe_code))]

use anyhow::{Context, Result, anyhow};
use axum::routing::get;
use axum::{extract::DefaultBodyLimit, http::Method};
use clap::Parser;
use pazufa_backend_lib::Configuration;
use pazufa_backend_lib::api::LTZFServer;
use pazufa_backend_lib::utils::auth as util_auth;
use pazufa_backend_lib::utils::{notify::MailBundle, shutdown_signal, tracing::Logging};
use sqlx::PgTransaction;
use std::process::exit;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use tower_http::{compression::CompressionLayer, cors, limit};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

mod auth;
mod misc;
mod sitzung;
#[cfg(test)]
mod testing;
mod unauthentifiziert;
mod vorgang;

#[allow(unused, reason = "only has to be instantiated lazily")]
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[allow(clippy::needless_for_each, reason = "IT IS IN A FUCKING MACRO")]
mod openapi {
    use utoipa::OpenApi;
    use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
    // initialize OpenAPI
    #[derive(OpenApi)]
    #[openapi(
    info(
        title= "LTZF API",
        version = "0.2.3",
        description = "Beschreibung der API des LTZF Projekts. Alle Listen, die vom Server kommen sind sortiert in\n  1. datum der letzten modifikation, absteigend\n  2. lexikographisch des titels falls nicht vorhanden"
        ),
    modifiers(&Security),
    tags((name= "Vorgang",                  description= "Endpoints related to legislative processes"),
         (name= "Sitzung",                  description= "Endpoints related to parliamentary sessions"),
         (name= "Data-Administration",      description= "Endpoints used for data administrative purposes"),
         (name= "Collector-Schnittstellen", description= "Endpoints to be used by collectors"),
         (name= "Unauthorisiert",           description= "Endpoints that do not require authentication"),
         (name= "Authentifizierung",        description= "Endpoints related to API key management"),
         (name= "Miscellaneous",            description= "Endpoints managing objects which cannot exist without a more higher-up object (Documents, Enumeration values etc)")
    ),
)]
    pub struct PazufAPI;
    struct Security;
    impl utoipa::Modify for Security {
        fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
            if let Some(components) = openapi.components.as_mut() {
                components.add_security_scheme(
                "apiKey",
                SecurityScheme::ApiKey(
                    ApiKey::Header(
                        ApiKeyValue::with_description("X-API-KEY", 
                            "
        API key of the Collector.
        - Expires based on 'expires_at' field when created (if not specified, defaults to 1 year)
        - Subject to rate limiting of 256 requests per second across all endpoints
        - Subject to body size limit of 16 GB
        - Tokens can be revoked at any time by administrators

        API Key Rotation Policy:
        - Keys should be rotated at least every 90 days (recommended)
        - A transition period of 1 hour is supported during rotation where both old and new keys are valid
        - When rotating keys, use the /api/v2/auth/rotate endpoint rather than creating and deleting keys separately
        - During rotation, both keys remain valid until the rotation_complete_date is reached
        - After the rotation_complete_date, the old key is automatically invalidated")
                    )
                ),
            );
            }
        }
    }
}

async fn handle_root() -> impl axum::response::IntoResponse {
    let s = include_str!("index.html");
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
        return Err(anyhow!("Server Connection failed after 10 retries"));
    }
    tracing::debug!("Started Database Pool");
    sqlx::migrate!().run(&sqlx_db).await?;
    tracing::debug!("Executed Migrations");
    Ok(sqlx_db)
}

async fn initialize_master_key(key: &str, tx: &mut PgTransaction<'_>) -> Result<()> {
    if !util_auth::check_key(key) {
        return Err(anyhow!(
            "Initialization keyadder key did not pass Sanity check. Is it maybe malformed or corrupted? Generate a key with <backend binary> --generate-key"
        ));
    }
    let tag = util_auth::keytag_of(key);
    let salt = util_auth::generate_salt();
    let hash = util_auth::hash_full_key(&salt, key);

    tracing::info!("Master key of this session has keytag {}", tag);

    sqlx::query!(
        "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag)
        VALUES
        ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq), $2, $3)
        ON CONFLICT DO NOTHING;", hash, salt, tag)
    .execute(&mut **tx).await?;
    Ok(())
}

fn initialize_router(state: &Arc<LTZFServer>) -> axum::Router {
    let (iv, cnt) = (
        u64::from(state.config.req_limit_interval),
        state.config.req_limit_count,
    );

    let governor_config = GovernorConfigBuilder::default()
        .const_per_second(iv)
        .const_burst_size(cnt)
        .key_extractor(GlobalKeyExtractor)
        .finish();

    let rl_config = if let Some(governor_config) = governor_config {
        Arc::new(governor_config)
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
    let interval = std::time::Duration::from_mins(1);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            tracing::debug!("rate limiting storage size: {}", limiter.len());
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

    let (router, api) = OpenApiRouter::with_openapi(openapi::PazufAPI::openapi())
        .routes(routes!(unauthentifiziert::status))
        .routes(routes!(unauthentifiziert::ping))
        .routes(routes!(auth::auth_rotate))
        .routes(routes!(auth::auth_status))
        .routes(routes!(auth::auth_post))
        .routes(routes!(auth::auth_delete))
        .routes(routes!(auth::auth_listing))
        .routes(routes!(auth::auth_listing_keytag))
        .routes(routes!(misc::enum_delete))
        .routes(routes!(misc::gremien_delete_by_param))
        .routes(routes!(misc::autoren_delete_by_param))
        .routes(routes!(vorgang::vorgang_get))
        .routes(routes!(vorgang::vorgang_put))
        .routes(routes!(vorgang::vorgang_get_by_id))
        .routes(routes!(vorgang::vorgang_delete))
        .routes(routes!(vorgang::vorgang_id_put))
        .routes(routes!(misc::autoren_get))
        .routes(routes!(misc::gremien_get))
        .routes(routes!(misc::dokument_get_by_id))
        .routes(routes!(misc::enum_get))
        .routes(routes!(misc::enum_put))
        .routes(routes!(misc::autoren_put))
        .routes(routes!(misc::gremien_put))
        .routes(routes!(misc::dokument_put_id))
        .routes(routes!(misc::dokument_delete_id))
        .routes(routes!(sitzung::sid_put))
        .routes(routes!(sitzung::kal_date_put))
        .routes(routes!(sitzung::kal_get))
        .routes(routes!(sitzung::kal_date_get))
        .routes(routes!(sitzung::s_get))
        .routes(routes!(sitzung::sitzung_delete))
        .routes(routes!(sitzung::s_get_by_id))
        .split_for_parts();

    router
        .merge(SwaggerUi::new("/api/v2").url("/openapi.json", api))
        .with_state(Arc::clone(state))
        .route("/", get(handle_root))
        .layer(DefaultBodyLimit::max(body_size_limit))
        .layer(request_size_limit)
        .layer(rate_limiter)
        .layer(cors_layer)
        .layer(compression_layer)
}

fn initialize_logging(config: &Configuration) -> Logging {
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
    logging
}

#[allow(clippy::disallowed_macros, reason = "Explicitly requested by user.")]
fn early_exits(config: &Configuration) {
    if config.dump_config {
        println!("{config:#?}");
        exit(0);
    }
    if config.generate_key {
        println!("{}", util_auth::generate_api_key());
        exit(0);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let config = Configuration::parse();
    early_exits(&config);

    let logging = initialize_logging(&config);
    tracing::debug!("Logging initialized");
    tracing::debug!("Configuration: {:?}", &config);

    let Some(ka_key) = &config.keyadder_key else {
        return Err(anyhow!(
            "Unable to start service without a master key. Please provide one via `--keyadder-key` or via PAZUFA_KEYADDER_KEY"
        ));
    };

    tracing::info!("Starting the Initialisation...");
    let listener = TcpListener::bind(format!("{}:{}", config.host, config.port))
        .await
        .with_context(|| format!("Tried to bind to {}:{}", config.host, config.port))?;

    tracing::debug!("TCP Listener Started");
    let sqlx_db = init_db_conn(&config.db_url).await?;

    // Run Key Administrative Functions

    let mut tx = sqlx_db.begin().await?;
    initialize_master_key(ka_key, &mut tx).await?;
    tx.commit().await?;
    tracing::debug!("Master Key initialized");

    let mailbundle = MailBundle::new(&config)?;
    tracing::debug!("Mail Bundle Constructed");

    let state = Arc::new(LTZFServer::new(sqlx_db, config, mailbundle, logging));
    tracing::debug!("Server State Constructed");

    // Init Axum router
    let app = initialize_router(&state);

    tracing::debug!("Router Constructed");
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
