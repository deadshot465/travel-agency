use axum::{Router, middleware::from_fn, routing::post};
use tracing::Level;

use crate::{
    controller::discord::interaction::handle_interaction,
    shared::middleware::discord_validation::validate_interaction,
};

mod controller;
mod shared;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_level = match std::env::var("LOG_LEVEL")?.as_str() {
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

    let app = Router::new()
        .route("/api/discord/interaction", post(handle_interaction))
        .layer(from_fn(validate_interaction));

    let server_bind_point = std::env::var("SERVER_BIND_POINT")?;
    let port = std::env::var("PORT")?;
    let server_bind_point = format!("{}:{}", server_bind_point, port);

    let listener = tokio::net::TcpListener::bind(&server_bind_point).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
