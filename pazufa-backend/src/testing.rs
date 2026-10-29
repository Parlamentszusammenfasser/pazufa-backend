#![allow(
    clippy::disallowed_methods,
    reason = "Database setup and taredown requires use of `sqlx::query` as those cannot be statically verified."
)]
use pazufa_backend_lib::api::auth::Claims;
use pazufa_backend_lib::utils::tracing::Logging;
use pazufa_backend_lib::{Configuration, PazufaServer};
use pazufa_models::auth::Sha256KeyGenerator;
use pazufa_models::params::XAPIKey;
use pazufa_models::{APIKey, APIScope};
use sha256::digest;
use sqlx::SqlSafeStr;
use std::fmt::Debug;
use std::sync::{Arc, LazyLock};

static MASTER_URL: LazyLock<String> = LazyLock::new(|| {
    dotenvy::dotenv().ok();
    std::env::var("DATABASE_URL").expect("Cannot run test without the DATABASE_URL environment variable set. The use of .env files is supported.")
});

pub(crate) struct TestSetup {
    pub(crate) name: &'static str,
    pub(crate) server: Option<PazufaServer>,
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
        sqlx::query(
            sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {dbname} WITH (FORCE);"))
                .into_sql_str(),
        )
        .execute(&create_pool)
        .await
        .expect("Failed to drop existing database.");
        let _ = sqlx::query(
            sqlx::AssertSqlSafe(format!("CREATE USER \"{testuser}\" with PASSWORD 'test';"))
                .into_sql_str(),
        )
        .execute(&create_pool)
        .await;
        sqlx::query(
            sqlx::AssertSqlSafe(format!("CREATE DATABASE {dbname} WITH OWNER '{testuser}'"))
                .into_sql_str(),
        )
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
        sqlx::migrate!("../migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations.");
        let hash = digest("total-nutzloser-wert");
        sqlx::query!(
            "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag, algo)
            VALUES
            ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), 
            (SELECT last_value FROM api_keys_id_seq), $2, $3, 
            (SELECT id FROM hash_algorithm WHERE value = 'sha256')
            )
            ON CONFLICT DO NOTHING;",
            hash,
            "salziger-salzkeks".as_bytes(),
            "total-nutzlos",
        )
        .execute(&pool)
        .await
        .expect("Failed to create test API keys.");
        let logging = Logging::new("testing_error.log".into(), None);

        Self {
            name: dbname,
            create_pool,
            server: Some(PazufaServer::new(
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
        sqlx::query(
            sqlx::AssertSqlSafe(format!("DROP DATABASE {} WITH (FORCE);", self.name))
                .into_sql_str(),
        )
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
    server: &PazufaServer,
) -> pazufa_backend_lib::Result<(String, String, String)> {
    use pazufa_backend_lib::db::auth as dbauth;
    let mut tx = server.sqlx_db.begin().await?;
    let collector_key: APIKey<Sha256KeyGenerator> = dbauth::create_apikey(
        1,
        APIScope::Collector,
        chrono::Utc::now().checked_add_days(chrono::Days::new(1)),
        None,
        &mut *tx,
    )
    .await?;
    let admin_key: APIKey<Sha256KeyGenerator> = dbauth::create_apikey(
        1,
        APIScope::Admin,
        chrono::Utc::now().checked_add_days(chrono::Days::new(1)),
        None,
        &mut *tx,
    )
    .await?;
    let keyadder_key: APIKey<Sha256KeyGenerator> = dbauth::create_apikey(
        1,
        APIScope::KeyAdder,
        chrono::Utc::now().checked_add_days(chrono::Days::new(1)),
        None,
        &mut *tx,
    )
    .await?;
    tx.commit().await?;

    Ok((
        collector_key.to_string(),
        admin_key.to_string(),
        keyadder_key.to_string(),
    ))
}

pub async fn authtesting<F, T>(
    server: Arc<pazufa_backend_lib::PazufaServer>,
    allowed_levels: &[APIScope],
    testing_function: F,
    failure: T,
) where
    F: AsyncFn(Claims, Arc<pazufa_backend_lib::PazufaServer>) -> T,
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

/// A response as observed on the wire.
#[derive(Debug)]
pub(crate) struct TestResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
}
impl TestResponse {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub(crate) fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_str(&self.body).unwrap_or_else(|e| {
            panic!(
                "Failed to parse body as {}: {e}\n{self:?}",
                std::any::type_name::<T>()
            )
        })
    }
}

/// Serves all API routers (without the production middleware layers) on a random local port,
/// so endpoints can be called by URL and headers only.
pub(crate) struct TestClient {
    addr: std::net::SocketAddr,
    handle: tokio::task::JoinHandle<()>,
}
impl TestClient {
    pub(crate) async fn new(server: Arc<PazufaServer>) -> Self {
        let (router, _) = utoipa_axum::router::OpenApiRouter::new()
            .nest("/api/v2/auth", crate::auth::router())
            .nest("/api/v2/autoren", crate::autoren::router())
            .nest("/api/v2/dokument", crate::dokument::router())
            .nest("/api/v2/enumeration", crate::enumeration::router())
            .nest("/api/v2/gremien", crate::gremien::router())
            .nest("/api/v2/kalender", crate::kalender::router())
            .nest("/api/v2/sitzung", crate::sitzung::router())
            .nest("/api/v2/vorgang", crate::vorgang::router())
            .merge(crate::unauthentifiziert::router())
            .split_for_parts();
        let app = router.with_state(server);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind test listener");
        let addr = listener
            .local_addr()
            .expect("Failed to get test listener address");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("Test server failed");
        });
        Self { addr, handle }
    }

    pub(crate) async fn call(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> TestResponse {
        use std::fmt::Write;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut request = format!(
            "{method} {url} HTTP/1.1\r\nhost: {}\r\nconnection: close\r\n",
            self.addr
        );
        for (k, v) in headers {
            write!(request, "{k}: {v}\r\n").expect("Failed to write header");
        }
        if let Some(body) = body {
            write!(
                request,
                "content-type: application/json\r\ncontent-length: {}\r\n",
                body.len()
            )
            .expect("Failed to write header");
        }
        request.push_str("\r\n");
        request.push_str(body.unwrap_or_default());

        let mut stream = tokio::net::TcpStream::connect(self.addr)
            .await
            .expect("Failed to connect to test server");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("Failed to send request");
        let mut raw = String::new();
        stream
            .read_to_string(&mut raw)
            .await
            .expect("Failed to read response");

        let (head, body) = raw.split_once("\r\n\r\n").expect("Malformed HTTP response");
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .expect("Malformed HTTP status line");
        let headers = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_owned()))
            .collect();
        TestResponse {
            status,
            headers,
            body: body.to_owned(),
        }
    }

    pub(crate) async fn get(&self, url: &str, headers: &[(&str, &str)]) -> TestResponse {
        self.call("GET", url, headers, None).await
    }
}
impl Drop for TestClient {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// A minimal Bundestag session in electoral period 20.
pub(crate) fn test_sitzung(
    termin: chrono::DateTime<chrono::Utc>,
    nummer: u16,
) -> pazufa_models::Sitzung {
    use pazufa_models as models;
    models::Sitzung {
        titel: Some(format!("Testsitzung {nummer}")),
        termin,
        nummer,
        public: true,
        gremium: models::Gremium {
            parlament: models::Parlament::Bt,
            wahlperiode: 20,
            link: None,
            name: "Ausschuss für Tests".to_owned(),
        },
        ..Default::default()
    }
}

/// Inserts `sessions` for the day of their first element via the kalender PUT endpoint.
/// The sessions must be in the Bundestag and no older than yesterday.
pub(crate) async fn seed_sitzungen(
    client: &TestClient,
    server: &PazufaServer,
    sessions: &[pazufa_models::Sitzung],
) {
    let (collector_key, _, _) = gen_all_keylevels(server)
        .await
        .expect("Failed to generate api keys");
    let datum = sessions
        .first()
        .expect("Expected at least one session")
        .termin
        .date_naive();
    let body = serde_json::to_string(sessions).expect("Failed to serialize sessions");
    let rsp = client
        .call(
            "PUT",
            &format!("/api/v2/kalender/BT/{datum}"),
            &[
                ("x-api-key", &collector_key),
                ("x-scraper-id", &uuid::Uuid::nil().to_string()),
            ],
            Some(&body),
        )
        .await;
    assert_eq!(rsp.status, 201, "Seeding failed: {rsp:?}");
}

pub(crate) fn tomorrow(dt: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    dt.checked_add_days(chrono::Days::new(1))
        .expect("Date out of range")
}
pub(crate) fn yesterday(dt: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    dt.checked_sub_days(chrono::Days::new(1))
        .expect("Date out of range")
}

/// Formats a timestamp for use in URLs and headers (no `+` that would need escaping).
pub(crate) fn rfc3339(dt: chrono::DateTime<chrono::Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
