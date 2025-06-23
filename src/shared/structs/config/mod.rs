use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub server_bind_point: String,
    pub server_address: String,
    pub log_level: String,
    pub language_decider_prompt: String,
}

impl Configuration {
    pub fn new() -> Self {
        Configuration {
            server_bind_point: "0.0.0.0:80".into(),
            server_address: "http://localhost:80/".into(),
            log_level: "DEBUG".into(),
            language_decider_prompt: "".into(),
        }
    }

    pub fn load_from_config_file() -> anyhow::Result<Self> {
        let config_directory_path = std::env::var("CONFIG_DIRECTORY")?;
        let config_directory = std::path::Path::new(&config_directory_path);
        if !config_directory.exists() {
            std::fs::create_dir_all(&config_directory_path)?;
        }

        let config_file_name = std::env::var("CONFIG_FILE_NAME")?;
        let configuration_path = config_directory.join(&config_file_name);
        if !configuration_path.exists() {
            let new_config = Configuration::new();
            let serialized = toml::to_string_pretty(&new_config)?;
            std::fs::write(configuration_path, serialized)?;
            Ok(new_config)
        } else {
            let raw_config = std::fs::read_to_string(configuration_path)?;
            let deserialized: Configuration = toml::from_str(&raw_config)?;
            Ok(deserialized)
        }
    }
}
