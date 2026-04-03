#[cfg(feature = "track-validation")]
use crate::db::validate::Tracks;
use chrono::DateTime;
use clap::crate_version;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{instrument, warn};

// the maxmium number of user agents / pathcount entries.
// this is to prevent clogging memory at runtime
const MAX_COUNT: usize = 1024;

#[derive(Serialize)]
struct StatOutput {
    spec_version: String,
    version: String,
    commit: String,
    #[cfg(feature = "track-validation")]
    track_version: String,

    unique_uas: u32,
    most_common_ua: String,
    objects_created: u32,
    up_since: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug)]
pub struct Statistics {
    pub pathcounts: RwLock<HashMap<String, u32>>,
    pub useragents: RwLock<HashMap<String, u32>>,
    pub up_since: DateTime<chrono::Utc>,
    pub objects_created: RwLock<u32>,
    #[cfg(feature = "track-validation")]
    pub track_version: String,
    pub put_with_no_change: RwLock<u32>,
}

impl Statistics {
    #[cfg(feature = "track-validation")]
    pub fn new(track_path: &str) -> Self {
        Self {
            pathcounts: HashMap::new().into(),
            useragents: HashMap::new().into(),
            up_since: chrono::Utc::now(),
            objects_created: 0.into(),
            track_version: Tracks::new(track_path).version,
            put_with_no_change: 0.into(),
        }
    }

    #[cfg(not(feature = "track-validation"))]
    pub fn new() -> Self {
        Self {
            pathcounts: HashMap::new().into(),
            useragents: HashMap::new().into(),
            up_since: chrono::Utc::now(),
            objects_created: 0.into(),
            put_with_no_change: 0.into(),
        }
    }
    pub async fn obj_created(&self) {
        let mut r = self.objects_created.write().await;
        *r += 1;
    }
    pub async fn put_nochange(&self) {
        let mut r = self.put_with_no_change.write().await;
        *r += 1;
    }
    #[instrument(skip_all, fields(ua=?ua))]
    pub async fn ua_registered(&self, ua: &str) {
        let mut r = self.useragents.write().await;
        if r.len() > MAX_COUNT {
            warn!(
                "Too many unique user agents recorded {MAX_COUNT}, discarding {}",
                ua
            );
            return;
        }
        let ua_cnt: u32 = *r.get(ua).unwrap_or(&0);
        r.insert(ua.to_owned(), ua_cnt + 1);
    }
    #[instrument(skip_all, fields(path=?path))]
    pub async fn path_called(&self, path: &str) {
        let mut r = self.pathcounts.write().await;
        let cnt: u32 = *r.get(path).unwrap_or(&0);
        r.insert(path.to_owned(), cnt + 1);
    }
    pub async fn output(&self) -> String {
        let out = StatOutput {
            unique_uas: self.useragents.read().await.len() as u32,
            objects_created: *self.objects_created.read().await,
            up_since: self.up_since,
            most_common_ua: self
                .useragents
                .read()
                .await
                .iter()
                .max_by(|a, b| a.1.cmp(b.1))
                .unwrap_or((&"None recorded".to_owned(), &0))
                .0
                .to_string(),
            commit: env!("GIT_HASH").to_owned(),
            version: crate_version!().to_owned(),
            #[cfg(feature = "track-validation")]
            track_version: self.track_version.clone(),
            spec_version: env!("OPENAPI_SERVER_VERSION").to_owned(),
        };
        serde_json::to_string(&out).expect("Expected to be able to serialize StatOutput struct")
    }
}
