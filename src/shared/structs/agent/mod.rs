use std::{collections::HashMap, fmt::Display, sync::Arc};

use async_openai::types::{
    ChatChoice, ChatCompletionRequestMessage, ChatCompletionRequestProvider,
    CreateChatCompletionRequest, CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
    ReasoningEffort,
    responses::{Content, CreateResponseArgs, Input, OutputContent, ReasoningConfigArgs},
};
use async_trait::async_trait;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use crate::shared::{
    CHAT_GPT_4O_LATEST, DEEP_SEEK_R1, DEEP_SEEK_V3, DOUBAO_SEED_16, GEMINI_25_PRO, GLM_4_PLUS,
    GPT_41, GROK_3, KIMI_LATEST, MINIMAX_M1, MISTRAL_LARGE, O3_PRO, OPUS_4, QWEN_MAX, SONNET_4,
    STEP_2_16K, TEMPERATURE_HIGH, TEMPERATURE_LOW, TEMPERATURE_MEDIUM, structs::LLMClients,
    utility::build_one_shot_messages,
};

pub mod record;

pub type TaskId = String;

pub static MODEL_NAME_MAP: Lazy<DashMap<LanguageModel, String>> = Lazy::new(|| {
    [
        (LanguageModel::ChatGPT4o, CHAT_GPT_4O_LATEST.into()),
        (LanguageModel::GPT41, GPT_41.into()),
        (LanguageModel::O3Pro, O3_PRO.into()),
        (LanguageModel::Sonnet4, SONNET_4.into()),
        (LanguageModel::Opus4, OPUS_4.into()),
        (LanguageModel::Gemini25Pro, GEMINI_25_PRO.into()),
        (LanguageModel::Grok3, GROK_3.into()),
        (LanguageModel::DeepSeekV3, DEEP_SEEK_V3.into()),
        (LanguageModel::DeepSeekR1, DEEP_SEEK_R1.into()),
        (LanguageModel::GLM4Plus, GLM_4_PLUS.into()),
        (LanguageModel::Step216k, STEP_2_16K.into()),
        (LanguageModel::QwenMax, QWEN_MAX.into()),
        (LanguageModel::DoubaoSeed16, DOUBAO_SEED_16.into()),
        (LanguageModel::KimiLatest, KIMI_LATEST.into()),
        (LanguageModel::MistralLarge, MISTRAL_LARGE.into()),
        (LanguageModel::MiniMaxM1, MINIMAX_M1.into()),
    ]
    .into_iter()
    .collect::<DashMap<_, _>>()
});

#[async_trait]
pub trait Taskable {
    async fn execute(
        &mut self,
        contexts: Arc<DashMap<TaskId, Context>>,
        llm_clients: Arc<LLMClients>,
    ) -> anyhow::Result<ChatChoice>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
pub enum Agent {
    #[default]
    Food,
    Transport,
    History,
    Modern,
    Nature,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    Japanese,
    Chinese,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageModel {
    // OpenAI
    ChatGPT4o,
    GPT41,
    O3Pro,
    // Anthropic
    Sonnet4,
    Opus4,
    // Google
    Gemini25Pro,
    // xAI
    Grok3,
    // DeepSeek
    DeepSeekV3,
    DeepSeekR1,
    // Zhipu
    GLM4Plus,
    // StepFun
    Step216k,
    // Qwen
    QwenMax,
    // Doubao
    DoubaoSeed16,
    // Kimi
    KimiLatest,
    // Mistral
    MistralLarge,
    // MiniMax
    MiniMaxM1,
}

#[derive(Deserialize, Serialize)]
pub struct LanguageTriageArguments {
    pub language: Language,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OrchestrationPlan {
    pub analysis: String,
    pub greeting_message: String,
    pub synthesis_plan: String,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Task {
    pub task_id: String,
    pub agent: Agent,
    pub dependencies: Vec<TaskId>,
    pub instruction: String,
}

pub struct Executor {
    pub task_id: TaskId,
    pub system_prompt: String,
    pub user_prompt: String,
    pub agent_type: Agent,
    pub agent_prompt: String,
    pub dependencies: Vec<TaskId>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Context {
    pub task_id: TaskId,
    pub agent_type: Agent,
    pub content: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FinalResult {
    pub final_result: String,
}

impl Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            Agent::Food => "Food",
            Agent::Transport => "Transport",
            Agent::History => "History",
            Agent::Modern => "Modern",
            Agent::Nature => "Nature",
        };

        write!(f, "{string}")
    }
}

impl Display for LanguageModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let model_name = if let Some(name) = MODEL_NAME_MAP.get(self) {
            name.value().clone()
        } else {
            Default::default()
        };

        write!(f, "{model_name}")
    }
}

#[async_trait]
impl Taskable for Executor {
    async fn execute(
        &mut self,
        contexts: Arc<DashMap<TaskId, Context>>,
        llm_clients: Arc<LLMClients>,
    ) -> anyhow::Result<ChatChoice> {
        let dependencies = self.dependencies.clone();

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let context_keys = contexts
                .iter()
                .map(|entry| entry.key().clone())
                .collect::<Vec<_>>();

            if dependencies
                .iter()
                .all(|task_id| context_keys.contains(task_id))
            {
                break;
            }
        }

        let context = contexts
            .iter()
            .filter(|c| dependencies.contains(c.key()))
            .map(|c| (c.key().clone(), c.value().content.clone()))
            .collect::<HashMap<_, _>>();

        let context = if !context.is_empty() {
            serde_json::to_string_pretty(&context)?
        } else {
            "".into()
        };

        self.user_prompt = self.user_prompt.replace("$CONTEXT", &context);

        let subtask_user_prompt = self.user_prompt.replace("$AGENT", "");
        let messages = build_one_shot_messages(&self.system_prompt, &subtask_user_prompt)?;
        let mut join_set = JoinSet::new();

        tracing::debug!("Subtask user prompt: {}", &subtask_user_prompt);

        for entry in MODEL_NAME_MAP.iter() {
            let (model, model_name) = (*entry.key(), entry.value().clone());
            let request = build_llm_request(model, model_name.clone(), messages.clone())?;
            let llm_clients_clone = llm_clients.clone();
            let system_prompt_clone = self.system_prompt.clone();
            let user_prompt_clone = subtask_user_prompt.clone();
            let agent_type = self.agent_type;

            join_set.spawn(async move {
                let result = match model {
                    m if m == LanguageModel::ChatGPT4o || m == LanguageModel::GPT41 => {
                        llm_clients_clone
                            .openai_client
                            .chat()
                            .create(request)
                            .await
                            .map(extract_response_content)
                    }
                    LanguageModel::DoubaoSeed16 => llm_clients_clone
                        .volc_engine_client
                        .chat()
                        .create(request)
                        .await
                        .map(extract_response_content),
                    LanguageModel::GLM4Plus => llm_clients_clone
                        .zhipu_client
                        .chat()
                        .create(request)
                        .await
                        .map(extract_response_content),
                    LanguageModel::KimiLatest => llm_clients_clone
                        .moonshot_client
                        .chat()
                        .create(request)
                        .await
                        .map(extract_response_content),
                    LanguageModel::Step216k => llm_clients_clone
                        .step_fun_client
                        .chat()
                        .create(request)
                        .await
                        .map(extract_response_content),
                    m if m == LanguageModel::DeepSeekV3 || m == LanguageModel::DeepSeekR1 => {
                        llm_clients_clone
                            .deepseek_client
                            .chat()
                            .create(request)
                            .await
                            .map(extract_response_content)
                    }
                    LanguageModel::O3Pro => {
                        let mut args = CreateResponseArgs::default();

                        let mut reasoning_config_args = ReasoningConfigArgs::default();
                        let reasoning_config = reasoning_config_args
                            .effort(ReasoningEffort::High)
                            .build()
                            .expect("Failed to build reasoning config.");

                        let request = args
                            .model(O3_PRO)
                            .instructions(system_prompt_clone)
                            .temperature(TEMPERATURE_HIGH)
                            .input(Input::Text(user_prompt_clone))
                            .reasoning(reasoning_config)
                            .build()
                            .expect("Failed to create response.");

                        llm_clients_clone
                            .openai_client
                            .responses()
                            .create(request)
                            .await
                            .map(|res| {
                                if let Some(OutputContent::Message(m)) = res.output.first()
                                    && let Some(output) = m.content.first()
                                    && let Content::OutputText(text) = output
                                {
                                    text.text.clone()
                                } else {
                                    String::new()
                                }
                            })
                    }
                    _ => llm_clients_clone
                        .open_router_clients
                        .get(&agent_type)
                        .expect("Failed to get the Open Router client for the agent.")
                        .chat()
                        .create(request)
                        .await
                        .map(extract_response_content),
                };

                match result {
                    Ok(res) => (model, res),
                    Err(e) => {
                        let error_msg = format!("Failed to get response from model {model}: {e:?}");
                        tracing::error!("{}", &error_msg);
                        (model, error_msg)
                    }
                }
            });
        }

        let results = join_set.join_all().await;
        tracing::info!("Results: {:?}", &results);

        let results = results
            .into_iter()
            .map(|(_, result)| result)
            .filter(|s| !s.starts_with("Fail"))
            .collect::<Vec<_>>();

        let results_dump = serde_json::to_string_pretty(&results)?;

        self.user_prompt = self.user_prompt.replace(
            "$AGENT",
            &self.agent_prompt.replace("$RESULTS", &results_dump),
        );

        tracing::info!("Agent system prompt: {}", &self.system_prompt);
        tracing::info!("Agent user prompt: {}", &self.user_prompt);

        let messages = build_one_shot_messages(&self.system_prompt, &self.user_prompt)?;

        let request = CreateChatCompletionRequestArgs::default()
            .model(SONNET_4)
            .temperature(TEMPERATURE_MEDIUM)
            .messages(messages)
            .build()?;

        llm_clients
            .open_router_clients
            .get(&self.agent_type)
            .expect("Failed to get the Open Router client for the agent.")
            .chat()
            .create(request)
            .await
            .map_err(|e| anyhow::anyhow!("{:?}", e))
            .and_then(|res| {
                res.choices
                    .first()
                    .cloned()
                    .ok_or(anyhow::anyhow!("Failed to generate a response from model."))
            })
    }
}

fn build_llm_request(
    model: LanguageModel,
    model_name: String,
    messages: Vec<ChatCompletionRequestMessage>,
) -> anyhow::Result<CreateChatCompletionRequest> {
    let temperature = match model {
        LanguageModel::KimiLatest => TEMPERATURE_LOW,
        LanguageModel::DeepSeekV3 => 1.8,
        _ => TEMPERATURE_HIGH,
    };

    let top_p = match model {
        LanguageModel::DeepSeekV3 => 0.98,
        _ => 1.0,
    };

    let mut args = CreateChatCompletionRequestArgs::default();
    args.messages(messages)
        .model(model_name)
        .temperature(temperature)
        .top_p(top_p);

    let request = match model {
        m if m == LanguageModel::DeepSeekV3 || m == LanguageModel::DeepSeekR1 => args
            .provider(ChatCompletionRequestProvider {
                order: vec!["DeepSeek".into()],
                allow_fallbacks: false,
            })
            .build()?,
        _ => args.build()?,
    };

    Ok(request)
}

fn extract_response_content(response: CreateChatCompletionResponse) -> String {
    response.choices[0]
        .message
        .content
        .clone()
        .unwrap_or_default()
}
