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
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{Message, Transport};
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

    #[arg(long, env = "PAZUFA_HOST", default_value = "0.0.0.0")]
    pub host: String,
    #[arg(long, env = "PAZUFA_PORT", default_value = "80")]
    pub port: u16,
    #[arg(long, short, env = "DATABASE_URL", help = "URL to the database")]
    pub db_url: String,

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
    /// # Errors
    ///
    /// This function may fail under three conditions:
    ///
    /// 1. The configuration does not contain all relevant information details:
    ///     1. `mail_server`
    ///     1. `mail_user`
    ///     1. `mail_password`
    ///     1. `mail_sender`
    ///     1. `mail_recipient`
    /// 1. The authentication credentials are invalid and rejected by the server.
    /// 1. The server is unreachable.
    pub fn build_mailer(&self) -> Result<(Mailbox, Mailbox, SmtpTransport)> {
        let (
            Some(mail_server),
            Some(mail_user),
            Some(mail_password),
            Some(mail_sender),
            Some(mail_recipient),
        ) = (
            &self.mail_server,
            &self.mail_user,
            &self.mail_password,
            &self.mail_sender,
            &self.mail_recipient,
        )
        else {
            return Err(LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message: "Mail Configuration is incomplete".into(),
                    config: Box::new(self.clone()),
                }),
            });
        };

        let sender = lettre::message::Mailbox::new(
            Some("PaZuFa".to_owned()),
            mail_sender.parse().map_err(|e| LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message: format!("Parsing mail sender to valid mail address failed: {e}"),
                    config: Box::new(self.clone()),
                }),
            })?,
        );

        let recipient: Mailbox = mail_recipient
            .parse()
            .map_err(|e| LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message: format!("Parsing mail recipient to valid mailbox failed: {e}"),
                    config: Box::new(self.clone()),
                }),
            })?;

        let mailer = SmtpTransport::relay(mail_server)?
            .credentials(Credentials::new(mail_user.clone(), mail_password.clone()))
            .build();
        if !mailer.test_connection()? {
            return Err(LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message:
                        "Connection to SMTP server failed. Probably invalid authentication data."
                            .into(),
                    config: Box::new(self.clone()),
                }),
            });
        }

        let Ok(message) = Message::builder()
            .from(sender.clone())
            .to(recipient.clone())
            .subject("Backend started. Testing mail setup.")
            .header(ContentType::TEXT_PLAIN)
            .body(
                "This is a test message, confirming the SMTP transport is configured correctly"
                    .to_owned(),
            )
        else {
            unreachable!();
        };

        mailer
            .send(&message)
            .map_err(|e| LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message: format!("Failed to send testmail using configuration: {e}"),
                    config: Box::new(self.clone()),
                }),
            })?;

        Ok((sender, recipient, mailer))
    }
}
