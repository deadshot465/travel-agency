use async_openai::types::{
    ChatChoice, ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessageArgs,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessageArgs,
    ChatCompletionTool, ChatCompletionToolArgs, ChatCompletionToolChoiceOption,
    ChatCompletionToolType, CreateChatCompletionRequestArgs, FinishReason, FunctionObjectArgs,
    ResponseFormat, ResponseFormatJsonSchema, Role,
};
use command_macros::command_handler;
use dashmap::DashMap;
use serde_json::json;
use serenity::all::{
    ChannelId, CommandInteraction, CreateEmbed, CreateEmbedAuthor, CreateMessage, CreateThread,
    EditInteractionResponse, EditMessage, GuildChannel, Message,
};
use std::collections::HashMap;
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
use crate::shared::structs::google_maps::{RouteWithDuration, TransferPlan};
use crate::shared::utility::google_maps::{get_latitude_and_longitude, get_travel_time};
use crate::shared::utility::{build_one_shot_messages, create_avatar_url};
use crate::shared::{
    EMBED_COLOR, GEMINI_25_FLASH, GEMINI_25_PRO, GPT_41, MAX_TOOL_RETRY_COUNT,
    PLAN_COLLECTION_NAME, PLAN_MAPPING_COLLECTION_NAME, TEMPERATURE_LOW, TEMPERATURE_MEDIUM,
};

type PromptMap = HashMap<Language, HashMap<Agent, PromptSet>>;

#[derive(Debug, Clone)]
struct PromptSet {
    pub system: String,
    pub user: String,
    pub agent: String,
    pub transport_agent: String,
    pub transport_agent_maximum_retry: String,
}

#[command_handler]
pub async fn plan(interaction: CommandInteraction, app_state: AppState) -> anyhow::Result<()> {
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
        language,
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
            ..Default::default()
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

        insert_record(plan_record, edited_message, thread.id, &app_state).await?;

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
                                    "description": "List of task IDs that must complete before this task. All task IDs in this list have to be `task_id`s of other tasks in the `tasks` and **must not** include your own tasks.",
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

    loop {
        let request_clone = request.clone();

        let response = app_state
            .llm_clients
            .open_router_clients
            .get(&Agent::default())
            .expect("Failed to get the Open Router client for orchestration.")
            .chat()
            .create(request_clone)
            .await;

        match response {
            Ok(res) => {
                let content = res.choices[0].message.content.clone().unwrap_or_default();
                let orchestration_plan = serde_json::from_str::<OrchestrationPlan>(&content)?;

                let mut all_dependencies = orchestration_plan
                    .tasks
                    .iter()
                    .flat_map(|t| t.dependencies.clone())
                    .collect::<Vec<_>>();

                all_dependencies.sort();
                all_dependencies.dedup();

                let all_task_ids = orchestration_plan
                    .tasks
                    .iter()
                    .map(|t| t.task_id.clone())
                    .collect::<Vec<_>>();

                if all_dependencies
                    .into_iter()
                    .all(|dep| all_task_ids.contains(&dep))
                {
                    tracing::info!("Orchestration response: {:?}", &orchestration_plan);
                    return Ok(orchestration_plan);
                }
            }
            Err(e) => {
                let error_msg = format!("Error when creating orchestration tasks: {e:?}");
                tracing::error!("{}", &error_msg);
                return Err(anyhow::anyhow!("{}", error_msg));
            }
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
        let google_maps_client_clone = app_state.google_maps_client.clone();

        join_set.spawn(async move {
            let clone = contexts_clone.clone();

            match executor.execute(clone, llm_clients_clone.clone()).await {
                Ok((choice, dumps)) => {
                    if choice.message.content.is_some() {
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

                    let context = match executor.agent_type {
                        Agent::Transport => {
                            if let Some(reason) = choice.finish_reason
                                && reason == FinishReason::ToolCalls
                                && let Some(mut tool_call) = choice
                                    .message
                                    .tool_calls
                                    .as_ref()
                                    .and_then(|v| v.first().cloned())
                            {
                                let mut completed_context = None;

                                let mut assistant_message = choice.message.clone();

                                let maximum_try_prompt = executor.transport_agent_maximum_try.clone().unwrap_or_default();

                                let mut retry_count = 0;
                                loop {
                                    if retry_count >= MAX_TOOL_RETRY_COUNT {
                                        break;
                                    }

                                    let user_prompt = executor
                                        .user_prompt
                                        .replace("$RETRY_COUNT", &retry_count.to_string())
                                        .replace("$MAXIMUM_RETRY_REACHED", if retry_count == MAX_TOOL_RETRY_COUNT - 1 {
                                            &maximum_try_prompt
                                        } else {
                                            ""
                                        })
                                        .trim()
                                        .to_string();

                                    tracing::info!("Retry system prompt: {}", &executor.system_prompt);
                                    tracing::info!("Retry user prompt: {user_prompt}");

                                    let mut message_histories = build_one_shot_messages(
                                        &executor.system_prompt, &user_prompt)
                                        .expect("Failed to build one-shot message with system prompt and user prompt.");

                                    let tool_call_id = assistant_message
                                        .tool_calls
                                        .as_ref()
                                        .and_then(|v| v.first())
                                        .map(|call| call.id.clone())
                                        .unwrap_or_default();

                                    message_histories.push(ChatCompletionRequestMessage::Assistant(
                                        ChatCompletionRequestAssistantMessageArgs::default()
                                            .content(assistant_message.content.clone().unwrap_or_default())
                                            .tool_calls(assistant_message.tool_calls.clone().unwrap_or_default())
                                            .build()
                                            .expect("Failed to add assistant message to message histories.")));

                                    let mut tool_call_failed = false;
                                    let results = handle_tool_call(
                                        tool_call.clone(),
                                        language,
                                        google_maps_client_clone.clone(),
                                    )
                                    .await
                                    .map_err(|e| {
                                        tracing::error!("Failed to handle tool call: {e:?}");
                                        tool_call_failed = true;
                                    })
                                    .unwrap_or_default();

                                    if tool_call_failed {
                                        retry_count += 1;
                                        continue;
                                    }

                                    let last_message = build_transport_agent_final_message(
                                        &mut message_histories,
                                        tool_call_id.clone(),
                                        results,
                                        executor.get_transit_time_tool.clone(),
                                        llm_clients_clone.clone(),
                                    )
                                    .await
                                    .expect("Failed to build final message for transport agent.");

                                    if let Some(reason) = last_message.finish_reason
                                        && reason != FinishReason::ToolCalls
                                    {
                                        completed_context =
                                            last_message.message.content.map(|s| {
                                                let ctx = Context {
                                                    task_id: task_id.clone(),
                                                    agent_type: executor.agent_type,
                                                    content: s,
                                                };

                                                contexts_clone.insert(task_id, ctx.clone());

                                                ctx
                                            });

                                        break;
                                    } else {
                                        tool_call = last_message
                                            .message
                                            .tool_calls
                                            .as_ref()
                                            .and_then(|v| v.first().cloned())
                                            .expect("Failed to extract tool call from response.");

                                        assistant_message = last_message.message.clone();
                                        retry_count += 1;
                                    }
                                }

                                completed_context
                            } else {
                                choice.message.content.map(|s| {
                                    let ctx = Context {
                                        task_id: task_id.clone(),
                                        agent_type: executor.agent_type,
                                        content: s,
                                    };

                                    contexts_clone.insert(task_id, ctx.clone());

                                    ctx
                                })
                            }
                        }
                        _ => choice.message.content.map(|s| {
                            let ctx = Context {
                                task_id: task_id.clone(),
                                agent_type: executor.agent_type,
                                content: s,
                            };

                            contexts_clone.insert(task_id, ctx.clone());

                            ctx
                        }),
                    };

                    (context, generation_dumps)
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
        .flat_map(|(_ctx, d)| (*d).clone())
        .collect::<Vec<_>>();

    plan_record.dumps.append(&mut dumps);

    let results = results
        .into_iter()
        .filter_map(|(ctx, _d)| ctx)
        .collect::<Vec<_>>();

    Ok((Some(message_mutex), results))
}

fn create_executors(tasks: &[Task], language: Language, app_state: &AppState) -> Vec<Executor> {
    let prompt_map = build_prompt_map(app_state);

    tasks
        .iter()
        .map(|task| Executor {
            task_id: task.task_id.clone(),
            system_prompt: prompt_map[&language][&task.agent].system.clone(),
            user_prompt: prompt_map[&language][&task.agent]
                .user
                .replace("$INSTRUCTION", &task.instruction),
            agent_type: task.agent,
            agent_prompt: prompt_map[&language][&task.agent].agent.clone(),
            dependencies: task.dependencies.clone(),
            transport_agent: match task.agent {
                Agent::Transport => {
                    Some(prompt_map[&language][&task.agent].transport_agent.clone())
                }
                _ => None,
            },
            transport_agent_maximum_try: match task.agent {
                Agent::Transport => Some(
                    prompt_map[&language][&task.agent]
                        .transport_agent_maximum_retry
                        .clone(),
                ),
                _ => None,
            },
            get_transit_time_tool: match task.agent {
                Agent::Transport => Some(
                    create_get_transit_time_tool()
                        .expect("Failed to create get_transit_time tool."),
                ),
                _ => None,
            },
        })
        .collect()
}

fn build_prompt_map(app_state: &AppState) -> PromptMap {
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
                        PromptSet {
                            system: inner_v.system_prompt.clone(),
                            user: inner_v.user_prompt.clone(),
                            agent: language_map[&k].agent.prompt.clone(),
                            transport_agent: language_map[&k].transport_agent.prompt.clone(),
                            transport_agent_maximum_retry: language_map[&k]
                                .transport_agent_maximum_try
                                .prompt
                                .clone(),
                        },
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
                is_final_result: true,
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
    original_message: Message,
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
        channel_id: original_message.channel_id.get().to_string(),
        original_message_id: original_message.id.get().to_string(),
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

async fn handle_tool_call(
    tool_call: ChatCompletionMessageToolCall,
    language: Language,
    google_maps_client: Arc<::google_maps::Client>,
) -> anyhow::Result<Vec<RouteWithDuration>> {
    let transfer_plan = serde_json::from_str::<TransferPlan>(&tool_call.function.arguments)?;

    tracing::debug!("Transfer Plan: {transfer_plan:?}");

    let mut routes = Vec::with_capacity(transfer_plan.routes.len());

    let lat_lngs = Arc::new(DashMap::new());

    for route in transfer_plan.routes.iter() {
        let (from, to) = get_latitude_and_longitude(
            route,
            language,
            lat_lngs.clone(),
            google_maps_client.clone(),
        )
        .await?;
        routes.push((from, to, route.by));
    }

    let routes = routes
        .into_iter()
        .zip(transfer_plan.routes.into_iter())
        .collect::<Vec<_>>();

    let mut results = Vec::with_capacity(routes.len());

    for (values, route) in routes.into_iter() {
        let (duration, alternative) =
            get_travel_time(values, language, google_maps_client.clone()).await?;
        results.push(RouteWithDuration {
            from: route.from,
            to: route.to,
            by: route.by,
            duration,
            alternative,
        });
    }

    tracing::debug!("Direction UI results: {results:?}");

    Ok(results)
}

async fn build_transport_agent_final_message(
    message_histories: &mut Vec<ChatCompletionRequestMessage>,
    tool_call_id: String,
    results: Vec<RouteWithDuration>,
    get_transit_time_tool: Option<ChatCompletionTool>,
    llm_clients: Arc<crate::shared::structs::LLMClients>,
) -> anyhow::Result<ChatChoice> {
    let results = serde_json::to_string_pretty(&results)?;

    message_histories.push(ChatCompletionRequestMessage::Tool(
        ChatCompletionRequestToolMessageArgs::default()
            .content(ChatCompletionRequestToolMessageContent::Text(results))
            .tool_call_id(tool_call_id)
            .build()?,
    ));

    tracing::debug!("Messages with tool result: {:?}", &message_histories[2..]);

    let mut request = CreateChatCompletionRequestArgs::default();
    request
        .model(GEMINI_25_PRO)
        .temperature(TEMPERATURE_MEDIUM)
        .messages(message_histories.clone());

    if let Some(tool) = get_transit_time_tool {
        request.tools(vec![tool]);
    }

    let client = &*llm_clients
        .open_router_clients
        .get(&Agent::Transport)
        .expect("Failed to get open router client for transport agent.");

    let response = client.chat().create(request.build()?).await?;

    response.choices.first().cloned().ok_or(anyhow::anyhow!(
        "Failed to generate final message for transport agent."
    ))
}

fn create_get_transit_time_tool() -> anyhow::Result<ChatCompletionTool> {
    Ok(ChatCompletionToolArgs::default()
                .r#type(ChatCompletionToolType::Function)
                .function(FunctionObjectArgs::default()
                    .name("get_transit_time")
                    .description("Get transit time needed to navigate from one place to another.")
                    .strict(true)
                    .parameters(json!({
                        "type": "object",
                        "properties": {
                            "routes": {
                                "type": "array",
                                "description": "A list of routes covered in the itinerary.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "from": {
                                            "type": "string",
                                            "description": "The origin or start point of a route. Make sure that it's a valid and correct place name."
                                        },
                                        "to": {
                                            "type": "string",
                                            "description": "The destination, goal, or end point of a route. Make sure that it's a valid and correct place name."
                                        },
                                        "by": {
                                            "type": "string",
                                            "description": "The preferred type of transit to take.",
                                            "enum": ["drive_or_taxi", "public_transport"]
                                        }
                                    },
                                    "required": ["from", "to", "by"],
                                    "additionalProperties": false
                                }
                            }
                        },
                        "required": ["routes"],
                        "additionalProperties": false
                    }))
                    .build()?)
                .build()?)
}
