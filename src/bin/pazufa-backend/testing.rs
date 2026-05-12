#![allow(
    clippy::disallowed_methods,
    reason = "Database setup and taredown requires use of `sqlx::query` as those cannot be statically verified."
)]
use pazufa_backend_lib::api::auth::{APIScope, Claims};
use pazufa_backend_lib::models::params::XAPIKey;
use pazufa_backend_lib::utils::tracing::Logging;
use pazufa_backend_lib::{Configuration, LTZFServer};
use sha256::digest;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock};

static MASTER_URL: LazyLock<String> = LazyLock::new(|| {
    dotenv::dotenv().ok();
    std::env::var("DATABASE_URL").expect("Cannot run test without the DATABASE_URL environment variable set. The use of .env files is supported.")
});

pub(crate) struct TestSetup {
    pub(crate) name: &'static str,
    pub(crate) server: Option<LTZFServer>,
    create_pool: sqlx::PgPool,
}
impl TestSetup {
    pub(crate) async fn new(dbname: &'static str) -> Self {
        let create_pool = sqlx::PgPool::connect(&MASTER_URL)
            .await
            .expect("Failed to establish connection to database server.");
        let user: String = sqlx::query_scalar::<_, String>("SELECT CURRENT_ROLE;")
            .fetch_one(&create_pool)
            .await
            .expect("Failed to get current user.");
        let testuser = format!("test_{user}");
        sqlx::query(&format!("DROP DATABASE IF EXISTS {dbname} WITH (FORCE);"))
            .execute(&create_pool)
            .await
            .expect("Failed to drop existing database.");
        let _ = sqlx::query(&format!("CREATE USER \"{testuser}\" with PASSWORD 'test';"))
            .execute(&create_pool)
            .await;
        sqlx::query(&format!("CREATE DATABASE {dbname} WITH OWNER '{testuser}'"))
            .execute(&create_pool)
            .await
            .expect("Failed to create test database.");

        let new_pool = match (MASTER_URL.rfind('/'), MASTER_URL.rfind('@')) {
            (Some(slash_pos), Some(at_pos)) => format!(
                "postgres://{testuser}:test@{}/{dbname}",
                &MASTER_URL[at_pos.saturating_add(1)..slash_pos]
            ),
            (Some(slash_pos), None) => {
                let double_slash_pos = MASTER_URL
                    .find("//")
                    .expect("Did not find \"//\" in DATABASE_URL. Invalid URL.");
                format!(
                    "postgres://{testuser}:test@{}/{dbname}",
                    &MASTER_URL[double_slash_pos.saturating_add(2)..slash_pos]
                )
            }
            (None, _) => unreachable!(),
        };

        let pool = sqlx::PgPool::connect(&new_pool)
            .await
            .expect("Failed to establish connection to database server.");
        sqlx::migrate!()
            .run(&pool)
            .await
            .expect("Failed to run migrations.");
        let hash = digest("total-nutzloser-wert");
        sqlx::query!(
            "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag)
            VALUES
            ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq), $2, $3)
            ON CONFLICT DO NOTHING;", hash, "salziger-salzkeks", "total-nutzlos")
        .execute(&pool).await.expect("Failed to create test API keys.");
        let logging = Logging::new("testing_error.log".into(), None);

        Self {
            name: dbname,
            create_pool,
            server: Some(LTZFServer::new(
                pool,
                Configuration {
                    per_object_scraper_log_size: 5,
                    track_path: "tracks.toml".to_owned(),
                    ..Default::default()
                },
                None,
                logging,
            )),
        }
    }
    pub(crate) async fn teardown(&self) {
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE);", self.name))
            .execute(&self.create_pool)
            .await
            .expect("Failed to drop test database.");
        let _ = std::fs::remove_file("testing_error.log");
    }
}

/// returns (collector, admin, keyadder)
/// # Errors
/// Can fail if any of the key gens fail
pub async fn gen_all_keylevels(
    server: &LTZFServer,
) -> pazufa_backend_lib::Result<(String, String, String)> {
    use pazufa_backend_lib::api::auth as libauth;
    let collector_key = libauth::auth_post(
        server,
        &Claims(APIScope::KeyAdder, 1),
        &libauth::CreateApiKey {
            scope: "collector".to_owned(),
            expires_at: chrono::Utc::now().checked_add_days(chrono::Days::new(1)),
        },
    )
    .await?;
    let admin_key = libauth::auth_post(
        server,
        &Claims(APIScope::KeyAdder, 1),
        &libauth::CreateApiKey {
            scope: "admin".to_owned(),
            expires_at: chrono::Utc::now().checked_add_days(chrono::Days::new(1)),
        },
    )
    .await?;
    let keyadder_key = libauth::auth_post(
        server,
        &Claims(APIScope::KeyAdder, 1),
        &libauth::CreateApiKey {
            scope: "keyadder".to_owned(),
            expires_at: chrono::Utc::now().checked_add_days(chrono::Days::new(1)),
        },
    )
    .await?;
    Ok((collector_key, admin_key, keyadder_key))
}
pub async fn authtesting<F, T>(
    server: Arc<pazufa_backend_lib::LTZFServer>,
    allowed_levels: &[APIScope],
    testing_function: F,
    failure: T,
) where
    F: AsyncFn(Claims, Arc<pazufa_backend_lib::LTZFServer>) -> T,
    T: Sized + PartialEq + Debug,
{
    let (coll, adm, keya) = gen_all_keylevels(server.as_ref())
        .await
        .expect("Expected flawless key generation");
    for (level, key) in [
        (APIScope::Collector, coll),
        (APIScope::Admin, adm),
        (APIScope::KeyAdder, keya),
    ] {
        let claims = Claims::extract(&server, XAPIKey(key))
            .await
            .expect("Failed to extract claims.");
        let rsp = testing_function(claims, Arc::clone(&server)).await;
        if allowed_levels.contains(&level) {
            assert_ne!(rsp, failure, "for level {level}");
        } else {
            assert_eq!(rsp, failure, "for level {level}");
        }
    }
}
