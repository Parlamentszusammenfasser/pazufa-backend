#![cfg_attr(
    test,
    allow(clippy::expect_used, reason = "all is fair in love, war and testing")
)]
pub mod api;
pub mod db;
pub mod error;
pub mod models;
pub mod utils;

pub use crate::api::{LTZFArc, LTZFServer};
pub use crate::error::{LTZFError, Result};
use clap::Parser;
use lettre::{SmtpTransport, transport::smtp::authentication::Credentials};
use std::default::Default;
use std::fmt::Debug;

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
    pub track_path: String,
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
