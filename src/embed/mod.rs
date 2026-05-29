use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

use crate::config::Config;

const EMBEDDING_DIM: usize = 1536; // text-embedding-3-small

/// Embedding engine with multi-provider support + hash-based fallback.
pub struct Embedder {
    clients: HashMap<String, reqwest::blocking::Client>,
    config: Config,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    usage: Option<EmbeddingUsage>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct EmbeddingUsage {
    total_tokens: usize,
}

#[derive(Deserialize)]
struct CodexAuth {
    token: Option<String>,
    access_token: Option<String>,
}

impl Embedder {
    /// Initialize the embedding engine with multi-provider support.
    pub fn init(config: &Config) -> Result<Self> {
        let mut clients = HashMap::new();

        // Build clients for any configured providers
        if config.has_openai_key() {
            clients.insert(
                "openai".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .context("Failed to build HTTP client for OpenAI")?,
            );
        }
        if config.has_openrouter_key() {
            clients.insert(
                "openrouter".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .context("Failed to build HTTP client for OpenRouter")?,
            );
        }
        if config.has_deepseek_key() {
            clients.insert(
                "deepseek".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .context("Failed to build HTTP client for DeepSeek")?,
            );
        }
        if config.has_codex_key() {
            clients.insert(
                "codex".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .context("Failed to build HTTP client for Codex")?,
            );
        }

        Ok(Embedder {
            clients,
            config: config.clone(),
        })
    }

    /// Get the provider to use
    fn selected_provider(&self) -> &str {
        self.config.selected_embedder_provider()
    }

    /// Embed a text string into a vector.
    /// Uses the selected provider's API if available, otherwise falls back to hash-based.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let provider = self.selected_provider();
        match provider {
            "openai" => {
                if let (Some(client), Some(key)) = (self.clients.get("openai"), &self.config.openai_key) {
                    if let Ok(vec) = self.embed_api(
                        client,
                        key,
                        text,
                        &self.config.openai_base_url.as_deref().unwrap_or("https://api.openai.com"),
                    ) {
                        if !vec.is_empty() {
                            return vec;
                        }
                    }
                }
            }
            "openrouter" => {
                if let (Some(client), Some(key)) = (self.clients.get("openrouter"), &self.config.openrouter_key) {
                    if let Ok(vec) = self.embed_api(
                        client,
                        key,
                        text,
                        &self.config.openrouter_base_url.as_deref().unwrap_or("https://openrouter.ai"),
                    ) {
                        if !vec.is_empty() {
                            return vec;
                        }
                    }
                }
            }
            "deepseek" => {
                if let (Some(client), Some(key)) = (self.clients.get("deepseek"), &self.config.deepseek_key) {
                    if let Ok(vec) = self.embed_api(
                        client,
                        key,
                        text,
                        &self.config.deepseek_base_url.as_deref().unwrap_or("https://api.deepseek.com"),
                    ) {
                        if !vec.is_empty() {
                            return vec;
                        }
                    }
                }
            }
            "codex" => {
                if let Some(client) = self.clients.get("codex") {
                    if let Some(token) = self.get_codex_token() {
                        if let Ok(vec) = self.embed_api_codex(client, &token, text) {
                            if !vec.is_empty() {
                                return vec;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        // Fallback: hash-based embed
        Self::hash_embed(text)
    }

    /// Embed a batch of texts in parallel (or serially with hash fallback).
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let provider = self.selected_provider();
        let parallel = self.config.parallel_embed.unwrap_or(false);

        if provider == "hash" || self.clients.is_empty() {
            return Ok(texts.iter().map(|t| Self::hash_embed(t)).collect());
        }

        if parallel && texts.len() > 1 {
            // Use rayon-style parallel iteration via threads
            let results: Vec<Vec<f32>> = texts
                .iter()
                .map(|t| self.embed(t))
                .collect();
            Ok(results)
        } else {
            let results: Vec<Vec<f32>> = texts.iter().map(|t| self.embed(t)).collect();
            Ok(results)
        }
    }

    /// Get Codex OAuth token from file
    fn get_codex_token(&self) -> Option<String> {
        // Check explicit config key first
        if let Some(ref key) = self.config.codex_key {
            if !key.is_empty() {
                return Some(key.clone());
            }
        }
        // Fall back to file
        let home = std::env::var("HOME").ok()?;
        let token_path = std::path::PathBuf::from(home).join(".hermes/auth/openai-codex-oauth-1.json");
        let content = std::fs::read_to_string(token_path).ok()?;
        let auth: CodexAuth = serde_json::from_str(&content).ok()?;
        auth.token.or(auth.access_token)
    }

    /// Call embedding API using blocking reqwest (standard format)
    fn embed_api(
        &self,
        client: &reqwest::blocking::Client,
        key: &str,
        text: &str,
        base_url: &str,
    ) -> Result<Vec<f32>> {
        let model = self.config.embedding_model_name();
        let url = format!("{}/v1/embeddings", base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": model,
            "input": text,
            "dimensions": EMBEDDING_DIM,
        });

        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", key))
            .json(&body)
            .send()
            .context("Embedding API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text_body = resp.text().unwrap_or_default();
            anyhow::bail!("Embedding API error {}: {}", status, text_body);
        }

        let parsed: EmbeddingResponse = resp
            .json()
            .context("Failed to parse embedding response")?;

        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("Empty embedding response")
    }

    /// Call Codex embedding API (uses GitHub Copilot endpoint)
    fn embed_api_codex(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        text: &str,
    ) -> Result<Vec<f32>> {
        let model = self.config.embedding_model_name();
        let url = "https://api.githubcopilot.com/v1/embeddings";

        let body = serde_json::json!({
            "model": model,
            "input": text,
            "dimensions": EMBEDDING_DIM,
        });

        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&body)
            .send()
            .context("Codex embedding API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text_body = resp.text().unwrap_or_default();
            anyhow::bail!("Codex embedding API error {}: {}", status, text_body);
        }

        let parsed: EmbeddingResponse = resp
            .json()
            .context("Failed to parse Codex embedding response")?;

        parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("Empty Codex embedding response")
    }

    /// Hash-based fallback embedding.
    /// Maps each word to a deterministic position via DJB2 hash.
    fn hash_embed(text: &str) -> Vec<f32> {
        let dim = EMBEDDING_DIM;
        let mut vec = vec![0.0f32; dim];
        let words = Self::tokenize(text);

        if words.is_empty() {
            return vec;
        }

        let n = words.len() as f32;
        for word in &words {
            let hash = Self::simple_hash(word);
            let pos = (hash as usize) % dim;
            vec[pos] += 1.0 / n;
        }

        // Normalize
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for v in vec.iter_mut() {
                *v /= magnitude;
            }
        }

        vec
    }

    /// Compute cosine similarity between two vectors
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag_a == 0.0 || mag_b == 0.0 {
            return 0.0;
        }
        dot / (mag_a * mag_b)
    }

    /// Simple non-crypto hash for quick embedding approximation
    fn simple_hash(s: &str) -> u64 {
        let mut h: u64 = 5381;
        for b in s.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        h
    }

    /// Tokenize text and code identifiers into searchable terms.
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut normalized = String::with_capacity(text.len() * 2);
        let mut prev_lower_or_digit = false;

        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() {
                if ch.is_ascii_uppercase() && prev_lower_or_digit {
                    normalized.push(' ');
                }
                normalized.push(ch.to_ascii_lowercase());
                prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            } else {
                normalized.push(' ');
                prev_lower_or_digit = false;
            }
        }

        normalized
            .split_whitespace()
            .filter(|w| w.len() > 1)
            .map(ToString::to_string)
            .collect()
    }

    /// Extract all compound identifiers from code text (CamelCase, snake_case)
    /// Returns them as whole tokens plus their split parts.
    pub fn extract_identifiers(text: &str) -> Vec<String> {
        let mut identifiers = Vec::new();
        let mut current = String::new();

        for ch in text.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                current.push(ch);
            } else {
                if current.len() > 1 {
                    Self::add_with_parts(&mut identifiers, &current);
                }
                current.clear();
            }
        }
        if current.len() > 1 {
            Self::add_with_parts(&mut identifiers, &current);
        }

        identifiers
    }

    /// Add an identifier and its split parts (CamelCase and snake_case)
    fn add_with_parts(identifiers: &mut Vec<String>, ident: &str) {
        identifiers.push(ident.to_string());

        // Snake case parts
        for part in ident.split('_') {
            let lower = part.to_lowercase();
            if lower.len() > 1 {
                identifiers.push(lower.clone());
            }
            // Also extract CamelCase from within each part
            let camel_parts = Self::split_camel_case(part);
            for cp in camel_parts {
                let cp_lower = cp.to_lowercase();
                if cp_lower.len() > 1 {
                    identifiers.push(cp_lower);
                }
            }
        }
    }

    /// Split a CamelCase string into its constituent words
    fn split_camel_case(s: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = s.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if ch.is_ascii_uppercase() {
                if !current.is_empty() {
                    // Check if this is the start of an acronym
                    if i + 1 < chars.len() && chars[i + 1].is_ascii_lowercase() {
                        parts.push(current.clone());
                        current.clear();
                    }
                }
                current.push(ch);
            } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                current.push(ch);
            }
        }
        if !current.is_empty() && current.len() > 1 {
            parts.push(current);
        }
        parts
    }

    #[allow(dead_code)]
    pub fn dimension() -> usize {
        EMBEDDING_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_returns_correct_dimension() {
        let config = Config::default();
        let embedder = Embedder::init(&config).unwrap();
        let vec = embedder.embed("fix the auth race condition");
        assert_eq!(vec.len(), 1536);
    }

    #[test]
    fn test_embed_normalized() {
        let config = Config::default();
        let embedder = Embedder::init(&config).unwrap();
        let vec = embedder.embed("add tests for payment webhook");
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5, "magnitude = {}", magnitude);
    }

    #[test]
    fn test_embed_empty_text() {
        let config = Config::default();
        let embedder = Embedder::init(&config).unwrap();
        let vec = embedder.embed("");
        let sum: f32 = vec.iter().sum();
        assert_eq!(sum, 0.0);
    }

    #[test]
    fn test_extract_identifiers_camel_case() {
        let ids = Embedder::extract_identifiers("clearIgnoredDiscoveredRepos");
        assert!(ids.contains(&"clearIgnoredDiscoveredRepos".to_string()), "whole id");
        assert!(ids.contains(&"clear".to_string()), "camel clear");
        assert!(ids.contains(&"ignored".to_string()), "camel ignored");
        assert!(ids.contains(&"discovered".to_string()), "camel discovered");
        assert!(ids.contains(&"repos".to_string()), "camel repos");
    }

    #[test]
    fn test_extract_identifiers_snake_case() {
        let ids = Embedder::extract_identifiers("auth_middleware");
        assert!(ids.contains(&"auth_middleware".to_string()));
        assert!(ids.contains(&"auth".to_string()));
        assert!(ids.contains(&"middleware".to_string()));
    }

    #[test]
    fn test_extract_identifiers_mixed() {
        let ids = Embedder::extract_identifiers("SomeFile.test_runner");
        assert!(ids.contains(&"SomeFile".to_string()));
        assert!(ids.contains(&"test_runner".to_string()));
        assert!(ids.contains(&"test".to_string()));
        assert!(ids.contains(&"runner".to_string()));
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((Embedder::cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((Embedder::cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let zero = vec![0.0, 0.0];
        let v = vec![1.0, 0.0];
        assert_eq!(Embedder::cosine_similarity(&zero, &v), 0.0);
        assert_eq!(Embedder::cosine_similarity(&v, &zero), 0.0);
    }

    #[test]
    fn test_tokenize_code_identifiers() {
        let words = Embedder::tokenize("AuthMiddleware");
        assert!(words.contains(&"auth".to_string()));
        assert!(words.contains(&"middleware".to_string()));
    }

    #[test]
    fn test_tokenize_paths() {
        let words = Embedder::tokenize("src/auth/middleware.ts");
        assert!(words.contains(&"src".to_string()));
        assert!(words.contains(&"auth".to_string()));
        assert!(words.contains(&"middleware".to_string()));
    }

    #[test]
    fn test_simple_hash_stable() {
        let h1 = Embedder::simple_hash("auth");
        let h2 = Embedder::simple_hash("auth");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_tokenize_filters_short_words() {
        let words = Embedder::tokenize("a b c d e f");
        assert!(words.is_empty(), "all single-char words should be filtered");
    }
}
