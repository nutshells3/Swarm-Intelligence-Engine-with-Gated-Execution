//! Worker protocol -- schema definitions for the protocol boundary between
//! workers and the control plane (WPR-001 through WPR-015).
//!
//! This crate defines the typed, versioned envelope that workers and the
//! control plane agree on for task dispatch, progress reporting, cancellation,
//! artifact references, and lifecycle governance.
//!
//! Key design rules:
//! - The worker protocol is *not* a hidden second control plane.
//! - Every envelope carries a ProtocolVersion for negotiation.
//! - All payloads are strongly typed enums/structs, never raw strings.

use serde::{Deserialize, Serialize};

/// Protocol version for envelope negotiation.
/// Major-breaking changes increment major; additive changes increment minor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self { major: 1, minor: 0 }
    }
}

/// Declared capability of a worker, sent during registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerCapability {
    /// Task kinds this worker can handle (e.g. "implementation", "review").
    pub accepted_task_kinds: Vec<String>,
    /// Maximum concurrent tasks.
    pub max_concurrency: u32,
    /// Maximum context tokens the worker can accept.
    pub max_context_tokens: u32,
    /// Whether this worker supports streaming progress events.
    pub supports_streaming: bool,
    /// Whether this worker supports cancellation.
    pub supports_cancel: bool,
}

/// Provider mode for task execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    Api,
    Session,
    Local,
}

/// Model binding specifying which AI model to use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelBinding {
    pub provider_name: Option<String>,
    pub model_name: Option<String>,
    pub reasoning_effort: Option<String>,
}

/// A caution attached to a task request, surfaced from CSV guardrails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCaution {
    /// Caution identifier (e.g. "no_silent_shell_hacks").
    pub caution_id: String,
    /// Human-readable description.
    pub description: String,
}

/// Task request dispatched from control plane to worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRequest {
    pub task_id: String,
    pub node_id: String,
    pub worker_role: String,
    pub skill_pack_id: String,
    pub artifact_refs: Vec<String>,
    pub summary: String,
    /// Protocol version for this envelope.
    pub protocol_version: ProtocolVersion,
    /// Model binding for this task.
    pub model_binding: ModelBinding,
    /// Provider mode override.
    pub provider_mode: Option<ProviderMode>,
    /// Timeout in seconds for this task.
    pub timeout_seconds: u32,
    /// Maximum retry attempts.
    pub retry_budget: u32,
    /// Maximum input context tokens.
    pub context_budget: u32,
    /// Cautions that the worker must respect.
    pub cautions: Vec<TaskCaution>,
}

/// Classification of the artifact produced by a task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Source code file(s).
    Code,
    /// Test file(s).
    Test,
    /// Documentation.
    Documentation,
    /// Configuration file(s).
    Configuration,
    /// Review or certification report.
    Report,
    /// Mixed or unclassified output.
    Mixed,
}

/// Token usage statistics from a task execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

/// Task result returned from worker to control plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskResult {
    pub task_id: String,
    pub status: String,
    pub summary: String,
    pub warnings: Vec<String>,
    pub artifact_refs: Vec<String>,
    /// Protocol version for this envelope.
    pub protocol_version: ProtocolVersion,
    /// Wall-clock duration of task execution in milliseconds.
    pub duration_ms: u64,
    /// Token usage statistics.
    pub token_usage: Option<TokenUsage>,
    /// Classification of the artifact produced.
    pub artifact_kind: Option<ArtifactKind>,
}

/// Progress event streamed during task execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgressEvent {
    pub task_id: String,
    pub worker_id: String,
    /// Progress percentage (0-100).
    pub progress_percent: u8,
    /// Current execution phase.
    pub phase: String,
    /// Human-readable progress message.
    pub message: String,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
}

/// Reason for requesting cancellation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    /// User requested cancellation.
    UserRequested,
    /// Timeout threshold exceeded.
    Timeout,
    /// Policy violation detected.
    PolicyViolation,
    /// Superseded by a newer task.
    Superseded,
    /// Lease expired without renewal.
    LeaseExpired,
}

/// Cancel request from control plane to worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelRequest {
    pub task_id: String,
    pub reason: CancelReason,
    /// Human-readable explanation.
    pub message: String,
    /// Grace period in seconds before forced kill.
    pub grace_period_seconds: u32,
}

/// Kill request for immediate forced termination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KillRequest {
    pub task_id: String,
    pub worker_id: String,
    /// Reason for the forced kill.
    pub reason: CancelReason,
}

/// Typed artifact reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactReference {
    /// Unique artifact identifier.
    pub artifact_id: String,
    /// Kind of artifact.
    pub kind: ArtifactKind,
    /// Storage path or URI.
    pub uri: String,
    /// SHA-256 hash of the artifact content.
    pub content_hash: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// Severity of a worker warning.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WarningSeverity {
    /// Informational notice, no action required.
    Info,
    /// Something unexpected happened but execution continued.
    Warning,
    /// A significant issue that may affect output quality.
    Severe,
}

/// Structured warning payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarningPayload {
    pub task_id: String,
    pub severity: WarningSeverity,
    /// Warning code for programmatic handling.
    pub code: String,
    /// Human-readable warning message.
    pub message: String,
}

/// Classification of worker errors.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Transient infrastructure error (network, rate limit).
    Transient,
    /// Permanent error (invalid input, unsupported task).
    Permanent,
    /// Worker internal bug or panic.
    Internal,
    /// Policy violation (exceeded budget, forbidden operation).
    PolicyViolation,
    /// Timeout exceeded.
    Timeout,
}

/// Structured error payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorPayload {
    pub task_id: String,
    pub category: ErrorCategory,
    /// Error code for programmatic handling.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Whether this error is retryable.
    pub retryable: bool,
}

/// Retryable failure with retry metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryableFailure {
    pub task_id: String,
    pub error: ErrorPayload,
    /// Number of attempts already made.
    pub attempts_so_far: u32,
    /// Maximum attempts allowed (from retry_budget).
    pub max_attempts: u32,
    /// Suggested delay in milliseconds before next retry.
    pub suggested_backoff_ms: u64,
}

/// Timeout result with partial output capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeoutResult {
    pub task_id: String,
    /// How long the task ran before timeout (milliseconds).
    pub elapsed_ms: u64,
    /// The configured timeout threshold (seconds).
    pub timeout_seconds: u32,
    /// Partial output captured before timeout, if any.
    pub partial_output: Option<String>,
    /// Whether the task was cleanly cancelled or force-killed.
    pub clean_shutdown: bool,
}

/// The kind of policy that was violated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyKind {
    /// Context budget exceeded.
    ContextBudget,
    /// Token budget exceeded.
    TokenBudget,
    /// Forbidden operation attempted.
    ForbiddenOperation,
    /// Output format violation.
    OutputFormat,
    /// Concurrency limit exceeded.
    ConcurrencyLimit,
    /// Lease violation.
    LeaseViolation,
}

/// Policy violation report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyViolation {
    pub task_id: String,
    pub worker_id: String,
    /// Which policy was violated.
    pub policy_kind: PolicyKind,
    /// Human-readable description.
    pub description: String,
    /// Whether the violation is blocking (task must stop).
    pub blocking: bool,
}

/// Resource usage snapshot reported in a heartbeat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceUsage {
    /// Memory usage in megabytes.
    pub memory_mb: u32,
    /// CPU usage percentage (0-100).
    pub cpu_percent: u8,
    /// Tokens consumed so far in this task.
    pub tokens_consumed: u32,
}

/// Worker heartbeat with enhanced telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerHeartbeat {
    pub task_attempt_id: String,
    pub worker_id: String,
    pub status: String,
    pub progress_message: String,
    /// Progress percentage (0-100).
    pub progress_percent: u8,
    /// Current execution phase.
    pub phase: String,
    /// Resource usage snapshot.
    pub resource_usage: Option<ResourceUsage>,
}

/// The kind of message carried in a protocol envelope.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    TaskRequest,
    TaskResult,
    ProgressEvent,
    CancelRequest,
    KillRequest,
    Heartbeat,
    Warning,
    Error,
    PolicyViolation,
}

/// Versioned protocol envelope wrapping all messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolEnvelope {
    /// Protocol version for negotiation.
    pub version: ProtocolVersion,
    /// Kind of message in this envelope.
    pub kind: EnvelopeKind,
    /// Unique message identifier.
    pub message_id: String,
    /// Correlation ID linking related messages (e.g. request-response).
    pub correlation_id: Option<String>,
    /// Serialized payload (the inner message).
    pub payload: serde_json::Value,
    /// ISO-8601 timestamp of envelope creation.
    pub timestamp: String,
}

/// Peer-to-peer message between agents/workers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerMessage {
    pub message_id: String,
    pub from_task_id: String,
    /// None = broadcast to all workers subscribed to the same topic.
    pub to_task_id: Option<String>,
    /// Topic channel, e.g. "task-123", "objective-456", "formalization-compare".
    pub topic: String,
    pub kind: PeerMessageKind,
    pub payload: serde_json::Value,
    /// ISO 8601 timestamp.
    pub created_at: String,
}

/// Classification of peer message intent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PeerMessageKind {
    /// Request help from another agent.
    RequestHelp,
    /// Share a finding or intermediate result.
    ShareFinding,
    /// Compare results (e.g., dual formalization).
    CompareResult,
    /// Report a blocker that affects other agents.
    ReportBlocker,
    /// Coordinate on shared resource (e.g., file ownership).
    CoordinateResource,
    /// Ask for review/opinion.
    RequestReview,
    /// Provide review/opinion response.
    ReviewResponse,
    /// Signal completion of a dependency.
    DependencyCompleted,
    /// Warn about potential conflict.
    ConflictWarning,
    /// General chat between agents.
    AgentChat,
}

/// Response to acknowledge receipt of a peer message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerMessageAck {
    pub message_id: String,
    pub acknowledged_by: String,
    pub response: Option<serde_json::Value>,
}

/// Subscription to a topic for receiving messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerSubscription {
    pub subscriber_task_id: String,
    pub topic: String,
    pub created_at: String,
}
