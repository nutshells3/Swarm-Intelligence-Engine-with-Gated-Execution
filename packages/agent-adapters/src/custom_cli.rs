//! ADT-016: Custom CLI adapter.
//!
//! Reads the CLI binary path from the `SWARM_CUSTOM_CLI` environment variable.
//! Invokes the binary with the prompt on stdin, captures stdout/stderr,
//! enforces UTF-8, handles timeouts, and retries up to 2 times on empty output.
//!
//! Implements the `AgentAdapter` trait so it integrates with the adapter registry
//! and is auto-detected during `AdapterRegistry::auto_detect()`.

use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::adapter::{
    AdapterProvenance, AdapterRequest, AdapterResponse, AdapterStatus, AgentAdapter, AgentKind,
};
use crate::normalize::{normalize_output, NormalizationPolicy, NormalizationResult};
use crate::spawn::PromptDeliveryMode;

/// Adapter that invokes a user-specified CLI tool as a subprocess.
///
/// The CLI path is read from `SWARM_CUSTOM_CLI` at construction time.
/// The prompt delivery method is determined by `PromptDeliveryMode`:
///
/// - `StdinPipe` (default): pipe the prompt to the process's stdin.
/// - `CommandLineArg`: pass the prompt as `--prompt "..."` CLI argument.
/// - `TempFile`: write the prompt to a temporary file and pass the path
///   as `--prompt-file <path>`.
pub struct CustomCliAdapter {
    /// Absolute or PATH-relative path to the custom CLI binary.
    pub cli_path: String,
    /// How the prompt is delivered to the CLI process.
    pub prompt_delivery: PromptDeliveryMode,
}

impl CustomCliAdapter {
    /// Create a new adapter for the given CLI binary path.
    ///
    /// Uses `StdinPipe` delivery by default.
    pub fn new(cli_path: String) -> Self {
        Self {
            cli_path,
            prompt_delivery: PromptDeliveryMode::default(),
        }
    }

    /// Create a new adapter with an explicit delivery mode.
    pub fn with_delivery(cli_path: String, delivery: PromptDeliveryMode) -> Self {
        Self {
            cli_path,
            prompt_delivery: delivery,
        }
    }

    /// Create from the `SWARM_CUSTOM_CLI` env var, returning `None` if unset or empty.
    ///
    /// The delivery mode can be set via `SWARM_CUSTOM_CLI_DELIVERY`:
    /// - `stdin`  (default) -> `StdinPipe`
    /// - `arg`              -> `CommandLineArg`
    /// - `tempfile`         -> `TempFile`
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("SWARM_CUSTOM_CLI").ok()?;
        if path.is_empty() {
            return None;
        }
        let delivery = match std::env::var("SWARM_CUSTOM_CLI_DELIVERY")
            .unwrap_or_default()
            .as_str()
        {
            "arg" => PromptDeliveryMode::CommandLineArg,
            "tempfile" => PromptDeliveryMode::TempFile,
            _ => PromptDeliveryMode::StdinPipe,
        };
        Some(Self::with_delivery(path, delivery))
    }

    /// Run the CLI once, delivering the prompt according to `self.prompt_delivery`.
    ///
    /// Dispatch by delivery mode:
    /// - `StdinPipe`:       pipe prompt to stdin (original behaviour).
    /// - `CommandLineArg`:  pass `--prompt "<prompt>"` on the command line.
    /// - `TempFile`:        write prompt to a temp file, pass `--prompt-file <path>`.
    async fn run_once(
        &self,
        request: &AdapterRequest,
        timeout: Duration,
    ) -> Result<(String, String, Option<i32>), String> {
        // Build the base command; delivery mode decides args and stdin.
        let mut cmd = Command::new(&self.cli_path);
        cmd.current_dir(&request.working_directory)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Prompt delivery mode handling.
        let _temp_file_guard: Option<tempfile::NamedTempFile>;
        match &self.prompt_delivery {
            PromptDeliveryMode::StdinPipe => {
                cmd.stdin(std::process::Stdio::piped());
                _temp_file_guard = None;
            }
            PromptDeliveryMode::CommandLineArg => {
                cmd.stdin(std::process::Stdio::null());
                cmd.arg("--prompt").arg(&request.prompt);
                _temp_file_guard = None;
            }
            PromptDeliveryMode::TempFile => {
                cmd.stdin(std::process::Stdio::null());
                // Write prompt to a temporary file that lives until this
                // invocation completes (the guard keeps it alive).
                let mut tmp = tempfile::NamedTempFile::new()
                    .map_err(|e| format!("Failed to create temp file for prompt: {e}"))?;
                std::io::Write::write_all(&mut tmp, request.prompt.as_bytes())
                    .map_err(|e| format!("Failed to write prompt to temp file: {e}"))?;
                let path_str = tmp.path().to_string_lossy().into_owned();
                cmd.arg("--prompt-file").arg(&path_str);
                _temp_file_guard = Some(tmp);
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("IO error launching custom CLI '{}': {e}", self.cli_path))?;

        // For StdinPipe mode, write prompt to stdin and close it.
        if self.prompt_delivery == PromptDeliveryMode::StdinPipe {
            if let Some(mut stdin) = child.stdin.take() {
                let prompt_bytes = request.prompt.as_bytes().to_vec();
                // Fire-and-forget write; if the process exits early it will error
                // but we still want to collect whatever output it produced.
                let _ = tokio::time::timeout(Duration::from_secs(5), async move {
                    let _ = stdin.write_all(&prompt_bytes).await;
                    let _ = stdin.shutdown().await;
                })
                .await;
            }
        }

        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let code = output.status.code();
                Ok((stdout, stderr, code))
            }
            Ok(Err(io_err)) => Err(format!(
                "IO error waiting for custom CLI '{}': {io_err}",
                self.cli_path
            )),
            Err(_elapsed) => {
                // child was consumed by wait_with_output(); the future being
                // dropped will cause the child process to be cleaned up by the OS.
                Err("timeout".to_string())
            }
        }
    }
}

impl AgentAdapter for CustomCliAdapter {
    fn name(&self) -> &str {
        "custom-cli"
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::GenericCli
    }

    async fn invoke(&self, request: AdapterRequest) -> AdapterResponse {
        let start = Instant::now();
        let started_at = chrono::Utc::now();
        let invocation_id = uuid::Uuid::now_v7().to_string();
        let timeout = Duration::from_secs(request.timeout_seconds);
        let task_id = request.task_id.clone();
        let policy = NormalizationPolicy::default();

        // Up to 2 retries on empty output (3 total attempts), same as codex/claude.
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
                                adapter = "custom-cli",
                                task_id = %task_id,
                                cli_path = %self.cli_path,
                                attempt = attempt_num + 1,
                                max_retries = MAX_RETRIES,
                                "Empty output from custom CLI, retrying"
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
                adapter_name: "custom-cli".to_string(),
                model_used: self.cli_path.clone(),
                provider: "custom".to_string(),
                invocation_id,
                started_at: started_at.to_rfc3339(),
                finished_at: finished_at.to_rfc3339(),
            },
        }
    }
}
