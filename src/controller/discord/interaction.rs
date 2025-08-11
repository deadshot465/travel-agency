use axum::body::Bytes;
use axum::http::StatusCode;
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use serenity::all::{
    CommandInteraction, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::Mutex;

use crate::shared::structs::AppState;
use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};

type CommandHandler =
    fn(CommandInteraction, AppState) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

lazy_static::lazy_static! {
    pub static ref COMMAND_REGISTRY: Mutex<HashMap<String, CommandHandler>> = Mutex::new(HashMap::new());
}

pub fn register_command(name: &str, handler: CommandHandler) {
    COMMAND_REGISTRY
        .blocking_lock()
        .insert(name.to_string(), handler);
}

macro_rules! call_command {
    ($command_name:expr, $data:expr, $app_state:expr) => {{
        let registry = COMMAND_REGISTRY.lock().await;
        if let Some(handler) = registry.get($command_name.as_str()) {
            handler($data, $app_state).await
        } else {
            Err(anyhow::anyhow!("Unknown command: {}", $command_name))
        }
    }};
}

pub async fn handle_interaction(State(app_state): State<AppState>, request: Bytes) -> Response {
    let bytes = request.to_vec();

    match serde_json::from_slice::<CommandInteraction>(&bytes) {
        Ok(command_interaction) => {
            tokio::spawn(async move {
                if let Err(e) = handle_command_interaction(command_interaction, app_state).await {
                    let error_msg = format!("Error when handling command interaction: {e:?}");
                    tracing::error!("{}", error_msg);
                }
            });

            let response =
                CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new());

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(_) => match serde_json::from_slice::<InteractionRequest>(&bytes) {
            Ok(ping_request) => {
                if ping_request.r#type == 1 {
                    (StatusCode::OK, Json(InteractionResponse { r#type: 1 })).into_response()
                } else {
                    StatusCode::BAD_REQUEST.into_response()
                }
            }
            Err(e) => {
                let error_message = format!("Failed to deserialize incoming payload: {e:?}");
                tracing::error!("{}", &error_message);
                StatusCode::BAD_REQUEST.into_response()
            }
        },
    }
}

async fn handle_command_interaction(
    interaction: CommandInteraction,
    app_state: AppState,
) -> anyhow::Result<()> {
    let command_name = interaction.data.name.clone();
    call_command!(command_name, interaction, app_state)?;

    Ok(())
}
