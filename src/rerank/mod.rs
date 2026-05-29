use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::Config;
use crate::signal::ScoredFile;

/// AI-powered reranker that uses an LLM to score top candidates against the task.
/// This is the "insanely accurate" step — it catches semantic intent that
/// no algorithmic approach can match.
pub struct Reranker {
    openai_key: Option<String>,
    model: String,
    client: Option<reqwest::blocking::Client>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct RankedOutput {
    ranked: Vec<RankedFile>,
}

#[derive(Deserialize)]
struct RankedFile {
    path: String,
    relevance: String,
    reason: String,
}

impl Reranker {
    pub fn new(config: &Config) -> Self {
        let client = if config.has_openai_key() {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .ok()
        } else {
            None
        };

        Self {
            openai_key: config.openai_key.clone(),
            model: config
                .reranker_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            client,
        }
    }

    /// Rerank up to 30 candidate files against a task using an LLM.
    /// Returns the reranked list. If no API key, returns candidates as-is.
    pub fn rerank(
        &self,
        task: &str,
        candidates: &[ScoredFile],
        codebase_path: &std::path::Path,
    ) -> Result<Vec<ScoredFile>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let (Some(client), Some(key)) = (&self.client, &self.openai_key) else {
            return Ok(candidates.to_vec());
        };

        let top: Vec<ScoredFile> = candidates.iter().take(30).cloned().collect();

        self.call_reranker_api(client, key, task, &top, codebase_path)
    }

    fn call_reranker_api(
        &self,
        client: &reqwest::blocking::Client,
        key: &str,
        task: &str,
        candidates: &[ScoredFile],
        codebase_path: &std::path::Path,
    ) -> Result<Vec<ScoredFile>> {
        // Build file summaries for the prompt
        let mut file_list = String::new();
        let repo_name = codebase_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        for (i, f) in candidates.iter().enumerate() {
            let summary_preview = if f.summary.len() > 200 {
                format!("{}...", &f.summary[..200])
            } else {
                f.summary.clone()
            };
            file_list.push_str(&format!(
                "{}. `{}` [{}] — {}\n",
                i + 1,
                f.path,
                f.language,
                summary_preview.replace('\n', " ").trim()
            ));
        }

        let system_prompt = "You are an expert code reviewer and software architect. \
            Your job is to select files from a codebase that are RELEVANT to completing a given task. \
            Be ruthless — only include files the developer MUST look at or modify. \
            Omit test files, config files, and helpers unless they are directly relevant. \
            Return your answer as a JSON object with a 'ranked' array of objects, \
            each with 'path' (the file path), 'relevance' (CRITICAL|HIGH|MEDIUM|LOW|SKIP), \
            and 'reason' (one sentence why).";

        let user_prompt = format!(
            "Repository: `{}`\n\nTask: {}\n\nCandidate files:\n{}",
            repo_name, task, file_list
        );

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "response_format": { "type": "json_object" },
            "temperature": 0.1,
            "max_tokens": 2000,
        });

        let resp = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", key))
            .json(&body)
            .send()
            .context("Reranker API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text_body = resp.text().unwrap_or_default();
            anyhow::bail!("Reranker API error {}: {}", status, text_body);
        }

        let parsed: ChatResponse = resp.json().context("Failed to parse reranker response")?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        // Parse JSON from the response
        let ranked: RankedOutput = match serde_json::from_str(&content) {
            Ok(o) => o,
            Err(_) => {
                // Try to extract JSON from markdown fence
                if let Some(start) = content.find("```json") {
                    let trimmed = &content[start + 7..];
                    if let Some(end) = trimmed.find("```") {
                        if let Ok(o) = serde_json::from_str(&trimmed[..end]) {
                            return self.apply_rerank(candidates, o);
                        }
                    }
                }
                return Ok(candidates.to_vec());
            }
        };

        self.apply_rerank(candidates, ranked)
    }

    fn apply_rerank(
        &self,
        candidates: &[ScoredFile],
        ranked: RankedOutput,
    ) -> Result<Vec<ScoredFile>> {
        let relevance_map: std::collections::HashMap<&str, &RankedFile> = ranked
            .ranked
            .iter()
            .map(|r| (r.path.as_str(), r))
            .collect();

        let mut reranked: Vec<ScoredFile> = candidates
            .iter()
            .filter_map(|f| {
                if let Some(ranked_file) = relevance_map.get(f.path.as_str()) {
                    match ranked_file.relevance.as_str() {
                        "SKIP" => None,
                        _ => {
                            let mut new_score = f.score;
                            match ranked_file.relevance.as_str() {
                                "CRITICAL" => new_score = 1.0,
                                "HIGH" => new_score = 0.9,
                                "MEDIUM" => new_score = 0.6,
                                _ => {}
                            }
                            let mut f2 = f.clone();
                            f2.score = new_score;
                            Some(f2)
                        }
                    }
                } else {
                    Some(f.clone())
                }
            })
            .collect();

        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(reranked)
    }
}
