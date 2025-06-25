use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionToolArgs, ChatCompletionToolChoiceOption,
    ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionObjectArgs, ResponseFormat,
    ResponseFormatJsonSchema,
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
use tokio::sync::Mutex;

use crate::shared::structs::AppState;
use crate::shared::structs::agent::{Language, LanguageTriageArgumants};
use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};
use crate::shared::{GEMINI_25_PRO, GPT_41};

type CommandHandler =
    fn(CommandData, AppState) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

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

#[axum::debug_handler]
pub async fn handle_interaction(State(app_state): State<AppState>, request: Bytes) -> Response {
    let bytes = request.to_vec();

    match serde_json::from_slice::<CommandInteraction>(&bytes) {
        Ok(command_request) => {
            tracing::debug!(
                "Received incoming command interaction: {:?}",
                &command_request
            );
            let _ = handle_command_interaction(command_request, app_state).await;
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

    let language = determine_language(&user_prompt, &app_state).await?;

    let orchestrator_system_prompt = match language {
        Language::Chinese => app_state.config.chinese_orchestrator_prompt.clone(),
        Language::Japanese => app_state.config.japanese_orchestrator_prompt.clone(),
        _ => app_state.config.english_orchestrator_prompt.clone(),
    };

    orchestrate(&orchestrator_system_prompt, &user_prompt, &app_state).await?;

    Ok(())
}

async fn determine_language(user_prompt: &str, app_state: &AppState) -> anyhow::Result<Language> {
    let system_prompt = app_state.config.language_triage_prompt.clone();

    let messages = build_one_shot_messages(&system_prompt, user_prompt)?;

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
                        "enum": ["English", "Chinese", "Japanese", "Other"]
                    }
                },
                "required": ["language"],
                "additionalProperties": false
            }))
            .strict(true)
            .build()?)
        .build()?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(GPT_41)
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

    match response {
        Ok(res) => {
            let arguments =
                res.choices
                    .first()
                    .and_then(|choice| {
                        let message = &choice.message;
                        message.tool_calls.as_ref().and_then(|calls| {
                            calls.first().map(|call| call.function.arguments.clone())
                        })
                    })
                    .unwrap_or_default();

            Ok(serde_json::from_str::<LanguageTriageArgumants>(&arguments)?.language)
        }
        Err(e) => {
            let error_msg = format!("Failed to call OpenAI API: {:?}. Fall back to English.", e);
            tracing::error!("{}", error_msg);
            Ok(Language::English)
        }
    }
}

async fn orchestrate(
    system_prompt: &str,
    user_prompt: &str,
    app_state: &AppState,
) -> anyhow::Result<()> {
    let messages = build_one_shot_messages(system_prompt, user_prompt)?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(GEMINI_25_PRO)
        .messages(messages)
        .temperature(0.3)
        .response_format(ResponseFormat::JsonSchema { json_schema: ResponseFormatJsonSchema {
            description: Some("Break the user's request into subtasks and orchestrate in order to get the final result.".into()),
            name: "orchestrate_tasks".into(),
            schema: Some(json!({
                "type": "object",
                "properties": {
                    "greeting_message": {
                        "type": "string",
                        "description": "Greeting message to greet the user and inform the user that you have received their request and is now planning the itinerary."
                    },
                    "analysis": {
                        "type": "string",
                        "description": "Brief analysis of what the user wants."
                    },
                    "tasks": {
                        "type": "array",
                        "description": "A list of tasks to assign to agents.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "task_id": {
                                    "type": "string",
                                    "description": "A unique task ID for each task."
                                },
                                "agent": {
                                    "type": "string",
                                    "description": "Agent name to assign this task to.",
                                    "enum": ["Food", "History", "Modern", "Nature", "Transport"]
                                },
                                "instruction": {
                                    "type": "string",
                                    "description": "Specific instruction for the agent to complete."
                                },
                                "dependencies": {
                                    "type": "array",
                                    "description": "List of task IDs that must complete before this task.",
                                    "items": {
                                        "type": "string"
                                    }
                                }
                            }
                        }
                    },
                    "synthesis_plan": {
                        "type": "string",
                        "description": "How you'll combine the results."
                    }
                },
                "required": ["greeting_message", "analysis", "tasks", "synthesis_plan"],
                "additionalProperties": false
            })),
            strict: Some(true),
        } })
        .build()?;

    let response = app_state
        .llm_clients
        .open_router_client
        .chat()
        .create(request)
        .await;

    match response {
        Ok(res) => {
            tracing::info!(
                "Orchestration response: {}",
                res.choices[0].message.content.clone().unwrap_or_default()
            )
        }
        Err(e) => {
            let error_msg = format!("Error when creating orchestration tasks: {:?}", e);
            tracing::error!("{}", &error_msg);
        }
    }

    Ok(())
}

fn build_one_shot_messages(
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<Vec<ChatCompletionRequestMessage>> {
    Ok(vec![
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
    ])
}
