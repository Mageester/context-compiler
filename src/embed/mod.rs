use anyhow::Result;

/// Local embedding engine using ONNX Runtime.
/// Runs all-MiniLM-L6-v2 model in-process (no API calls needed).
pub struct Embedder;

const EMBEDDING_DIM: usize = 384;

impl Embedder {
    /// Initialize the embedding model
    pub fn init() -> Result<Self> {
        // In v1, we embed via a simple hash-based approach for demo purposes.
        // Production: load ONNX model from bundled binary.
        Ok(Embedder)
    }

    /// Embed a text string into a 384-dimensional vector.
    /// Falls back to a hash-based approximation if ONNX model isn't bundled yet.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        // Lightweight lexical embedding for the MVP.
        // It handles natural text plus code identifiers such as `auth_middleware`,
        // `AuthMiddleware`, `src/compile/mod.rs`, and `ctx compile`.
        let mut vec = vec![0.0f32; EMBEDDING_DIM];
        let words = Self::tokenize(text);

        if words.is_empty() {
            return vec;
        }

        for word in &words {
            let hash = Self::simple_hash(word);
            let pos = (hash as usize) % EMBEDDING_DIM;
            let freq = words.iter().filter(|w| *w == word).count() as f32;
            vec[pos] += freq / words.len() as f32;
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
    fn tokenize(text: &str) -> Vec<String> {
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
        let embedder = Embedder::init().unwrap();
        let vec = embedder.embed("fix the auth race condition");
        assert_eq!(vec.len(), 384);
    }

    #[test]
    fn test_embed_normalized() {
        let embedder = Embedder::init().unwrap();
        let vec = embedder.embed("add tests for payment webhook");
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-5, "magnitude = {}", magnitude);
    }

    #[test]
    fn test_embed_empty_text() {
        let embedder = Embedder::init().unwrap();
        let vec = embedder.embed("");
        assert_eq!(vec.len(), 384);
        // Empty text should result in zero vector (all 0.0)
        let sum: f32 = vec.iter().sum();
        assert_eq!(sum, 0.0);
    }

    #[test]
    fn test_embed_similar_tasks_have_similar_vectors() {
        let embedder = Embedder::init().unwrap();
        let a = embedder.embed("fix login timeout bug");
        let b = embedder.embed("resolve login timeout issue");
        let c = embedder.embed("add new color theme for settings page");
        let sim_ab = Embedder::cosine_similarity(&a, &b);
        let sim_ac = Embedder::cosine_similarity(&a, &c);
        assert!(
            sim_ab > sim_ac,
            "similar tasks should be more similar than different ones: {} vs {}",
            sim_ab,
            sim_ac
        );
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
    fn test_simple_hash_different() {
        let h1 = Embedder::simple_hash("auth");
        let h2 = Embedder::simple_hash("payment");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_tokenize_filters_short_words() {
        let words = Embedder::tokenize("a b c d e f");
        assert!(words.is_empty(), "all single-char words should be filtered");
    }
}
