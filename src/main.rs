use axum::{Router, routing::post};
use tracing::Level;

use crate::{controller::ping::ack_ping, shared::structs::config::CONFIGURATION};

mod controller;
mod shared;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_level = match CONFIGURATION.log_level.as_str() {
        "TRACE" => Level::TRACE,
        "INFO" => Level::INFO,
        "WARN" => Level::WARN,
        "ERROR" => Level::ERROR,
        _ => Level::DEBUG,
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(log_level)
        .pretty()
        .finish();

    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!(
            "Initialization of tracing subscriber failed with error: {}",
            e
        );
    }

    let app = Router::new().route("/api/ping", post(ack_ping));

    let listener = tokio::net::TcpListener::bind(&CONFIGURATION.server_bind_point).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
