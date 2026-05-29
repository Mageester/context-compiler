use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

use crate::config::Config;
use crate::signal::ScoredFile;

/// AI-powered reranker that uses an LLM to score top candidates against the task.
/// This is the "insanely accurate" step — it catches semantic intent that
/// no algorithmic approach can match.
pub struct Reranker {
    config: Config,
    clients: HashMap<String, reqwest::blocking::Client>,
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

#[derive(Deserialize)]
struct CodexAuth {
    token: Option<String>,
    access_token: Option<String>,
}

impl Reranker {
    pub fn new(config: &Config) -> Self {
        let mut clients = HashMap::new();

        if config.has_openai_key() {
            clients.insert(
                "openai".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                    .ok()
                    .unwrap_or_default(),
            );
        }
        if config.has_openrouter_key() {
            clients.insert(
                "openrouter".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                    .ok()
                    .unwrap_or_default(),
            );
        }
        if config.has_deepseek_key() {
            clients.insert(
                "deepseek".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                    .ok()
                    .unwrap_or_default(),
            );
        }
        if config.has_codex_key() {
            clients.insert(
                "codex".to_string(),
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(60))
                    .build()
                    .ok()
                    .unwrap_or_default(),
            );
        }

        Self {
            config: config.clone(),
            clients,
        }
    }

    /// Get the primary reranker provider
    fn selected_provider(&self) -> &str {
        self.config.selected_reranker_provider()
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

        // Check if ensemble mode is enabled
        let use_ensemble = self.config.ensemble_rerank.unwrap_or(false);

        if use_ensemble {
            self.rerank_ensemble(task, candidates, codebase_path)
        } else {
            self.rerank_single(task, candidates, codebase_path)
        }
    }

    /// Single provider reranking
    fn rerank_single(
        &self,
        task: &str,
        candidates: &[ScoredFile],
        codebase_path: &std::path::Path,
    ) -> Result<Vec<ScoredFile>> {
        let provider = self.selected_provider();
        if provider == "none" {
            return Ok(candidates.to_vec());
        }

        let top: Vec<ScoredFile> = candidates.iter().take(30).cloned().collect();
        self.call_provider_reranker(provider, task, &top, codebase_path)
    }

    /// Ensemble reranking: call two providers and merge results
    fn rerank_ensemble(
        &self,
        task: &str,
        candidates: &[ScoredFile],
        codebase_path: &std::path::Path,
    ) -> Result<Vec<ScoredFile>> {
        let top: Vec<ScoredFile> = candidates.iter().take(30).cloned().collect();

        // Determine which two providers to use
        let providers = self.get_ensemble_providers();
        if providers.is_empty() {
            return Ok(candidates.to_vec());
        }

        let mut all_results: Vec<(String, ScoredFile, f32, String)> = Vec::new(); // (path, file, score, reason)

        for provider in &providers {
            match self.call_provider_reranker(provider, task, &top, codebase_path) {
                Ok(results) => {
                    for file in &results {
                        all_results.push((
                            file.path.clone(),
                            file.clone(),
                            file.score,
                            String::new(),
                        ));
                    }
                }
                Err(e) => {
                    log::warn!("Ensemble reranker {} failed: {}", provider, e);
                }
            }
        }

        if all_results.is_empty() {
            return Ok(candidates.to_vec());
        }

        // Merge: for each unique path, compute average score and keep max relevance
        let mut merged: HashMap<String, (Vec<f32>, Vec<String>)> = HashMap::new();
        for (path, _file, score, reason) in &all_results {
            let entry = merged
                .entry(path.clone())
                .or_insert_with(|| (Vec::new(), Vec::new()));
            entry.0.push(*score);
            entry.1.push(reason.clone());
        }

        // Build merged scored files
        let mut result: Vec<ScoredFile> = candidates
            .iter()
            .filter_map(|candidate| {
                if let Some((scores, _reasons)) = merged.get(&candidate.path) {
                    let avg_score = scores.iter().sum::<f32>() / scores.len() as f32;
                    let max_score = scores.iter().cloned().fold(0.0f32, f32::max);
                    // CRITICAL from any provider boosts to 1.0
                    let final_score = if max_score >= 0.99 { 1.0 } else { avg_score };
                    let mut f = candidate.clone();
                    f.score = final_score;
                    Some(f)
                } else {
                    // File not in any provider's results - keep with reduced score
                    let mut f = candidate.clone();
                    f.score = f.score * 0.5;
                    Some(f)
                }
            })
            .collect();

        result.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(result)
    }

    /// Get the providers to use for ensemble reranking
    fn get_ensemble_providers(&self) -> Vec<String> {
        let primary = self.selected_provider();
        let mut providers = Vec::new();

        match primary {
            "openai" => {
                providers.push("openai".to_string());
                if self.config.has_openrouter_key() {
                    providers.push("openrouter".to_string());
                } else if self.config.has_deepseek_key() {
                    providers.push("deepseek".to_string());
                }
            }
            "openrouter" => {
                providers.push("openrouter".to_string());
                if self.config.has_openai_key() {
                    providers.push("openai".to_string());
                } else if self.config.has_deepseek_key() {
                    providers.push("deepseek".to_string());
                }
            }
            "deepseek" => {
                providers.push("deepseek".to_string());
                if self.config.has_openai_key() {
                    providers.push("openai".to_string());
                } else if self.config.has_openrouter_key() {
                    providers.push("openrouter".to_string());
                }
            }
            "codex" => {
                providers.push("codex".to_string());
                if self.config.has_openai_key() {
                    providers.push("openai".to_string());
                }
            }
            _ => {}
        }

        // Limit to ensemble_count
        let count = self.config.ensemble_count.unwrap_or(2).min(providers.len());
        providers.truncate(count);

        providers
    }

    /// Call a specific provider's reranker API
    fn call_provider_reranker(
        &self,
        provider: &str,
        task: &str,
        candidates: &[ScoredFile],
        codebase_path: &std::path::Path,
    ) -> Result<Vec<ScoredFile>> {
        let (client, key, base_url) = match provider {
            "openai" => {
                let client = self.clients.get("openai").ok_or_else(|| anyhow::anyhow!("No OpenAI client"))?;
                let key = self.config.openai_key.as_deref().ok_or_else(|| anyhow::anyhow!("No OpenAI key"))?;
                let base_url = self.config.openai_base_url.as_deref().unwrap_or("https://api.openai.com");
                (client, key, base_url)
            }
            "openrouter" => {
                let client = self.clients.get("openrouter").ok_or_else(|| anyhow::anyhow!("No OpenRouter client"))?;
                let key = self.config.openrouter_key.as_deref().ok_or_else(|| anyhow::anyhow!("No OpenRouter key"))?;
                let base_url = self.config.openrouter_base_url.as_deref().unwrap_or("https://openrouter.ai");
                (client, key, base_url)
            }
            "deepseek" => {
                let client = self.clients.get("deepseek").ok_or_else(|| anyhow::anyhow!("No DeepSeek client"))?;
                let key = self.config.deepseek_key.as_deref().ok_or_else(|| anyhow::anyhow!("No DeepSeek key"))?;
                let base_url = self.config.deepseek_base_url.as_deref().unwrap_or("https://api.deepseek.com");
                (client, key, base_url)
            }
            "codex" => {
                let client = self.clients.get("codex").ok_or_else(|| anyhow::anyhow!("No Codex client"))?;
                let token = self.get_codex_token().ok_or_else(|| anyhow::anyhow!("No Codex token"))?;
                return self.call_codex_reranker_api(client, &token, task, candidates, codebase_path);
            }
            _ => anyhow::bail!("Unknown provider: {}", provider),
        };

        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        self.call_reranker_api(client, key, &url, task, candidates, codebase_path)
    }

    /// Get Codex OAuth token from file
    fn get_codex_token(&self) -> Option<String> {
        if let Some(ref key) = self.config.codex_key {
            if !key.is_empty() {
                return Some(key.clone());
            }
        }
        let home = std::env::var("HOME").ok()?;
        let token_path = std::path::PathBuf::from(home).join(".hermes/auth/openai-codex-oauth-1.json");
        let content = std::fs::read_to_string(token_path).ok()?;
        let auth: CodexAuth = serde_json::from_str(&content).ok()?;
        auth.token.or(auth.access_token)
    }

    fn call_reranker_api(
        &self,
        client: &reqwest::blocking::Client,
        key: &str,
        url: &str,
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

        let model = self.config.reranker_model_name();

        let body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "response_format": { "type": "json_object" },
            "temperature": 0.1,
            "max_tokens": 2000,
        });

        let resp = client
            .post(url)
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

    /// Call Codex reranker API (uses GitHub Copilot endpoint)
    fn call_codex_reranker_api(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        task: &str,
        candidates: &[ScoredFile],
        codebase_path: &std::path::Path,
    ) -> Result<Vec<ScoredFile>> {
        let url = "https://api.githubcopilot.com/v1/chat/completions";

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

        let model = self.config.reranker_model_name();

        let body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ],
            "response_format": { "type": "json_object" },
            "temperature": 0.1,
            "max_tokens": 2000,
        });

        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&body)
            .send()
            .context("Codex reranker API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text_body = resp.text().unwrap_or_default();
            anyhow::bail!("Codex reranker API error {}: {}", status, text_body);
        }

        let parsed: ChatResponse = resp.json().context("Failed to parse Codex reranker response")?;

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
