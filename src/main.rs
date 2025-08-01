use std::sync::Arc;

use axum::{Router, middleware::from_fn, routing::post};
use firestore::{FirestoreDb, FirestoreDbOptions};
use serenity::all::{ApplicationId, Http};
use tracing::Level;

use crate::{
    controller::discord::interaction::{COMMAND_REGISTRY, handle_interaction},
    shared::{
        USER_AGENT,
        middleware::discord_validation::validate_interaction,
        structs::{AppState, LLMClients, config::Configuration},
    },
};

mod controller;
mod shared;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_level = match std::env::var("LOG_LEVEL").unwrap_or_default().as_str() {
        "TRACE" => Level::TRACE,
        "INFO" => Level::INFO,
        "WARN" => Level::WARN,
        "ERROR" => Level::ERROR,
        _ => Level::DEBUG,
    };

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("Initialization of tracing subscriber failed with error: {e}");
    }

    let available_commands = COMMAND_REGISTRY
        .lock()
        .await
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    tracing::info!("Available commands: {:?}", &available_commands);

    let bot_token = std::env::var("BOT_TOKEN")?;

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to initialize TLS.");

    let sa_path = Configuration::config_directory()?.join(std::env::var("SA_FILE_NAME")?);

    let discord_http = Arc::new(Http::new(&bot_token));
    discord_http.set_application_id(ApplicationId::new(
        std::env::var("APPLICATION_ID")?.parse::<u64>()?,
    ));

    let app_state = AppState {
        config: Configuration::load_from_config_file()?,
        llm_clients: Arc::new(LLMClients::new()),
        http_client: reqwest::Client::builder().user_agent(USER_AGENT).build()?,
        http: discord_http,
        firestore_db: FirestoreDb::with_options_service_account_key_file(
            FirestoreDbOptions::new(std::env::var("PROJECT_ID")?),
            sa_path,
        )
        .await?,
        google_maps_client: Arc::new(::google_maps::Client::try_new(std::env::var(
            "GOOGLE_API_KEY",
        )?)?),
    };

    let app = Router::new()
        .route("/api/discord/interaction", post(handle_interaction))
        .layer(from_fn(validate_interaction))
        .with_state(app_state);

    let server_bind_point = std::env::var("SERVER_BIND_POINT")?;
    let port = std::env::var("PORT")?;
    let server_bind_point = format!("{server_bind_point}:{port}");

    let listener = tokio::net::TcpListener::bind(&server_bind_point).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
