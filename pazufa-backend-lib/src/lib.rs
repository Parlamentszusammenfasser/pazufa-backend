#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "all is fair in love, war and testing")
)]
#![cfg_attr(
    test,
    allow(
        clippy::arithmetic_side_effects,
        reason = "all is fair in love, war and testing"
    )
)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, reason = "all is fair in love, war and testing")
)]
pub mod api;
pub mod db;
pub mod error;
pub mod utils;

pub use crate::api::{LTZFArc, PazufaServer};
pub use crate::error::{PazufaError, Result};
use clap::Parser;
use std::default::Default;
use std::fmt::Debug;

#[derive(Parser, Clone, Debug, Default)]
#[command(author, version, about)]
pub struct Configuration {
    #[arg(long, env = "PAZUFA_HOST", default_value = "0.0.0.0")]
    pub host: String,
    #[arg(long, env = "PAZUFA_PORT", default_value = "80")]
    pub port: u16,
    #[arg(
        long,
        short,
        env = "DATABASE_URL",
        help = "URL to the database",
        default_value = "invalid_url"
    )]
    pub db_url: String,

    #[arg(long, short, env = "PUBLIC_URL")]
    pub public_url: Option<String>,

    #[arg(
        long,
        env = "PAZUFA_RO_DBPASS",
        default_value = "pazufa-ro-pass",
        help = "The password for the read-only user 'pazufa_ro'"
    )]
    pub read_only_dbpass: String,

    #[arg(
        long,
        env = "PAZUFA_KEYADDER_KEY",
        help = "The API Key that is used to add new Keys. This is saved in the database. Required to be set somehow if you want to run the backend itself"
    )]
    pub keyadder_key: Option<String>,

    #[arg(long, env = "MERGE_TITLE_SIMILARITY", default_value = "0.8")]
    pub merge_title_similarity: f32,
    #[arg(
        long,
        env = "REQUEST_LIMIT_COUNT",
        help = "Global Request Bucket Size",
        default_value = "512"
    )]
    pub req_limit_count: u32,
    #[arg(
        long,
        env = "REQUEST_LIMIT_INTERVAL",
        help = "(whole) number of milliseconds, after which the request bucket gets replenished by 1 token",
        default_value = "10"
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
    pub track_path: String,
    #[arg(long, env = "PAZUFA_ERROR_LOG", default_value = "./error.log")]
    pub error_log_path: String,

    #[arg(
        long,
        env = "PAZUFA_OBJECT_LOG",
        help = "If you want to log object lifecycle operations(create/delete/merge/...), enter a path"
    )]
    pub object_log_path: Option<String>,

    #[arg(
        long,
        help = "If you want to check the config before executing the server, run this and 
        the server will print it's configuration considering all inputs and then exit."
    )]
    pub dump_config: bool,

    #[arg(
        long,
        help = "Prints the Openapi.json this server is configured to serve"
    )]
    pub dump_spec: bool,

    #[arg(
        long,
        help = "Generates a Key that conforms to pazufa's format, then exits with success"
    )]
    pub generate_key: bool,
}
impl Configuration {
    pub const PUBLIC_URL_DEFAULT: &str = "http://localhost:80";
}
