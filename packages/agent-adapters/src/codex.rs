//! ADT-002, ADT-003, ADT-006: Codex CLI adapter.
//!
//! Wraps the `codex` CLI tool as a governed adapter.
//! Runs `codex exec --skip-git-repo-check "<prompt>"` as a subprocess,
//! capturing stdout/stderr, enforcing UTF-8, handling timeouts, and
//! retrying once on empty output.
//!
//! The `--skip-git-repo-check` flag is required for codex to operate
//! inside dynamically created git worktrees. Output is parsed to
//! extract the actual response content from the multi-section format.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::process::Command;

use crate::adapter::{
    AdapterProvenance, AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind,
};
use crate::normalize::{
    extract_codex_exec_content, normalize_output, NormalizationPolicy, NormalizationResult,
};

/// Codex-specific request configuration layered on top of AdapterInput.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexRequestConfig {
    /// Model name for the Codex API (e.g. "codex-mini-latest").
    pub model: String,
    /// Working directory for the Codex session.
    pub working_directory: String,
    /// Whether to enable full-auto mode (requires explicit policy approval).
    pub full_auto: bool,
    /// Environment variables to pass (explicit, never implicit).
    pub env_vars: Vec<CodexEnvVar>,
}

/// An explicit environment variable passed to the Codex process.
/// No implicit env-only behavior: every variable must be declared here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexEnvVar {
    pub name: String,
    pub value: String,
    /// Whether this variable contains sensitive data (masked in logs).
    pub sensitive: bool,
}

/// Codex-specific response metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexResponseMeta {
    /// Files modified by the Codex session.
    pub modified_files: Vec<String>,
    /// Commands executed (for audit trail, no silent shell hacks).
    pub commands_executed: Vec<String>,
    /// Whether the session completed successfully.
    pub session_success: bool,
}

/// Adapter that invokes the `codex` CLI as a subprocess.
pub struct CodexCliAdapter {
    /// Path to the codex CLI binary (default: "codex").
    pub cli_path: String,
}

impl CodexCliAdapter {
    /// Create a new adapter using the default `codex` binary on PATH.
    pub fn new() -> Self {
        Self {
            cli_path: "codex".to_string(),
        }
    }

    /// Create a new adapter using a specific path to the codex binary.
    pub fn with_path(path: String) -> Self {
        Self { cli_path: path }
    }

    /// Run the CLI once and return raw output or an error description.
    async fn run_once(
        &self,
        request: &AdapterRequest,
        timeout: Duration,
    ) -> Result<(String, String, Option<i32>), String> {
        // On Windows, npm-installed CLIs are .cmd scripts that need cmd.exe.
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", &self.cli_path, "exec", "--skip-git-repo-check", &request.prompt]);
            c
        } else {
            let mut c = Command::new(&self.cli_path);
            c.args(["exec", "--skip-git-repo-check", &request.prompt]);
            c
        };
        cmd.current_dir(&request.working_directory);

        let result = tokio::time::timeout(timeout, cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let code = output.status.code();
                Ok((stdout, stderr, code))
            }
            Ok(Err(io_err)) => Err(format!("IO error launching codex CLI: {io_err}")),
            Err(_elapsed) => Err("timeout".to_string()),
        }
    }
}

impl Default for CodexCliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentAdapter for CodexCliAdapter {
    fn name(&self) -> &str {
        "codex-cli"
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::Codex
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

                    let normalized = normalize_output(&stdout_raw, &policy);
                    if normalized.result == NormalizationResult::Empty {
                        if attempt_num < MAX_RETRIES {
                            tracing::warn!(
                                adapter = "codex-cli",
                                task_id = %task_id,
                                attempt = attempt_num + 1,
                                max_retries = MAX_RETRIES,
                                "Empty output from codex CLI, retrying"
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

        // Extract response content from codex exec multi-section output.
        // Codex exec produces structured output with a "codex" section header,
        // the actual response, and then metadata lines (model, tokens used, etc.).
        let extracted_content = extract_codex_exec_content(&stdout_raw);

        // Normalize the extracted content.
        let normalized = normalize_output(&extracted_content, &policy);

        AdapterResponse {
            task_id,
            status,
            output: normalized.content,
            stdout: stdout_raw,
            stderr: stderr_raw,
            duration_ms,
            token_usage: None,
            artifacts: Vec::new(),
            provenance: AdapterProvenance {
                adapter_name: "codex-cli".to_string(),
                model_used: request.model.unwrap_or_else(|| "gpt-5.4".to_string()),
                provider: "openai".to_string(),
                invocation_id,
                started_at: started_at.to_rfc3339(),
                finished_at: finished_at.to_rfc3339(),
            },
        }
    }
}

pub type CodexAdapter = CodexCliAdapter;
