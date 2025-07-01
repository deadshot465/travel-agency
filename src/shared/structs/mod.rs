use std::sync::Arc;

use async_openai::config::OpenAIConfig;
use dashmap::DashMap;
use serenity::all::Http;

use crate::shared::structs::{agent::Agent, config::Configuration};

pub mod agent;
pub mod config;
pub mod discord;

const OPEN_ROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const VOLC_ENGINE_BASE_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";
const MOONSHOT_BASE_URL: &str = "https://api.moonshot.cn/v1";
const STEP_FUN_BASE_URL: &str = "https://api.stepfun.com/v1";
const ZHIPU_BASE_URL: &str = "https://open.bigmodel.cn/api/paas/v4";
const DEEP_SEEK_BASE_URL: &str = "https://api.deepseek.com";

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Configuration,
    pub llm_clients: Arc<LLMClients>,
    pub http_client: reqwest::Client,
    pub http: Arc<Http>,
    pub firestore_db: firestore::FirestoreDb,
}

#[derive(Debug, Clone)]
pub struct LLMClients {
    pub open_router_clients: DashMap<Agent, async_openai::Client<OpenAIConfig>>,
    pub openai_client: async_openai::Client<OpenAIConfig>,
    pub volc_engine_client: async_openai::Client<OpenAIConfig>,
    pub moonshot_client: async_openai::Client<OpenAIConfig>,
    pub step_fun_client: async_openai::Client<OpenAIConfig>,
    pub zhipu_client: async_openai::Client<OpenAIConfig>,
    pub deepseek_client: async_openai::Client<OpenAIConfig>,
}

impl LLMClients {
    pub fn new() -> Self {
        let openai_config =
            OpenAIConfig::new().with_api_key(std::env::var("OPENAI_API_KEY").unwrap_or_default());
        let openai_client = async_openai::Client::with_config(openai_config);

        let open_router_clients = DashMap::new();
        let agents = [
            Agent::Food,
            Agent::History,
            Agent::Modern,
            Agent::Nature,
            Agent::Transport,
        ];

        for agent in agents.into_iter() {
            open_router_clients.insert(
                agent,
                Self::initialize_compatible_client(
                    OPEN_ROUTER_BASE_URL,
                    std::env::var("OPEN_ROUTER_API_KEY").unwrap_or_default(),
                ),
            );
        }

        LLMClients {
            open_router_clients,
            openai_client,
            volc_engine_client: Self::initialize_compatible_client(
                VOLC_ENGINE_BASE_URL,
                std::env::var("VOLC_ENGINE_API_KEY").unwrap_or_default(),
            ),
            moonshot_client: Self::initialize_compatible_client(
                MOONSHOT_BASE_URL,
                std::env::var("MOONSHOT_API_KEY").unwrap_or_default(),
            ),
            step_fun_client: Self::initialize_compatible_client(
                STEP_FUN_BASE_URL,
                std::env::var("STEP_FUN_API_KEY").unwrap_or_default(),
            ),
            zhipu_client: Self::initialize_compatible_client(
                ZHIPU_BASE_URL,
                std::env::var("ZHIPU_API_KEY").unwrap_or_default(),
            ),
            deepseek_client: Self::initialize_compatible_client(
                DEEP_SEEK_BASE_URL,
                std::env::var("DEEP_SEEK_API_KEY").unwrap_or_default(),
            ),
        }
    }

    fn initialize_compatible_client(
        base_url: &str,
        api_key: String,
    ) -> async_openai::Client<OpenAIConfig> {
        let config = OpenAIConfig::new()
            .with_api_base(base_url)
            .with_api_key(api_key);

        async_openai::Client::with_config(config)
    }
}
