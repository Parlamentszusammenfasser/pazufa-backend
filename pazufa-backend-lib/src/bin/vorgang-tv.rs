#![allow(
    clippy::disallowed_macros,
    reason = "I will not do a full logging init for a little program"
)]
use clap::Parser;
use pazufa_backend_lib::db::validate;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
struct Config {
    #[arg(long, short)]
    tracks: PathBuf,
    content: clap_stdin::MaybeStdin<String>,
}

#[allow(clippy::expect_used, reason = "this function is just a one-off")]
#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "RUST_LOG=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let config = Config::parse();
    let tracks = validate::Tracks::new(
        config
            .tracks
            .to_str()
            .expect("Track Path Could not be made a &str"),
    );
    let vorgang = serde_json::from_str(&config.content).expect("Vorgang could not be parsed");
    match tracks.validate(&vorgang) {
        Ok(()) => println!("Successful validation"),
        Err(e) => {
            println!("Error validating Vorgang: {e}");
            std::process::exit(1);
        }
    }
}
