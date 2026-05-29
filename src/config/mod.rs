use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Context Compiler configuration stored in .ctx/config.toml
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// OpenAI API key for embeddings and AI reranker
    pub openai_key: Option<String>,
    /// OpenRouter API key
    pub openrouter_key: Option<String>,
    /// DeepSeek API key
    pub deepseek_key: Option<String>,
    /// GitHub Copilot / Codex OAuth key
    pub codex_key: Option<String>,
    /// Provider selection: "openai", "openrouter", "deepseek", "auto"
    pub provider: Option<String>,
    /// OpenAI base URL override
    pub openai_base_url: Option<String>,
    /// OpenRouter base URL override
    pub openrouter_base_url: Option<String>,
    /// DeepSeek base URL override
    pub deepseek_base_url: Option<String>,
    /// Embedding model to use (default depends on provider)
    pub embedding_model: Option<String>,
    /// Reranker model to use (default depends on provider)
    pub reranker_model: Option<String>,
    /// Whether to use AI reranker (default: true if key is set)
    pub use_reranker: Option<bool>,
    /// Whether to use ensemble reranking (two providers)
    pub ensemble_rerank: Option<bool>,
    /// Number of ensemble rerankers to use
    pub ensemble_count: Option<usize>,
    /// Enable code-level chunking (future)
    pub code_chunking: Option<bool>,
    /// Enable cross-file reference analysis (future)
    pub cross_file_refs: Option<bool>,
    /// Enable term expansion for queries (future)
    pub term_expansion: Option<bool>,
    /// Enable parallel embedding
    pub parallel_embed: Option<bool>,
    /// Cache TTL in seconds
    pub cache_ttl: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openai_key: None,
            openrouter_key: None,
            deepseek_key: None,
            codex_key: None,
            provider: None,
            openai_base_url: None,
            openrouter_base_url: None,
            deepseek_base_url: None,
            embedding_model: Some("text-embedding-3-small".to_string()),
            reranker_model: Some("gpt-4o-mini".to_string()),
            use_reranker: Some(true),
            ensemble_rerank: Some(false),
            ensemble_count: Some(2),
            code_chunking: Some(false),
            cross_file_refs: Some(false),
            term_expansion: Some(false),
            parallel_embed: Some(false),
            cache_ttl: Some(3600),
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

        // Provider
        if let Ok(p) = std::env::var("CONTEXT_COMPILER_PROVIDER") {
            config.provider = Some(p);
        }

        // OpenAI key (backward compat)
        if let Ok(key) = std::env::var("CONTEXT_COMPILER_OPENAI_KEY") {
            config.openai_key = Some(key);
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            config.openai_key = Some(key);
        }

        // OpenRouter key
        if let Ok(key) = std::env::var("CONTEXT_COMPILER_OPENROUTER_KEY") {
            config.openrouter_key = Some(key);
        }

        // DeepSeek key
        if let Ok(key) = std::env::var("CONTEXT_COMPILER_DEEPSEEK_KEY") {
            config.deepseek_key = Some(key);
        }

        // Codex key
        if let Ok(key) = std::env::var("CONTEXT_COMPILER_CODEX_KEY") {
            config.codex_key = Some(key);
        }

        // Base URLs
        if let Ok(url) = std::env::var("CONTEXT_COMPILER_OPENAI_BASE_URL") {
            config.openai_base_url = Some(url);
        }
        if let Ok(url) = std::env::var("CONTEXT_COMPILER_OPENROUTER_BASE_URL") {
            config.openrouter_base_url = Some(url);
        }
        if let Ok(url) = std::env::var("CONTEXT_COMPILER_DEEPSEEK_BASE_URL") {
            config.deepseek_base_url = Some(url);
        }

        // Models
        if let Ok(model) = std::env::var("CONTEXT_COMPILER_EMBEDDING_MODEL") {
            config.embedding_model = Some(model);
        }
        if let Ok(model) = std::env::var("CONTEXT_COMPILER_RERANKER_MODEL") {
            config.reranker_model = Some(model);
        }

        // Flags
        if let Ok(v) = std::env::var("CONTEXT_COMPILER_USE_RERANKER") {
            config.use_reranker = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("CONTEXT_COMPILER_ENSEMBLE_RERANK") {
            config.ensemble_rerank = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("CONTEXT_COMPILER_CODE_CHUNKING") {
            config.code_chunking = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("CONTEXT_COMPILER_CROSS_FILE_REFS") {
            config.cross_file_refs = Some(v == "true" || v == "1");
        }
        if let Ok(v) = std::env::var("CONTEXT_COMPILER_PARALLEL_EMBED") {
            config.parallel_embed = Some(v == "true" || v == "1");
        }

        config
    }

    pub fn has_openai_key(&self) -> bool {
        self.openai_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false)
    }

    pub fn has_openrouter_key(&self) -> bool {
        self.openrouter_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false)
    }

    pub fn has_deepseek_key(&self) -> bool {
        self.deepseek_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false)
    }

    pub fn has_codex_key(&self) -> bool {
        // Check explicit key first
        if self.codex_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) {
            return true;
        }
        // Also check for Codex OAuth token file
        Self::codex_oauth_token().is_some()
    }

    /// Read Codex OAuth token from $HOME/.hermes/auth/openai-codex-oauth-1.json
    fn codex_oauth_token() -> Option<String> {
        let home = std::env::var("HOME").ok()?;
        let token_path = PathBuf::from(home).join(".hermes/auth/openai-codex-oauth-1.json");
        let content = std::fs::read_to_string(token_path).ok()?;
        // Parse JSON to get the token
        #[derive(Deserialize)]
        struct CodexAuth {
            token: Option<String>,
            access_token: Option<String>,
        }
        let auth: CodexAuth = serde_json::from_str(&content).ok()?;
        auth.token.or(auth.access_token)
    }

    /// Get the effective provider for embeddings
    pub fn selected_embedder_provider(&self) -> &str {
        if let Some(ref p) = self.provider {
            match p.as_str() {
                "openai" => return "openai",
                "openrouter" => return "openrouter",
                "deepseek" => return "deepseek",
                "codex" => return "codex",
                _ => {}
            }
        }
        // Auto-detect based on available keys
        if self.has_openai_key() {
            "openai"
        } else if self.has_openrouter_key() {
            "openrouter"
        } else if self.has_deepseek_key() {
            "deepseek"
        } else if self.has_codex_key() {
            "codex"
        } else {
            "hash"
        }
    }

    /// Get the effective provider for reranking
    pub fn selected_reranker_provider(&self) -> &str {
        if let Some(ref p) = self.provider {
            match p.as_str() {
                "openai" => return "openai",
                "openrouter" => return "openrouter",
                "deepseek" => return "deepseek",
                "codex" => return "codex",
                _ => {}
            }
        }
        // Auto-detect based on available keys
        if self.has_openai_key() {
            "openai"
        } else if self.has_openrouter_key() {
            "openrouter"
        } else if self.has_deepseek_key() {
            "deepseek"
        } else if self.has_codex_key() {
            "codex"
        } else {
            "none"
        }
    }

    /// Get the embedding model name for the selected provider
    pub fn embedding_model_name(&self) -> String {
        if let Some(ref model) = self.embedding_model {
            return model.clone();
        }
        match self.selected_embedder_provider() {
            "openrouter" => "openai/text-embedding-3-small".to_string(),
            "deepseek" => "deepseek-embedding".to_string(),
            "codex" => "text-embedding-3-small".to_string(),
            _ => "text-embedding-3-small".to_string(),
        }
    }

    /// Get the reranker model name for the selected provider
    pub fn reranker_model_name(&self) -> String {
        if let Some(ref model) = self.reranker_model {
            return model.clone();
        }
        match self.selected_reranker_provider() {
            "openrouter" => "openai/gpt-4o-mini".to_string(),
            "deepseek" => "deepseek-chat".to_string(),
            "codex" => "gpt-4o-mini".to_string(),
            _ => "gpt-4o-mini".to_string(),
        }
    }

    /// Human-readable provider summary
    pub fn provider_context_summary(&self) -> String {
        let embed_provider = self.selected_embedder_provider();
        let rerank_provider = self.selected_reranker_provider();
        let embed_model = self.embedding_model_name();
        let rerank_model = self.reranker_model_name();

        let mut parts = Vec::new();
        if embed_provider == "hash" {
            parts.push(format!("Embed: hash-based"));
        } else {
            parts.push(format!("Embed: {} ({})", embed_provider, embed_model));
        }
        if rerank_provider == "none" {
            parts.push(format!("Rerank: disabled"));
        } else {
            parts.push(format!("Rerank: {} ({})", rerank_provider, rerank_model));
            if self.ensemble_rerank.unwrap_or(false) {
                parts.push(format!("Ensemble: on ({} providers)", self.ensemble_count.unwrap_or(2)));
            }
        }
        parts.join(" · ")
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
        let other_ork = other.openrouter_key.filter(|k| !k.is_empty());
        let self_ork = self.openrouter_key.filter(|k| !k.is_empty());
        let other_dsk = other.deepseek_key.filter(|k| !k.is_empty());
        let self_dsk = self.deepseek_key.filter(|k| !k.is_empty());
        let other_cdx = other.codex_key.filter(|k| !k.is_empty());
        let self_cdx = self.codex_key.filter(|k| !k.is_empty());

        Self {
            openai_key: other_key.or(self_key),
            openrouter_key: other_ork.or(self_ork),
            deepseek_key: other_dsk.or(self_dsk),
            codex_key: other_cdx.or(self_cdx),
            provider: other.provider.or(self.provider),
            openai_base_url: other.openai_base_url.or(self.openai_base_url),
            openrouter_base_url: other.openrouter_base_url.or(self.openrouter_base_url),
            deepseek_base_url: other.deepseek_base_url.or(self.deepseek_base_url),
            embedding_model: other.embedding_model.or(self.embedding_model),
            reranker_model: other.reranker_model.or(self.reranker_model),
            use_reranker: other.use_reranker.or(self.use_reranker),
            ensemble_rerank: other.ensemble_rerank.or(self.ensemble_rerank),
            ensemble_count: other.ensemble_count.or(self.ensemble_count),
            code_chunking: other.code_chunking.or(self.code_chunking),
            cross_file_refs: other.cross_file_refs.or(self.cross_file_refs),
            term_expansion: other.term_expansion.or(self.term_expansion),
            parallel_embed: other.parallel_embed.or(self.parallel_embed),
            cache_ttl: other.cache_ttl.or(self.cache_ttl),
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
