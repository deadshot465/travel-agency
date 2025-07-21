use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub server_bind_point: String,
    pub server_address: String,
    pub log_level: String,
    pub language_triage_prompt: String,
    pub english: Language,
    pub chinese: Language,
    pub japanese: Language,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Language {
    pub orchestrator: Prompt,
    pub naming: Prompt,
    pub food: PromptPair,
    pub history: PromptPair,
    pub modern: PromptPair,
    pub nature: PromptPair,
    pub transport: PromptPair,
    pub agent: Prompt,
    pub synthesis: Prompt,
    pub transport_agent: Prompt,
    pub transport_agent_maximum_try: Prompt,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Prompt {
    pub prompt: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct PromptPair {
    pub system_prompt: String,
    pub user_prompt: String,
}

impl Configuration {
    pub fn new() -> Self {
        Configuration {
            server_bind_point: "0.0.0.0:80".into(),
            server_address: "http://localhost:80/".into(),
            log_level: "DEBUG".into(),
            language_triage_prompt: "".into(),
            english: Default::default(),
            chinese: Default::default(),
            japanese: Default::default(),
        }
    }

    pub fn load_from_config_file() -> anyhow::Result<Self> {
        let config_directory = Self::config_directory()?;

        if !config_directory.exists() {
            std::fs::create_dir_all(&config_directory)?;
        }

        let config_path = Self::config_path()?;
        if !config_path.exists() {
            let new_config = Configuration::new();
            let serialized = toml::to_string_pretty(&new_config)?;
            std::fs::write(config_path, serialized)?;
            Ok(new_config)
        } else {
            let raw_config = std::fs::read_to_string(config_path)?;
            let deserialized: Configuration = toml::from_str(&raw_config)?;
            Ok(deserialized)
        }
    }

    pub fn config_directory() -> anyhow::Result<PathBuf> {
        let config_directory_path = std::env::var("CONFIG_DIRECTORY")?;
        let config_directory = std::path::Path::new(&config_directory_path);

        Ok(config_directory.to_path_buf())
    }

    pub fn config_path() -> anyhow::Result<PathBuf> {
        let config_directory = Self::config_directory()?;
        let config_file_name = std::env::var("CONFIG_FILE_NAME")?;
        Ok(config_directory.join(&config_file_name))
    }
}
