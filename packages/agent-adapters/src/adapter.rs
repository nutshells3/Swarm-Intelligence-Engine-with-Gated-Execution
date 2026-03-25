//! ADT-001: External agent adapter trait interface.
//!
//! CSV guardrail: "adapter contract validation; UTF-8 I/O check;
//!   durable stdio capture check; empty-output retry simulation"
//! Caution: No silent shell hacks; no implicit env-only behavior.
//!
//! Every external coding/chat agent is wrapped in a governed adapter that
//! enforces UTF-8 I/O, captures durable provenance, and never silently
//! shells out.

use serde::{Deserialize, Serialize};

/// The kind of external agent being adapted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// OpenAI Codex-style coding agent.
    Codex,
    /// Anthropic Claude-style coding/chat agent.
    Claude,
    /// Generic coding agent with stdio interface.
    GenericCli,
    /// HTTP API-based agent.
    HttpApi,
    /// Local model server (vLLM, ollama, llama.cpp, etc.)
    Local,
}

/// Input payload sent to an external agent via the adapter.
/// All content must be valid UTF-8 (enforced at the adapter boundary).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterInput {
    /// Unique invocation identifier for provenance tracking.
    pub invocation_id: String,
    /// Task identifier linking to the control plane.
    pub task_id: String,
    /// The prompt or instruction sent to the agent.
    pub prompt: String,
    /// Context files or references (all UTF-8).
    pub context_refs: Vec<String>,
    /// Timeout in seconds for this invocation.
    pub timeout_seconds: u32,
    /// Maximum output tokens to request.
    pub max_output_tokens: u32,
}

/// Output payload received from an external agent via the adapter.
/// The adapter validates UTF-8 compliance before returning this.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterOutput {
    /// Matching invocation identifier.
    pub invocation_id: String,
    /// The agent's response content (guaranteed UTF-8).
    pub content: String,
    /// Whether the output was truncated.
    pub truncated: bool,
    /// Exit code for CLI-based agents (None for API agents).
    pub exit_code: Option<i32>,
    /// Raw stderr capture for debugging (guaranteed UTF-8).
    pub stderr_capture: Option<String>,
    /// Duration of the invocation in milliseconds.
    pub duration_ms: u64,
}

/// Error returned when an adapter invocation fails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterError {
    pub invocation_id: String,
    pub kind: AdapterErrorKind,
    pub message: String,
    pub retryable: bool,
}

/// Classification of adapter errors.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterErrorKind {
    /// Agent process timed out.
    Timeout,
    /// Agent returned non-UTF-8 output.
    Utf8Violation,
    /// Agent returned empty output.
    EmptyOutput,
    /// Agent process crashed or returned non-zero exit code.
    ProcessFailure,
    /// Network or API error.
    NetworkError,
    /// Rate limit or quota exceeded.
    RateLimited,
    /// Agent returned output that failed schema validation.
    SchemaViolation,
    /// Unknown or unclassified error.
    Unknown,
}

/// Unified request sent to any adapter (CLI or API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRequest {
    /// Task identifier linking to the control plane.
    pub task_id: String,
    /// The prompt or instruction sent to the agent.
    pub prompt: String,
    /// Context files or references (all UTF-8).
    pub context_files: Vec<String>,
    /// Working directory for CLI-based agents.
    pub working_directory: String,
    /// Model override (adapter-specific default used when None).
    pub model: Option<String>,
    /// Provider mode: "cli", "api", or "local".
    pub provider_mode: String,
    /// Timeout in seconds for this invocation.
    pub timeout_seconds: u64,
    /// Maximum output tokens to request.
    pub max_tokens: Option<u32>,
    /// Sampling temperature (API adapters only).
    pub temperature: Option<f64>,
}

/// Unified response from any adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterResponse {
    /// Task identifier echoed back.
    pub task_id: String,
    /// Outcome status of the invocation.
    pub status: AdapterStatus,
    /// Primary output content (guaranteed UTF-8).
    pub output: String,
    /// Raw stdout capture (CLI adapters).
    pub stdout: String,
    /// Raw stderr capture (CLI adapters).
    pub stderr: String,
    /// Duration of the invocation in milliseconds.
    pub duration_ms: u64,
    /// Token usage (API adapters; None for CLI).
    pub token_usage: Option<TokenUsage>,
    /// File paths or other artifacts produced.
    pub artifacts: Vec<String>,
    /// Provenance metadata for this invocation.
    pub provenance: AdapterProvenance,
}

/// Outcome status of an adapter invocation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterStatus {
    /// Agent completed successfully with output.
    Succeeded,
    /// Agent process or API call failed.
    Failed,
    /// Invocation timed out.
    TimedOut,
    /// Agent returned empty output.
    EmptyOutput,
    /// Agent output could not be parsed.
    MalformedOutput,
    /// Transient error that may succeed on retry.
    RetryableError,
}

/// Token usage reported by API-based adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_tokens: Option<u32>,
}

/// Provenance metadata captured for every invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterProvenance {
    /// Name of the adapter that handled this invocation.
    pub adapter_name: String,
    /// Model that was used.
    pub model_used: String,
    /// Provider identifier (e.g. "anthropic", "openai", "local").
    pub provider: String,
    /// Unique identifier for this invocation.
    pub invocation_id: String,
    /// ISO-8601 timestamp when the invocation started.
    pub started_at: String,
    /// ISO-8601 timestamp when the invocation finished.
    pub finished_at: String,
}

/// The core async adapter trait that all external agent adapters implement.
///
/// Implementors wrap a specific external agent (Codex, Claude, APIs, etc.) and
/// enforce the governance contract:
/// 1. All I/O is valid UTF-8.
/// 2. Every invocation produces provenance metadata.
/// 3. Empty output triggers a single retry (not silent swallowing).
/// 4. No silent shell hacks -- all subprocess calls are explicit.
///
/// The trait uses `impl Future` return types so it can be object-safe via
/// boxing at the registry level.
pub trait AgentAdapter: Send + Sync {
    /// Returns the adapter's display name.
    fn name(&self) -> &str;

    /// Returns the kind of agent this adapter wraps.
    fn agent_kind(&self) -> AgentKind;

    /// Invoke the external agent with the given request.
    ///
    /// The adapter MUST:
    /// - Validate UTF-8 compliance of all output.
    /// - Handle timeouts via tokio::time::timeout.
    /// - Retry once on empty output.
    /// - Populate provenance metadata in the response.
    fn invoke(
        &self,
        request: AdapterRequest,
    ) -> impl std::future::Future<Output = AdapterResponse> + Send;
}
