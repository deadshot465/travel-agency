use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionToolArgs, ChatCompletionToolChoiceOption,
    ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionObjectArgs,
};
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use command_macros::command_handler;
use serde_json::json;
use serenity::all::{CommandData, CommandInteraction};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use crate::shared::CHATGPT_4O_LATEST;
use crate::shared::structs::AppState;
use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};

type CommandHandler =
    fn(CommandData, AppState) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

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
    ($command_name:expr, $data:expr, $app_state:expr) => {{
        let registry = COMMAND_REGISTRY.lock().unwrap();
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
        Ok(command_request) => {
            tracing::debug!(
                "Received incoming command interaction: {:?}",
                &command_request
            );
            let _ = handle_command_interaction(command_request, app_state);
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

async fn handle_command_interaction(
    interaction: CommandInteraction,
    app_state: AppState,
) -> anyhow::Result<()> {
    let command_name = interaction.data.name.clone();
    call_command!(command_name, interaction.data, app_state)?;

    Ok(())
}

#[command_handler]
async fn plan(data: CommandData, app_state: AppState) -> anyhow::Result<()> {
    let user_prompt = data.options[0]
        .value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_default();
    let system_prompt = app_state.config.language_decider_prompt.clone();

    let messages = vec![
        ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_prompt)
                .build()?,
        ),
        ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_prompt)
                .build()?,
        ),
    ];

    let tool = ChatCompletionToolArgs::default()
        .r#type(ChatCompletionToolType::Function)
        .function(FunctionObjectArgs::default()
            .name("get_language")
            .description("Determine the language of the user's prompt.")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "language": {
                        "type": "string",
                        "description": "The language of the user's prompt, e.g. Simplified Chinese, English, Japanese, etc.",
                        "enum": ["English", "Simplified Chinese", "Traditional Chinese", "Japanese"]
                    }
                },
                "required": ["language"]
            }))
            .strict(true)
            .build()?)
        .build()?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(CHATGPT_4O_LATEST)
        .messages(messages)
        .temperature(0.3)
        .tools(vec![tool])
        .tool_choice(ChatCompletionToolChoiceOption::Required)
        .build()?;

    let response = app_state
        .llm_clients
        .openai_client
        .chat()
        .create(request)
        .await;

    Ok(())
}
