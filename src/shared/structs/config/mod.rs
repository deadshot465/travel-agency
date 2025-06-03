use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

pub const CONFIG_DIRECTORY: &str = "config";
pub const CONFIG_FILE_NAME: &str = "config.toml";

pub static CONFIGURATION: Lazy<Configuration> = Lazy::new(|| {
    let config =
        Configuration::load_from_config_file().expect("Failed to load config from config file.");
    config
});

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub server_bind_point: String,
    pub server_address: String,
    pub log_level: String,
}

impl Configuration {
    pub fn new() -> Self {
        Configuration {
            server_bind_point: "0.0.0.0:80".into(),
            server_address: "http://localhost:80/".into(),
            log_level: "DEBUG".into(),
        }
    }

    pub fn load_from_config_file() -> anyhow::Result<Self> {
        let config_directory = std::path::Path::new(CONFIG_DIRECTORY);
        if !config_directory.exists() {
            std::fs::create_dir(CONFIG_DIRECTORY)?;
        }

        let configuration_path = config_directory.join(CONFIG_FILE_NAME);
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
