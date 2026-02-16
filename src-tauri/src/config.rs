use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub description: String,
    pub indexed_paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub embedding_model: String,
    pub containers: HashMap<String, ContainerInfo>,
    pub active_container: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut containers = HashMap::new();
        containers.insert("Default".to_string(), ContainerInfo {
            description: String::new(),
            indexed_paths: Vec::new(),
        });
        Self {
            embedding_model: "MultilingualE5Base".to_string(),
            containers,
            active_container: "Default".to_string(),
        }
    }
}

pub struct ConfigState {
    pub config: Arc<Mutex<Config>>,
    pub path: std::path::PathBuf,
}

impl ConfigState {
    pub async fn save(&self) -> Result<(), String> {
        let config = self.config.lock().await;
        let content = serde_json::to_string_pretty(&*config).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, content).map_err(|e| e.to_string())
    }
}

pub fn get_table_name(container: &str) -> String {
    let sanitized: String = container.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
            c.to_string()
        } else {
            format!("{:04x}", c as u32)
        }
    }).collect();
    format!("c_{}", sanitized)
}

pub fn get_embedding_model(name: &str) -> fastembed::EmbeddingModel {
    match name {
        "AllMiniLML6V2" => fastembed::EmbeddingModel::AllMiniLML6V2,
        "MultilingualE5Small" => fastembed::EmbeddingModel::MultilingualE5Small,
        "MultilingualE5Base" => fastembed::EmbeddingModel::MultilingualE5Base,
        _ => fastembed::EmbeddingModel::MultilingualE5Base,
    }
}
