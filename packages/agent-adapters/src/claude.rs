//! ADT-004, ADT-005, ADT-007: Claude CLI adapter.
//!
//! Wraps the `claude` CLI tool (Claude Code) as a governed adapter.
//! Runs `claude --print --output-format json "<prompt>"` as a subprocess,
//! capturing stdout/stderr, enforcing UTF-8, handling timeouts, and
//! retrying once on empty output.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::process::Command;

use crate::adapter::{
    AdapterProvenance, AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind,
    TokenUsage,
};
use crate::normalize::{normalize_output, NormalizationPolicy, NormalizationResult};

/// Claude-specific request configuration layered on top of AdapterInput.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeRequestConfig {
    /// Model name (e.g. "claude-sonnet-4-20250514").
    pub model: String,
    /// System instruction prepended to every prompt.
    pub system_instruction: Option<String>,
    /// Whether to use extended thinking.
    pub extended_thinking: bool,
    /// Maximum tokens for the thinking budget (when extended_thinking is true).
    pub thinking_budget_tokens: Option<u32>,
    /// Allowed tools/commands the Claude session may use.
    pub allowed_tools: Vec<String>,
    /// Working directory for Claude Code sessions.
    pub working_directory: Option<String>,
}

/// Stop reason reported by the Claude API.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeStopReason {
    /// Normal completion.
    EndTurn,
    /// Hit the max_tokens limit.
    MaxTokens,
    /// Stopped by a stop sequence.
    StopSequence,
    /// Stopped by a tool use.
    ToolUse,
}

/// Claude-specific response metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeResponseMeta {
    /// Why the response stopped.
    pub stop_reason: ClaudeStopReason,
    /// Input tokens consumed.
    pub input_tokens: u32,
    /// Output tokens produced.
    pub output_tokens: u32,
    /// Files modified during the session (for Claude Code).
    pub modified_files: Vec<String>,
    /// Commands executed during the session (explicit audit trail).
    pub commands_executed: Vec<String>,
}

/// Adapter that invokes the `claude` CLI (Claude Code) as a subprocess.
pub struct ClaudeCliAdapter {
    /// Path to the claude CLI binary (default: "claude").
    pub cli_path: String,
}

impl ClaudeCliAdapter {
    /// Create a new adapter using the default `claude` binary on PATH.
    pub fn new() -> Self {
        Self {
            cli_path: "claude".to_string(),
        }
    }

    /// Create a new adapter using a specific path to the claude binary.
    pub fn with_path(path: String) -> Self {
        Self { cli_path: path }
    }

    /// Run the CLI once and return raw output or an error description.
    async fn run_once(
        &self,
        request: &AdapterRequest,
        timeout: Duration,
    ) -> Result<(String, String, Option<i32>), String> {
        // On Windows, CLI tools may be .cmd/.ps1 scripts that need cmd.exe.
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", &self.cli_path, "--print", "--output-format", "json", &request.prompt]);
            c
        } else {
            let mut c = Command::new(&self.cli_path);
            c.args(["--print", "--output-format", "json", &request.prompt]);
            c
        };
        cmd.current_dir(&request.working_directory)
            .env("CLAUDE_NO_INTERACTIVE", "1");

        let result = tokio::time::timeout(timeout, cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let code = output.status.code();
                Ok((stdout, stderr, code))
            }
            Ok(Err(io_err)) => Err(format!("IO error launching claude CLI: {io_err}")),
            Err(_elapsed) => Err("timeout".to_string()),
        }
    }
}

impl Default for ClaudeCliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentAdapter for ClaudeCliAdapter {
    fn name(&self) -> &str {
        "claude-cli"
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    async fn invoke(&self, request: AdapterRequest) -> AdapterResponse {
        let start = Instant::now();
        let started_at = chrono::Utc::now();
        let invocation_id = uuid::Uuid::now_v7().to_string();
        let timeout = Duration::from_secs(request.timeout_seconds);
        let task_id = request.task_id.clone();
        let policy = NormalizationPolicy::default();

        // Up to 2 retries on empty output (3 total attempts).
        const MAX_RETRIES: u32 = 2;

        let mut stdout_raw = String::new();
        let mut stderr_raw = String::new();
        let mut status = AdapterStatus::Failed;

        for attempt_num in 0..=MAX_RETRIES {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                status = AdapterStatus::TimedOut;
                break;
            }

            match self.run_once(&request, remaining).await {
                Ok((stdout, stderr, code)) => {
                    stdout_raw = stdout;
                    stderr_raw = stderr;

                    // Check for is_error in JSON response.
                    // Claude CLI returns exit code 0 even when credit is low
                    // or other errors occur, but sets is_error: true in JSON.
                    let json_is_error = serde_json::from_str::<serde_json::Value>(&stdout_raw)
                        .ok()
                        .and_then(|v| v.get("is_error")?.as_bool())
                        .unwrap_or(false);

                    if json_is_error {
                        // Extract error message from result field for explicit surfacing
                        let error_detail = serde_json::from_str::<serde_json::Value>(&stdout_raw)
                            .ok()
                            .and_then(|v| {
                                v.get("result")
                                    .and_then(|r| r.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| "unknown error (is_error=true)".to_string());
                        tracing::error!(
                            adapter = "claude-cli",
                            task_id = %task_id,
                            error_detail = %error_detail,
                            stderr = %stderr_raw,
                            "Claude CLI returned is_error=true despite exit code 0"
                        );
                        // Surface the error explicitly -- no silent fallback
                        stderr_raw = format!("{}\nClaude CLI is_error: {}", stderr_raw, error_detail);
                        status = AdapterStatus::Failed;
                        break;
                    }

                    let normalized = normalize_output(&stdout_raw, &policy);
                    if normalized.result == NormalizationResult::Empty {
                        if attempt_num < MAX_RETRIES {
                            tracing::warn!(
                                adapter = "claude-cli",
                                task_id = %task_id,
                                attempt = attempt_num + 1,
                                max_retries = MAX_RETRIES,
                                "Empty output from claude CLI, retrying"
                            );
                            continue;
                        }
                        status = AdapterStatus::EmptyOutput;
                    } else if code == Some(0) || code.is_none() {
                        status = AdapterStatus::Succeeded;
                    } else {
                        status = AdapterStatus::Failed;
                    }
                    break;
                }
                Err(e) if e == "timeout" => {
                    status = AdapterStatus::TimedOut;
                    break;
                }
                Err(e) => {
                    stderr_raw = e;
                    status = AdapterStatus::Failed;
                    break;
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let finished_at = chrono::Utc::now();

        // Normalize final output.
        let normalized = normalize_output(&stdout_raw, &policy);

        // Try to extract token usage from JSON output.
        let token_usage = serde_json::from_str::<serde_json::Value>(&stdout_raw)
            .ok()
            .and_then(|v| {
                let input = v.get("usage")?.get("input_tokens")?.as_u64()? as u32;
                let output = v.get("usage")?.get("output_tokens")?.as_u64()? as u32;
                let cache = v
                    .get("usage")
                    .and_then(|u| u.get("cache_tokens"))
                    .and_then(|c| c.as_u64())
                    .map(|c| c as u32);
                Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    cache_tokens: cache,
                })
            });

        // Extract the primary text content from JSON if possible.
        let output_text = serde_json::from_str::<serde_json::Value>(&stdout_raw)
            .ok()
            .and_then(|v| {
                // Claude CLI JSON output: { "result": "...", ... }
                v.get("result")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or(normalized.content);

        AdapterResponse {
            task_id,
            status,
            output: output_text,
            stdout: stdout_raw,
            stderr: stderr_raw,
            duration_ms,
            token_usage,
            artifacts: Vec::new(),
            provenance: AdapterProvenance {
                adapter_name: "claude-cli".to_string(),
                model_used: request.model.unwrap_or_else(|| "claude-default".to_string()),
                provider: "anthropic".to_string(),
                invocation_id,
                started_at: started_at.to_rfc3339(),
                finished_at: finished_at.to_rfc3339(),
            },
        }
    }
}

pub type ClaudeAdapter = ClaudeCliAdapter;
