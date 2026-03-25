//! OpenAI Chat Completions API adapter.
//!
//! Calls the OpenAI REST API (POST /v1/chat/completions) directly using
//! reqwest. Enforces UTF-8, handles timeouts, retries once on empty output,
//! and captures full provenance including token usage.

use std::time::{Duration, Instant};

use crate::adapter::{
    AdapterProvenance, AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind,
    TokenUsage,
};
use crate::normalize::{normalize_output, NormalizationPolicy, NormalizationResult};

/// Adapter that calls the OpenAI Chat Completions API over HTTPS.
pub struct OpenAiApiAdapter {
    /// OpenAI API key (from OPENAI_API_KEY env var).
    pub api_key: String,
    /// Default model to use (e.g. "gpt-4o").
    pub model: String,
    /// Base URL for the API (default: "https://api.openai.com").
    pub base_url: String,
}

impl OpenAiApiAdapter {
    /// Create with the given API key and default settings.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "gpt-4o".to_string(),
            base_url: "https://api.openai.com".to_string(),
        }
    }

    /// Create with custom model and base URL (e.g. for Azure OpenAI).
    pub fn with_config(api_key: String, model: String, base_url: String) -> Self {
        Self {
            api_key,
            model,
            base_url,
        }
    }

    /// Make a single API call and return the raw response body or an error.
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
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                    .unwrap_or_else(|e| format!("Failed to read response body: {e}"));
                if status_code.is_success() {
                    Ok(body_text)
                } else if status_code.as_u16() == 429 {
                    Err(format!("rate_limited: {body_text}"))
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

impl AgentAdapter for OpenAiApiAdapter {
    fn name(&self) -> &str {
        "openai-api"
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::HttpApi
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

        // First attempt.
        let attempt = self.call_once(&request, timeout).await;

        let (raw_body, status, token_usage, output_text) = match attempt {
            Ok(body) => {
                let parsed = parse_openai_response(&body);
                let normalized = normalize_output(&parsed.text, &policy);

                if normalized.result == NormalizationResult::Empty {
                    // Retry once on empty output.
                    tracing::warn!(
                        adapter = "openai-api",
                        task_id = %task_id,
                        "Empty output from OpenAI API, retrying once"
                    );
                    let remaining = timeout.saturating_sub(start.elapsed());
                    if remaining.is_zero() {
                        (body, AdapterStatus::TimedOut, parsed.usage, parsed.text)
                    } else {
                        match self.call_once(&request, remaining).await {
                            Ok(body2) => {
                                let p2 = parse_openai_response(&body2);
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
                            Err(e) => {
                                (e, AdapterStatus::Failed, None, String::new())
                            }
                        }
                    }
                } else {
                    (body, AdapterStatus::Succeeded, parsed.usage, parsed.text)
                }
            }
            Err(e) if e == "timeout" => {
                (String::new(), AdapterStatus::TimedOut, None, String::new())
            }
            Err(e) if e.starts_with("rate_limited") => {
                (e, AdapterStatus::RetryableError, None, String::new())
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
                adapter_name: "openai-api".to_string(),
                model_used,
                provider: "openai".to_string(),
                invocation_id,
                started_at: started_at.to_rfc3339(),
                finished_at: finished_at.to_rfc3339(),
            },
        }
    }
}

struct ParsedOpenAiResponse {
    text: String,
    usage: Option<TokenUsage>,
}

fn parse_openai_response(body: &str) -> ParsedOpenAiResponse {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            return ParsedOpenAiResponse {
                text: body.to_string(),
                usage: None,
            };
        }
    };

    // Extract text from choices[0].message.content.
    let text = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("content"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    // Extract token usage.
    let usage = v.get("usage").and_then(|u| {
        let input = u.get("prompt_tokens")?.as_u64()? as u32;
        let output = u.get("completion_tokens")?.as_u64()? as u32;
        Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_tokens: None,
        })
    });

    ParsedOpenAiResponse { text, usage }
}
