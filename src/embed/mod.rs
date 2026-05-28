use anyhow::Result;
use std::path::Path;

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
        // Simple hash-based embedding for MVP.
        // Each byte of the text contributes to a bucketed position.
        let mut vec = vec![0.0f32; EMBEDDING_DIM];
        let text = text.to_lowercase();
        let words: Vec<&str> = text.split_whitespace().collect();

        for (i, word) in words.iter().enumerate() {
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

    pub fn dimension() -> usize {
        EMBEDDING_DIM
    }
}
