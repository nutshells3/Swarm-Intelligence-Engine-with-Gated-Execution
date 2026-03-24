//! Local model adapter (vLLM, ollama, llama.cpp, etc.)
//!
//! Any OpenAI-compatible local server works. Detects:
//! - ollama:  http://localhost:11434/v1
//! - vLLM:    http://localhost:8000/v1
//! - custom:  SWARM_LOCAL_MODEL_URL env var

use crate::adapter::{
    AdapterProvenance, AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind,
    TokenUsage,
};
use crate::normalize::{normalize_output, NormalizationPolicy, NormalizationResult};
use std::time::{Duration, Instant};

/// Adapter for local OpenAI-compatible model servers.
pub struct LocalModelAdapter {
    /// Base URL (e.g. "http://localhost:11434/v1" for ollama).
    pub base_url: String,
    /// Default model name.
    pub model: String,
    /// Display name for provenance.
    pub provider_name: String,
}

impl LocalModelAdapter {
    /// Create for ollama (default model: llama3.1).
    pub fn ollama() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.1".to_string(),
            provider_name: "ollama".to_string(),
        }
    }

    /// Create for vLLM.
    pub fn vllm(model: String) -> Self {
        Self {
            base_url: "http://localhost:8000/v1".to_string(),
            model,
            provider_name: "vllm".to_string(),
        }
    }

    /// Create with custom URL and model.
    pub fn custom(base_url: String, model: String, provider_name: String) -> Self {
        Self {
            base_url,
            model,
            provider_name,
        }
    }

    async fn call_once(
        &self,
        request: &AdapterRequest,
        timeout: Duration,
    ) -> Result<String, String> {
        let client = reqwest::Client::new();
        let model = request.model.as_deref().unwrap_or(&self.model);
        let max_tokens = request.max_tokens.unwrap_or(4096);

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [{"role": "user", "content": &request.prompt}]
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let result = client
            .post(format!("{}/chat/completions", self.base_url))
            .header("content-type", "application/json")
            .json(&body)
            .timeout(timeout)
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status_code = resp.status();
                let body_text = resp
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("Failed to read body: {e}"));
                if status_code.is_success() {
                    Ok(body_text)
                } else {
                    Err(format!("HTTP {status_code}: {body_text}"))
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    Err("timeout".to_string())
                } else {
                    Err(format!("Request error: {e}"))
                }
            }
        }
    }
}

impl AgentAdapter for LocalModelAdapter {
    fn name(&self) -> &str {
        // Return a static-ish name. We leak the string to get 'static lifetime.
        // This is fine because adapters are long-lived singletons.
        let name = format!("local-{}", self.provider_name);
        Box::leak(name.into_boxed_str())
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::Local
    }

    async fn invoke(&self, request: AdapterRequest) -> AdapterResponse {
        let start = Instant::now();
        let started_at = chrono::Utc::now();
        let invocation_id = uuid::Uuid::now_v7().to_string();
        let timeout = Duration::from_secs(request.timeout_seconds);
        let task_id = request.task_id.clone();
        let model_used = request
            .model
            .clone()
            .unwrap_or_else(|| self.model.clone());
        let policy = NormalizationPolicy::default();

        let attempt = self.call_once(&request, timeout).await;

        let (raw_body, status, token_usage, output_text) = match attempt {
            Ok(body) => {
                let parsed = parse_local_response(&body);
                let normalized = normalize_output(&parsed.text, &policy);

                if normalized.result == NormalizationResult::Empty {
                    tracing::warn!(
                        adapter = %self.provider_name,
                        task_id = %task_id,
                        "Empty output, retrying once"
                    );
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        (body, AdapterStatus::TimedOut, parsed.usage, parsed.text)
                    } else {
                        match self.call_once(&request, remaining).await {
                            Ok(body2) => {
                                let p2 = parse_local_response(&body2);
                                let n2 = normalize_output(&p2.text, &policy);
                                let st = if n2.result == NormalizationResult::Empty {
                                    AdapterStatus::EmptyOutput
                                } else {
                                    AdapterStatus::Succeeded
                                };
                                (body2, st, p2.usage, p2.text)
                            }
                            Err(e) if e == "timeout" => {
                                (body, AdapterStatus::TimedOut, parsed.usage, parsed.text)
                            }
                            Err(e) => (e, AdapterStatus::Failed, None, String::new()),
                        }
                    }
                } else {
                    (body, AdapterStatus::Succeeded, parsed.usage, parsed.text)
                }
            }
            Err(e) if e == "timeout" => {
                (String::new(), AdapterStatus::TimedOut, None, String::new())
            }
            Err(e) => (e, AdapterStatus::Failed, None, String::new()),
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let finished_at = chrono::Utc::now();

        AdapterResponse {
            task_id,
            status,
            output: output_text,
            stdout: raw_body,
            stderr: String::new(),
            duration_ms,
            token_usage,
            artifacts: Vec::new(),
            provenance: AdapterProvenance {
                adapter_name: format!("local-{}", self.provider_name),
                model_used,
                provider: self.provider_name.clone(),
                invocation_id,
                started_at: started_at.to_rfc3339(),
                finished_at: finished_at.to_rfc3339(),
            },
        }
    }
}

struct ParsedLocalResponse {
    text: String,
    usage: Option<TokenUsage>,
}

fn parse_local_response(body: &str) -> ParsedLocalResponse {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            return ParsedLocalResponse {
                text: body.to_string(),
                usage: None,
            };
        }
    };

    let text = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    let usage = v.get("usage").and_then(|u| {
        let input = u.get("prompt_tokens")?.as_u64()? as u32;
        let output = u.get("completion_tokens")?.as_u64()? as u32;
        Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_tokens: None,
        })
    });

    ParsedLocalResponse { text, usage }
}
