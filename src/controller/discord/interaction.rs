use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs, ChatCompletionToolArgs,
    ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
    FunctionObjectArgs, ResponseFormat, ResponseFormatJsonSchema, Role,
};
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use command_macros::command_handler;
use dashmap::DashMap;
use serde_json::json;
use serenity::all::{
    ChannelId, CommandInteraction, CreateEmbed, CreateEmbedAuthor, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, CreateThread, EditInteractionResponse,
    EditMessage, GuildChannel, Message,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use crate::shared::structs::AppState;
use crate::shared::structs::agent::record::{Content, GenerationDump, PlanRecord};
use crate::shared::structs::agent::record::{Message as RecordMessage, PlanMapping};
use crate::shared::structs::agent::{
    Agent, Context, Executor, FinalResult, Language, LanguageModel, LanguageTriageArguments,
    OrchestrationPlan, Task, Taskable,
};
use crate::shared::structs::discord::interaction::{InteractionRequest, InteractionResponse};
use crate::shared::utility::{build_one_shot_messages, create_avatar_url};
use crate::shared::{
    EMBED_COLOR, GEMINI_25_FLASH, GEMINI_25_PRO, GPT_41, PLAN_COLLECTION_NAME,
    PLAN_MAPPING_COLLECTION_NAME, TEMPERATURE_LOW, TEMPERATURE_MEDIUM,
};

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

#[axum::debug_handler]
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

#[command_handler]
async fn plan(interaction: CommandInteraction, app_state: AppState) -> anyhow::Result<()> {
    let user_prompt = interaction.data.options[0]
        .value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_default();

    let language = determine_language(&user_prompt, &app_state).await?;

    let orchestrator_system_prompt = match language {
        Language::Chinese => app_state.config.chinese.orchestrator.prompt.clone(),
        Language::Japanese => app_state.config.japanese.orchestrator.prompt.clone(),
        _ => app_state.config.english.orchestrator.prompt.clone(),
    };

    let orchestration_response =
        orchestrate(&orchestrator_system_prompt, &user_prompt, &app_state).await;
    let (message, orchestration) = match orchestration_response {
        Ok(response) => (response.greeting_message.clone(), response),
        Err(e) => (format!("{e:?}"), OrchestrationPlan::default()),
    };

    let mut plan_record = PlanRecord {
        id: uuid::Uuid::now_v7(),
        messages: vec![
            RecordMessage {
                role: Role::System,
                content: Content::Plain(orchestrator_system_prompt.clone()),
            },
            RecordMessage {
                role: Role::User,
                content: Content::Plain(user_prompt.clone()),
            },
            RecordMessage {
                role: Role::Assistant,
                content: Content::Dynamic(serde_json::to_value(&orchestration)?),
            },
        ],
        dumps: vec![GenerationDump {
            model: LanguageModel::Gemini25Pro,
            content: orchestration.to_string(),
        }],
    };

    let edited_message = send_greeting(&interaction, message, &app_state).await?;
    let thread = create_thread(&edited_message, language, &app_state).await?;

    let (maybe_message, results) = execute_plan(
        orchestration,
        language,
        thread.id,
        &mut plan_record,
        &app_state,
    )
    .await?;

    if let Some(message_mutex) = maybe_message {
        {
            let mut message = message_mutex.lock().await;

            if let Some(original_embed) = message.embeds.first()
                && let Some(ref original_desc) = original_embed.description
            {
                let mut new_embed = original_embed.clone();
                new_embed.description =
                    Some(format!("{original_desc}\n🔄 Synthesizing final result..."));

                let edit_message_args = EditMessage::new().embed(CreateEmbed::from(new_embed));

                let new_message = app_state
                    .http
                    .edit_message(message.channel_id, message.id, &edit_message_args, vec![])
                    .await?;

                *message = new_message;
            }
        }

        let final_result = synthesize(language, results, &mut plan_record, &app_state).await?;

        insert_record(plan_record, thread.id, &app_state).await?;

        send_final_result_message(final_result, thread.id, &app_state).await?;
    }

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
        .temperature(TEMPERATURE_LOW)
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

            Ok(serde_json::from_str::<LanguageTriageArguments>(&arguments)?.language)
        }
        Err(e) => {
            let error_msg = format!("Failed to call OpenAI API: {e:?}. Fall back to English.");
            tracing::error!("{}", error_msg);
            Ok(Language::English)
        }
    }
}

async fn orchestrate(
    system_prompt: &str,
    user_prompt: &str,
    app_state: &AppState,
) -> anyhow::Result<OrchestrationPlan> {
    let messages = build_one_shot_messages(system_prompt, user_prompt)?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(GEMINI_25_PRO)
        .messages(messages)
        .temperature(TEMPERATURE_LOW)
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
                            },
                            "required": ["task_id", "agent", "instruction", "dependencies"],
                            "additionalProperties": false
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
        .open_router_clients
        .get(&Agent::default())
        .expect("Failed to get the Open Router client for orchestration.")
        .chat()
        .create(request)
        .await;

    match response {
        Ok(res) => {
            let content = res.choices[0].message.content.clone().unwrap_or_default();
            let orchestration_plan = serde_json::from_str::<OrchestrationPlan>(&content)?;
            tracing::info!("Orchestration response: {:?}", &orchestration_plan);
            Ok(orchestration_plan)
        }
        Err(e) => {
            let error_msg = format!("Error when creating orchestration tasks: {e:?}");
            tracing::error!("{}", &error_msg);
            Err(anyhow::anyhow!("{}", error_msg))
        }
    }
}

async fn send_greeting(
    interaction: &CommandInteraction,
    message: String,
    app_state: &AppState,
) -> anyhow::Result<Message> {
    let edit_content = EditInteractionResponse::new().content(message);

    let response = app_state
        .http
        .edit_original_interaction_response(&interaction.token, &edit_content, Vec::new())
        .await;

    match response {
        Ok(message) => Ok(message),
        Err(e) => {
            let error_msg = format!("Failed to edit the original response: {e:?}");
            tracing::error!("{}", &error_msg);
            Err(anyhow::anyhow!("{}", error_msg))
        }
    }
}

async fn create_thread(
    message: &Message,
    language: Language,
    app_state: &AppState,
) -> anyhow::Result<GuildChannel> {
    let title = name_thread(message, language, app_state).await?;

    let create_thread_args =
        CreateThread::new(title).auto_archive_duration(serenity::all::AutoArchiveDuration::OneWeek);

    let response = app_state
        .http
        .create_thread_from_message(message.channel_id, message.id, &create_thread_args, None)
        .await;

    match response {
        Ok(guild_channel) => Ok(guild_channel),
        Err(e) => {
            let error_msg = format!("Failed to create a discussion thread: {e:?}");
            tracing::error!("{}", &error_msg);
            Err(anyhow::anyhow!("{}", error_msg))
        }
    }
}

async fn name_thread(
    message: &Message,
    language: Language,
    app_state: &AppState,
) -> anyhow::Result<String> {
    let system_prompt = match language {
        Language::Chinese => app_state.config.chinese.naming.prompt.clone(),
        Language::Japanese => app_state.config.japanese.naming.prompt.clone(),
        _ => app_state.config.english.naming.prompt.clone(),
    };

    let messages = build_one_shot_messages(&system_prompt, &message.content)?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(GEMINI_25_FLASH)
        .temperature(TEMPERATURE_MEDIUM)
        .messages(messages)
        .build()?;

    let response = app_state
        .llm_clients
        .open_router_clients
        .get(&Agent::default())
        .expect("Failed to get the Open Router client for renaming thread.")
        .chat()
        .create(request)
        .await
        .map(|res| {
            res.choices
                .first()
                .and_then(|choice| choice.message.content.clone())
                .unwrap_or_default()
        });

    Ok(response?)
}

async fn execute_plan(
    orchestration: OrchestrationPlan,
    language: Language,
    discussion_thread_id: ChannelId,
    plan_record: &mut PlanRecord,
    app_state: &AppState,
) -> anyhow::Result<(Option<Arc<Mutex<Message>>>, Vec<Context>)> {
    if orchestration.tasks.is_empty() {
        return Ok((None, vec![]));
    }

    let app_info = app_state.http.get_current_application_info().await?;
    let icon_hash = app_info
        .icon
        .expect("Failed to get application's icon hash.");
    let icon_url = create_avatar_url(app_info.id.get(), icon_hash);

    let description = format!(
        "- Analysis: {}\n- Number of tasks: {}\n\n🚀 Executing tasks...",
        &orchestration.analysis,
        orchestration.tasks.len()
    );

    let message_args = CreateMessage::new().embed(
        CreateEmbed::new()
            .author(CreateEmbedAuthor::new(&app_info.name).icon_url(icon_url))
            .color(EMBED_COLOR)
            .description(description)
            .title("Execution Plan"),
    );

    let embed_message = app_state
        .http
        .send_message(discussion_thread_id, Vec::new(), &message_args)
        .await?;

    let message_mutex = Arc::new(tokio::sync::Mutex::new(embed_message));

    let executors = create_executors(&orchestration.tasks, language, app_state);

    let mut join_set = JoinSet::new();
    let contexts = Arc::new(DashMap::new());

    for mut executor in executors.into_iter() {
        {
            let mut message = message_mutex.lock().await;
            if let Some(original_embed) = message.embeds.first()
                && let Some(ref original_desc) = original_embed.description
            {
                let mut new_embed = original_embed.clone();
                new_embed.description = Some(format!(
                    "{}\nExecuting {} with {} Agent...",
                    original_desc,
                    executor.task_id.clone(),
                    executor.agent_type
                ));

                let edit_message_args = EditMessage::new().embed(CreateEmbed::from(new_embed));

                let new_message = app_state
                    .http
                    .edit_message(message.channel_id, message.id, &edit_message_args, vec![])
                    .await?;

                *message = new_message;
            }
        }

        let llm_clients_clone = app_state.llm_clients.clone();
        let contexts_clone = contexts.clone();
        let task_id = executor.task_id.clone();
        let message_mutex_clone = message_mutex.clone();
        let http_clone = app_state.http.clone();

        join_set.spawn(async move {
            let clone = contexts_clone.clone();

            match executor.execute(clone, llm_clients_clone).await {
                Ok((choice, dumps)) => {
                    if let Some(ref content) = choice.message.content {
                        contexts_clone.insert(
                            task_id.clone(),
                            Context {
                                task_id: task_id.clone(),
                                agent_type: executor.agent_type,
                                content: content.clone(),
                            },
                        );

                        let mut message = message_mutex_clone.lock().await;

                        if let Some(original_embed) = message.embeds.first()
                            && let Some(ref original_desc) = original_embed.description
                        {
                            let mut new_embed = original_embed.clone();
                            new_embed.description = Some(format!(
                                "{}\n✅ {} completed.",
                                original_desc,
                                task_id.clone()
                            ));

                            let edit_message_args =
                                EditMessage::new().embed(CreateEmbed::from(new_embed));

                            let new_message = http_clone
                                .edit_message(
                                    message.channel_id,
                                    message.id,
                                    &edit_message_args,
                                    vec![],
                                )
                                .await
                                .expect("Failed to update message.");

                            *message = new_message;
                        }
                    }

                    let generation_dumps = {
                        let dumps_lock = dumps.lock().await;
                        dumps_lock.clone()
                    };

                    (
                        choice.message.content.map(|s| Context {
                            task_id,
                            agent_type: executor.agent_type,
                            content: s,
                        }),
                        generation_dumps,
                    )
                }
                Err(e) => {
                    let error_msg = format!(
                        "Failed to get a response from agent {}: {:?}",
                        executor.agent_type, e
                    );
                    tracing::error!("{}", &error_msg);
                    (None, vec![])
                }
            }
        });

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let results = join_set.join_all().await;

    let mut dumps = results
        .iter()
        .map(|(_ctx, d)| (*d).clone())
        .flatten()
        .collect::<Vec<_>>();

    plan_record.dumps.append(&mut dumps);

    let results = results
        .into_iter()
        .map(|(ctx, _d)| ctx)
        .flatten()
        .collect::<Vec<_>>();

    Ok((Some(message_mutex), results))
}

fn create_executors(tasks: &[Task], language: Language, app_state: &AppState) -> Vec<Executor> {
    let prompt_map = build_prompt_map(app_state);

    tasks
        .iter()
        .map(|task| Executor {
            task_id: task.task_id.clone(),
            system_prompt: prompt_map[&language][&task.agent].0.clone(),
            user_prompt: prompt_map[&language][&task.agent]
                .1
                .replace("$INSTRUCTION", &task.instruction),
            agent_type: task.agent,
            agent_prompt: prompt_map[&language][&task.agent].2.clone(),
            dependencies: task.dependencies.clone(),
        })
        .collect()
}

fn build_prompt_map(
    app_state: &AppState,
) -> HashMap<Language, HashMap<Agent, (String, String, String)>> {
    let languages = [Language::Chinese, Language::Japanese, Language::English];
    let agent_types = [
        Agent::Food,
        Agent::History,
        Agent::Modern,
        Agent::Nature,
        Agent::Transport,
    ];

    let language_map = [
        (Language::Chinese, &app_state.config.chinese),
        (Language::Japanese, &app_state.config.japanese),
        (Language::English, &app_state.config.english),
    ]
    .into_iter()
    .collect::<HashMap<_, _>>();

    let mut prompt_map = HashMap::new();

    for language in languages.into_iter() {
        let entry = prompt_map.entry(language).or_insert(HashMap::new());

        for agent in agent_types.into_iter() {
            match agent {
                Agent::Food => {
                    entry.insert(agent, &language_map[&language].food);
                }
                Agent::Transport => {
                    entry.insert(agent, &language_map[&language].transport);
                }
                Agent::History => {
                    entry.insert(agent, &language_map[&language].history);
                }
                Agent::Modern => {
                    entry.insert(agent, &language_map[&language].modern);
                }
                Agent::Nature => {
                    entry.insert(agent, &language_map[&language].nature);
                }
            }
        }
    }

    prompt_map
        .into_iter()
        .map(|(k, v)| {
            let new_v = v
                .into_iter()
                .map(|(inner_k, inner_v)| {
                    (
                        inner_k,
                        (
                            inner_v.system_prompt.clone(),
                            inner_v.user_prompt.clone(),
                            language_map[&k].agent.prompt.clone(),
                        ),
                    )
                })
                .collect::<HashMap<_, _>>();

            (k, new_v)
        })
        .collect()
}

async fn synthesize(
    language: Language,
    results: Vec<Context>,
    plan_record: &mut PlanRecord,
    app_state: &AppState,
) -> anyhow::Result<String> {
    let synthesis_prompt = match language {
        Language::Chinese => app_state.config.chinese.synthesis.prompt.clone(),
        Language::Japanese => app_state.config.japanese.synthesis.prompt.clone(),
        _ => app_state.config.english.synthesis.prompt.clone(),
    };

    let results = results
        .into_iter()
        .map(|c| (c.task_id.clone(), c))
        .collect::<HashMap<_, _>>();

    let synthesis_prompt =
        synthesis_prompt.replace("$RESULTS", &serde_json::to_string_pretty(&results)?);

    tracing::debug!("Synthesis prompt: {:?}", &synthesis_prompt);

    let mut messages = plan_record
        .messages
        .iter()
        .map(|m| {
            m.to_openai_message()
                .expect("Failed to convert plan record to OpenAI message.")
        })
        .collect::<Vec<_>>();

    messages.push(ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
            .content(synthesis_prompt.as_str())
            .build()?,
    ));

    plan_record.messages.push(RecordMessage {
        role: Role::User,
        content: Content::Plain(synthesis_prompt.clone()),
    });

    let request = CreateChatCompletionRequestArgs::default()
        .model(GEMINI_25_PRO)
        .temperature(TEMPERATURE_LOW)
        .messages(messages)
        .response_format(ResponseFormat::JsonSchema { json_schema: ResponseFormatJsonSchema {
            description: Some("Synthesize the results of subtasks into the final response.".into()),
            name: "synthesize_tasks".into(),
            schema: Some(json!({
                "type": "object",
                "properties": {
                    "final_result": {
                        "type": "string",
                        "description": "The combined and synthesized result to respond to the user's request."
                    }
                },
                "required": ["final_result"],
                "additionalProperties": false
            })),
            strict: Some(true) } })
        .build()?;

    let response = app_state
        .llm_clients
        .open_router_clients
        .get(&Agent::default())
        .expect("Failed to get the Open Router client for synthesis.")
        .chat()
        .create(request)
        .await;

    match response {
        Ok(res) => {
            let content = res.choices[0].message.content.clone().unwrap_or_default();
            let final_result = serde_json::from_str::<FinalResult>(&content)?;

            plan_record.messages.push(RecordMessage {
                role: Role::Assistant,
                content: Content::Dynamic(serde_json::to_value(&final_result)?),
            });

            plan_record.dumps.push(GenerationDump {
                model: LanguageModel::Gemini25Pro,
                content: final_result.to_string(),
            });

            tracing::info!("Final result: {:?}", &final_result);
            Ok(final_result.final_result)
        }
        Err(e) => {
            let error_msg = format!("Failed to get final result via API: {:?}", &e);
            tracing::error!("{}", &error_msg);
            Err(anyhow::anyhow!("{}", error_msg))
        }
    }
}

async fn insert_record(
    plan_record: PlanRecord,
    thread_id: ChannelId,
    app_state: &AppState,
) -> anyhow::Result<()> {
    let record_id = plan_record.id.to_string();

    let result = app_state
        .firestore_db
        .fluent()
        .insert()
        .into(PLAN_COLLECTION_NAME)
        .document_id(record_id.as_str())
        .object(&plan_record)
        .execute::<PlanRecord>()
        .await;

    if let Err(e) = result {
        let error_msg = format!("Failed to create document in Firestore: {e:?}");
        tracing::error!("{}", &error_msg);
        return Err(anyhow::anyhow!("{}", error_msg));
    }

    let mapping = PlanMapping {
        plan_id: plan_record.id,
        thread_id,
    };

    let result = app_state
        .firestore_db
        .fluent()
        .insert()
        .into(PLAN_MAPPING_COLLECTION_NAME)
        .document_id(record_id.as_str())
        .object(&mapping)
        .execute::<PlanMapping>()
        .await;

    if let Err(e) = result {
        let error_msg = format!("Failed to create plan mapping in Firestore: {e:?}");
        tracing::error!("{}", &error_msg);
        return Err(anyhow::anyhow!("{}", error_msg));
    }

    Ok(())
}

async fn send_final_result_message(
    mut final_result: String,
    thread_id: ChannelId,
    app_state: &AppState,
) -> anyhow::Result<()> {
    let mut character_count = final_result.chars().count();
    let messages = if character_count > 1000 {
        let mut container = vec![];

        while character_count > 0 {
            if character_count >= 1000 {
                let drained = final_result.chars().take(1000).collect::<String>();
                container.push(drained);
                final_result = final_result.chars().skip(1000).collect();
                character_count = final_result.chars().count();
            } else {
                container.push(final_result.clone());
                character_count = 0;
            }
        }

        container
    } else {
        vec![final_result]
    };

    for message in messages.into_iter() {
        let message_args = CreateMessage::new().content(message);

        let _ = app_state
            .http
            .send_message(thread_id, vec![], &message_args)
            .await?;
    }

    Ok(())
}
