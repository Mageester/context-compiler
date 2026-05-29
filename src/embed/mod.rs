use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::Config;

const EMBEDDING_DIM: usize = 1536; // text-embedding-3-small

/// Embedding engine with real OpenAI embeddings + hash-based fallback.
pub struct Embedder {
    openai_key: Option<String>,
    model: String,
    client: Option<reqwest::blocking::Client>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    usage: EmbeddingUsage,
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

impl Embedder {
    /// Initialize the embedding engine.
    pub fn init(config: &Config) -> Result<Self> {
        let client = if config.has_openai_key() {
            Some(
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .context("Failed to build HTTP client")?,
            )
        } else {
            None
        };

        Ok(Embedder {
            openai_key: config.openai_key.clone(),
            model: config
                .embedding_model
                .clone()
                .unwrap_or_else(|| "text-embedding-3-small".to_string()),
            client,
        })
    }

    /// Embed a text string into a vector.
    /// Uses OpenAI API if key is available, otherwise falls back to hash-based.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        if let (Some(client), Some(key)) = (&self.client, &self.openai_key) {
            match self.embed_api(client, key, text) {
                Ok(vec) if !vec.is_empty() => return vec,
                _ => {}
            }
        }
        // Fallback: hash-based embed
        Self::hash_embed(text)
    }

    /// Call OpenAI embeddings API using blocking reqwest
    fn embed_api(
        &self,
        client: &reqwest::blocking::Client,
        key: &str,
        text: &str,
    ) -> Result<Vec<f32>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
            "dimensions": EMBEDDING_DIM,
        });

        let resp = client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", key))
            .json(&body)
            .send()
            .context("OpenAI embedding API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text_body = resp.text().unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, text_body);
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
