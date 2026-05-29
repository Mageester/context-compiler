use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Context Compiler configuration stored in .ctx/config.toml
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// OpenAI API key for embeddings and AI reranker
    pub openai_key: Option<String>,
    /// Embedding model to use (default: text-embedding-3-small)
    pub embedding_model: Option<String>,
    /// Reranker model to use (default: gpt-4o-mini)
    pub reranker_model: Option<String>,
    /// Whether to use AI reranker (default: true if key is set)
    pub use_reranker: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openai_key: None,
            embedding_model: Some("text-embedding-3-small".to_string()),
            reranker_model: Some("gpt-4o-mini".to_string()),
            use_reranker: Some(true),
        }
    }
}

impl Config {
    /// Load config from a project path (.ctx/config.toml).
    /// Falls back to env vars, then defaults.
    pub fn load(project_path: &Path) -> Self {
        // Try file first
        let file_config = Self::from_file(project_path);
        // Then override with env vars
        Self::from_env().merge(file_config)
    }

    /// Load config from .ctx/config.toml
    fn from_file(project_path: &Path) -> Option<Self> {
        let config_path = project_path.join(".ctx/config.toml");
        let content = std::fs::read_to_string(config_path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Load from environment variables
    fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(key) = std::env::var("CONTEXT_COMPILER_OPENAI_KEY") {
            config.openai_key = Some(key);
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            config.openai_key = Some(key);
        }

        if let Ok(model) = std::env::var("CONTEXT_COMPILER_EMBEDDING_MODEL") {
            config.embedding_model = Some(model);
        }

        if let Ok(model) = std::env::var("CONTEXT_COMPILER_RERANKER_MODEL") {
            config.reranker_model = Some(model);
        }

        config
    }

    pub fn has_openai_key(&self) -> bool {
        self.openai_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false)
    }

    /// Save config to .ctx/config.toml in the given project
    pub fn save(&self, project_path: &Path) -> Result<()> {
        let config_dir = project_path.join(".ctx");
        std::fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join("config.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Merge another config into this one (non-None fields win)
    fn merge(self, other: Option<Self>) -> Self {
        let Some(other) = other else { return self };
        // Treat empty string keys as unset
        let other_key = other.openai_key.filter(|k| !k.is_empty());
        let self_key = self.openai_key.filter(|k| !k.is_empty());
        Self {
            openai_key: other_key.or(self_key),
            embedding_model: other.embedding_model.or(self.embedding_model),
            reranker_model: other.reranker_model.or(self.reranker_model),
            use_reranker: other.use_reranker.or(self.use_reranker),
        }
    }
}

/// Home config path (~/.ctx/config.toml for global settings)
pub fn global_config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".ctx/config.toml")
}

/// Load global config from ~/.ctx/config.toml
pub fn load_global_config() -> Config {
    let path = global_config_path();
    if let Some(config) = Config::from_file(path.parent().unwrap_or(Path::new("."))) {
        Config::from_env().merge(Some(config))
    } else {
        Config::from_env()
    }
}
