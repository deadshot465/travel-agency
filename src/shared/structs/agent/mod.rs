use std::{collections::HashMap, fmt::Display, sync::Arc};

use async_openai::types::{
    ChatChoice, ChatCompletionRequestMessage, ChatCompletionRequestProvider, ChatCompletionTool,
    ChatCompletionToolChoiceOption, CreateChatCompletionRequest, CreateChatCompletionRequestArgs,
    CreateChatCompletionResponse,
};
use async_trait::async_trait;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task::JoinSet};

use crate::shared::{
    CHAT_GPT_4O_LATEST, DEEP_SEEK_R1, DEEP_SEEK_V3, DOUBAO_SEED_16, ERNIE_45_300B_A47B,
    GEMINI_25_PRO, GLM_4_PLUS, GPT_41, GROK_3, GROK_4, KIMI_K2, KIMI_LATEST, MAX_TOOL_RETRY_COUNT,
    MISTRAL_LARGE, O3, OPUS_4, QWEN_3_235B_A22B, QWEN_MAX, SONNET_4, TEMPERATURE_HIGH,
    TEMPERATURE_LOW, TEMPERATURE_MEDIUM,
    structs::{LLMClients, agent::record::GenerationDump},
    utility::build_one_shot_messages,
};

pub mod record;

pub type TaskId = String;

pub const DEFAULT_SUBTASK_TIMEOUT: u64 = 60 * 10;

pub static MODEL_NAME_MAP: Lazy<DashMap<LanguageModel, String>> = Lazy::new(|| {
    [
        (LanguageModel::ChatGPT4o, CHAT_GPT_4O_LATEST.into()),
        (LanguageModel::GPT41, GPT_41.into()),
        (LanguageModel::O3, O3.into()),
        (LanguageModel::Sonnet4, SONNET_4.into()),
        (LanguageModel::Opus4, OPUS_4.into()),
        (LanguageModel::Gemini25Pro, GEMINI_25_PRO.into()),
        (LanguageModel::Grok3, GROK_3.into()),
        (LanguageModel::Grok4, GROK_4.into()),
        (LanguageModel::DeepSeekV3, DEEP_SEEK_V3.into()),
        (LanguageModel::DeepSeekR1, DEEP_SEEK_R1.into()),
        (LanguageModel::GLM4Plus, GLM_4_PLUS.into()),
        // (LanguageModel::Step216k, STEP_2_16K.into()),
        (LanguageModel::QwenMax, QWEN_MAX.into()),
        (LanguageModel::Qwen3235BA22B, QWEN_3_235B_A22B.into()),
        (LanguageModel::DoubaoSeed16, DOUBAO_SEED_16.into()),
        (LanguageModel::KimiLatest, KIMI_LATEST.into()),
        (LanguageModel::KimiK2, KIMI_K2.into()),
        (LanguageModel::MistralLarge, MISTRAL_LARGE.into()),
        // (LanguageModel::MiniMaxM1, MINIMAX_M1.into()),
        (LanguageModel::Ernie45300BA47B, ERNIE_45_300B_A47B.into()),
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
    ) -> anyhow::Result<(ChatChoice, Arc<Mutex<Vec<GenerationDump>>>)>;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum LanguageModel {
    // OpenAI
    ChatGPT4o,
    GPT41,
    O3,
    // Anthropic
    Sonnet4,
    Opus4,
    // Google
    Gemini25Pro,
    // xAI
    Grok3,
    Grok4,
    // DeepSeek
    DeepSeekV3,
    DeepSeekR1,
    // Zhipu
    GLM4Plus,
    // StepFun
    Step216k,
    // Qwen
    QwenMax,
    Qwen3235BA22B,
    // Doubao
    DoubaoSeed16,
    // Kimi
    KimiLatest,
    KimiK2,
    // Mistral
    MistralLarge,
    // MiniMax
    MiniMaxM1,
    // Baidu Ernie
    Ernie45300BA47B,
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
    pub transport_agent: Option<String>,
    pub transport_agent_maximum_try: Option<String>,
    pub get_transit_time_tool: Option<ChatCompletionTool>,
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

impl Display for OrchestrationPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(self).unwrap_or_default()
        )
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(self).unwrap_or_default()
        )
    }
}

impl Display for FinalResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(self).unwrap_or_default()
        )
    }
}

#[async_trait]
impl Taskable for Executor {
    async fn execute(
        &mut self,
        contexts: Arc<DashMap<TaskId, Context>>,
        llm_clients: Arc<LLMClients>,
    ) -> anyhow::Result<(ChatChoice, Arc<Mutex<Vec<GenerationDump>>>)> {
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

        let generation_dumps = Arc::new(Mutex::new(Vec::new()));

        for entry in MODEL_NAME_MAP.iter() {
            let (model, model_name) = (*entry.key(), entry.value().clone());
            let request = build_llm_request(model, model_name.clone(), messages.clone())?;
            let llm_clients_clone = llm_clients.clone();
            let agent_type = self.agent_type;
            let dumps = generation_dumps.clone();

            join_set.spawn(async move {
                let open_router_client = llm_clients_clone
                    .open_router_clients
                    .get(&agent_type)
                    .expect("Failed to get the Open Router client for the agent.");

                let result = match model {
                    m if m == LanguageModel::ChatGPT4o || m == LanguageModel::GPT41 => {
                        let chat = llm_clients_clone.openai_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                    LanguageModel::DoubaoSeed16 => {
                        let chat = llm_clients_clone.volc_engine_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                    LanguageModel::GLM4Plus => {
                        let chat = llm_clients_clone.zhipu_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                    LanguageModel::KimiLatest => {
                        let chat = llm_clients_clone.moonshot_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                    LanguageModel::Step216k => {
                        let chat = llm_clients_clone.step_fun_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                    m if m == LanguageModel::DeepSeekV3 || m == LanguageModel::DeepSeekR1 => {
                        let chat = llm_clients_clone.deepseek_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                    _ => {
                        let chat = open_router_client.chat();
                        let future = chat.create(request);
                        tokio::time::timeout(std::time::Duration::from_secs(DEFAULT_SUBTASK_TIMEOUT), future).await
                    }
                };

                match result {
                    Ok(res) => match res {
                        Ok(r) => {
                            tracing::info!("{model} has completed a {agent_type} task.");
                            let extracted = extract_response_content(r);

                            {
                                let mut dumps_lock = dumps.lock().await;
                                dumps_lock.push(GenerationDump { model, content: extracted.clone() });
                            }

                            (model, extracted)
                        },
                        Err(e) => {
                            let error_msg = format!("Failed to get response from model {model} when trying to complete a {agent_type} task: {e:?}");
                            tracing::error!("{}", &error_msg);
                            (model, error_msg)
                        }
                    },
                    Err(_e) => {
                        let error_msg = format!("{model} timed out when trying to complete a {agent_type} task.");
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

        let transport_agent_prompt = if let Some(ref p) = self.transport_agent {
            p.replace("$RETRY_COUNT", &MAX_TOOL_RETRY_COUNT.to_string())
                .replace("$MAXIMUM_RETRY_REACHED", "")
                .trim()
                .to_string()
        } else {
            "".into()
        };

        self.user_prompt = self.user_prompt.replace(
            "$AGENT",
            self.agent_prompt
                .replace("$RESULTS", &results_dump)
                .replace("$AGENT_TRANSPORT", &transport_agent_prompt)
                .trim(),
        );

        tracing::info!("Agent system prompt: {}", &self.system_prompt);
        tracing::info!("Agent user prompt: {}", &self.user_prompt);

        let messages = build_one_shot_messages(&self.system_prompt, &self.user_prompt)?;

        let mut request = CreateChatCompletionRequestArgs::default();
        request
            .model(SONNET_4)
            .temperature(TEMPERATURE_MEDIUM)
            .messages(messages);

        if self.agent_type == Agent::Transport
            && let Some(ref tool) = self.get_transit_time_tool
        {
            request
                .tools(vec![tool.clone()])
                .tool_choice(ChatCompletionToolChoiceOption::Required);
        }

        llm_clients
            .open_router_clients
            .get(&self.agent_type)
            .expect("Failed to get the Open Router client for the agent.")
            .chat()
            .create(request.build()?)
            .await
            .map_err(|e| anyhow::anyhow!("{e:?}"))
            .and_then(|res| {
                res.choices
                    .first()
                    .cloned()
                    .map(|c| (c, generation_dumps))
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
