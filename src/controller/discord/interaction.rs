use axum::body::Bytes;
use axum::http::StatusCode;
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use command_macros::command_handler;
use serenity::all::{CommandData, CommandInteraction};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};

type CommandHandler = fn(CommandData) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

lazy_static::lazy_static! {
    pub static ref COMMAND_REGISTRY: Mutex<HashMap<String, CommandHandler>> = Mutex::new(HashMap::new());
}

pub fn register_command(name: &str, handler: CommandHandler) {
    COMMAND_REGISTRY
        .lock()
        .unwrap()
        .insert(name.to_string(), handler);
}

macro_rules! call_command {
    ($command_name:expr, $data:expr) => {{
        let registry = COMMAND_REGISTRY.lock().unwrap();
        if let Some(handler) = registry.get($command_name.as_str()) {
            handler($data).await
        } else {
            Err(anyhow::anyhow!("Unknown command: {}", $command_name))
        }
    }};
}

pub async fn handle_interaction(request: Bytes) -> Response {
    let bytes = request.to_vec();

    match serde_json::from_slice::<CommandInteraction>(&bytes) {
        Ok(command_request) => {
            tracing::debug!(
                "Received incoming command interaction: {:?}",
                &command_request
            );
            let _ = handle_command_interaction(command_request);
            StatusCode::OK.into_response()
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
                let error_message = format!("Failed to deserialize incoming payload: {}", e);
                tracing::error!("{}", &error_message);
                StatusCode::BAD_REQUEST.into_response()
            }
        },
    }
}

async fn handle_command_interaction(interaction: CommandInteraction) -> anyhow::Result<()> {
    let command_name = interaction.data.name.clone();
    call_command!(command_name, interaction.data)?;

    Ok(())
}

#[command_handler]
async fn plan(_data: CommandData) -> anyhow::Result<()> {
    Ok(())
}
