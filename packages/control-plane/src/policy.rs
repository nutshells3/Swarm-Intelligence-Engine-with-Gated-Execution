//! User policy surface for the control plane (POL-001 through POL-015).
//!
//! CSV guardrail: "Expose execution policy as typed, versioned,
//!   per-cycle-snapshotted records"
//! Caution: Do not let policy live only as env vars.
//!
//! This module extends the user-policy crate with control-plane-specific
//! policy types that are persisted, versioned, and snapshotted per cycle.
//! All policy is typed structs -- never env vars, never raw strings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── POL-001: Policy version tracking ─────────────────────────────────────
//
// Every policy change creates a new version. The control plane never
// silently mutates live policy.

/// POL-001 -- Policy version metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyVersion {
    /// Monotonically increasing version number.
    pub version: u32,
    /// Who or what created this version.
    pub created_by: String,
    /// Why this version was created.
    pub change_reason: String,
    /// Timestamp of version creation.
    pub created_at: DateTime<Utc>,
    /// Hash of the serialized policy for integrity checking.
    pub content_hash: String,
}

// ── POL-002: Per-cycle policy snapshot ────────────────────────────────────
//
// Each execution cycle captures a frozen snapshot of the active policy.
// This ensures reproducibility and audit trail.

/// POL-002 -- Per-cycle policy snapshot reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyCycleSnapshot {
    /// Unique snapshot identifier.
    pub snapshot_id: String,
    /// Cycle this snapshot belongs to.
    pub cycle_id: String,
    /// Policy version that was active at snapshot time.
    pub policy_version: u32,
    /// Serialized policy content (frozen).
    pub policy_content: serde_json::Value,
    /// Timestamp of snapshot creation.
    pub snapshotted_at: DateTime<Utc>,
}

// ── POL-003: Worker dispatch policy ──────────────────────────────────────
//
// Controls how tasks are dispatched to workers, including concurrency
// limits and routing preferences.

/// POL-003 -- Worker dispatch policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerDispatchPolicy {
    /// Maximum concurrent workers across all tasks.
    pub max_concurrent_workers: u32,
    /// Maximum concurrent tasks per worker.
    pub max_tasks_per_worker: u32,
    /// Default task timeout in seconds.
    pub default_timeout_seconds: u32,
    /// Default retry budget per task.
    pub default_retry_budget: u32,
    /// Whether to prefer local workers over API workers.
    pub prefer_local: bool,
    /// Whether to allow session-mode workers.
    pub allow_session_mode: bool,
}

impl Default for WorkerDispatchPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_workers: 4,
            max_tasks_per_worker: 1,
            default_timeout_seconds: 600,
            default_retry_budget: 3,
            prefer_local: false,
            allow_session_mode: true,
        }
    }
}

// ── POL-004: Adapter selection policy ────────────────────────────────────
//
// Controls which adapter is selected for a given task kind and role.

/// POL-004 -- Adapter selection preference.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterPreference {
    /// Use any available adapter.
    Any,
    /// Prefer a specific agent kind.
    PreferKind,
    /// Require a specific agent kind.
    RequireKind,
    /// Use the cheapest available adapter.
    CostOptimized,
    /// Use the fastest available adapter.
    LatencyOptimized,
}

/// POL-004 -- Adapter selection policy per task kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterSelectionPolicy {
    /// Task kind this policy applies to.
    pub task_kind: String,
    /// Selection preference.
    pub preference: AdapterPreference,
    /// Preferred agent kind (when preference is PreferKind or RequireKind).
    pub preferred_agent_kind: Option<String>,
    /// Preferred model name.
    pub preferred_model: Option<String>,
}

// ── POL-005: Token budget policy ─────────────────────────────────────────
//
// Per-role token budgets. The control plane enforces these before
// dispatching context to workers.

/// POL-005 -- Token budget policy per worker role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenBudgetPolicy {
    /// Worker role this budget applies to.
    pub worker_role: String,
    /// Maximum input tokens.
    pub max_input_tokens: u32,
    /// Maximum output tokens.
    pub max_output_tokens: u32,
    /// Maximum total tokens (input + output).
    pub max_total_tokens: u32,
    /// Whether to allow budget overflow with warning.
    pub allow_overflow_with_warning: bool,
}

// ── POL-006: Timeout policy ──────────────────────────────────────────────
//
// Per-task-kind timeout policies with escalation rules.

/// POL-006 -- Timeout policy per task kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeoutPolicy {
    /// Task kind this policy applies to.
    pub task_kind: String,
    /// Timeout in seconds.
    pub timeout_seconds: u32,
    /// Grace period in seconds for graceful shutdown.
    pub grace_period_seconds: u32,
    /// Action to take on timeout.
    pub timeout_action: TimeoutAction,
}

/// Action to take when a task times out.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutAction {
    /// Cancel the task gracefully.
    Cancel,
    /// Kill the task immediately.
    Kill,
    /// Retry the task.
    Retry,
    /// Escalate to human review.
    Escalate,
}

// ── POL-007: Retry policy ────────────────────────────────────────────────
//
// Per-task-kind retry policies.

/// POL-007 -- Retry policy per task kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Task kind this policy applies to.
    pub task_kind: String,
    /// Maximum retry attempts.
    pub max_attempts: u32,
    /// Backoff strategy.
    pub backoff_strategy: RetryBackoff,
    /// Backoff base in milliseconds.
    pub backoff_base_ms: u32,
    /// Whether to retry on empty output.
    pub retry_on_empty_output: bool,
    /// Whether to retry on timeout.
    pub retry_on_timeout: bool,
}

/// Backoff strategy for retries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetryBackoff {
    /// No delay between retries.
    None,
    /// Fixed delay.
    Fixed,
    /// Exponential backoff.
    Exponential,
    /// Linear backoff.
    Linear,
}

// ── POL-008: Concurrency policy ──────────────────────────────────────────
//
// Global and per-role concurrency limits.

/// POL-008 -- Concurrency policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConcurrencyPolicy {
    /// Global maximum concurrent workers.
    pub global_max_workers: u32,
    /// Per-role concurrency limits.
    pub per_role_limits: Vec<RoleConcurrencyLimit>,
}

/// Concurrency limit for a specific worker role.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleConcurrencyLimit {
    pub worker_role: String,
    pub max_concurrent: u32,
}

// ── POL-009: Caution policy ──────────────────────────────────────────────
//
// Per-task-kind cautions that workers must respect.

/// POL-009 -- Caution policy per task kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CautionPolicy {
    /// Task kind this policy applies to.
    pub task_kind: String,
    /// Cautions that workers must acknowledge.
    pub cautions: Vec<CautionEntry>,
}

/// A single caution entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CautionEntry {
    /// Caution identifier.
    pub caution_id: String,
    /// Human-readable description.
    pub description: String,
    /// Whether violation of this caution blocks the task.
    pub blocking: bool,
}

// ── POL-010: Output format policy ────────────────────────────────────────
//
// Controls expected output format per task kind.

/// POL-010 -- Output format policy per task kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputFormatPolicy {
    /// Task kind this policy applies to.
    pub task_kind: String,
    /// Expected output format.
    pub expected_format: OutputFormat,
    /// Whether to allow fuzzy repair of malformed output.
    pub allow_fuzzy_repair: bool,
    /// Maximum output size in characters.
    pub max_output_chars: u32,
}

/// Expected output format classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Structured JSON.
    Json,
    /// Markdown prose.
    Markdown,
    /// Source code.
    Code,
    /// Free-form text.
    FreeText,
    /// Mixed format.
    Mixed,
}

// ── POL-011: Policy override ─────────────────────────────────────────────
//
// Per-task policy overrides that take precedence over the global policy.
// Overrides must have explicit justification and are auditable.

/// POL-011 -- Per-task policy override.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyOverride {
    /// Unique override identifier.
    pub override_id: String,
    /// Task this override applies to.
    pub task_id: String,
    /// Which policy field is being overridden.
    pub field: PolicyField,
    /// The override value (serialized).
    pub override_value: serde_json::Value,
    /// Justification for the override.
    pub justification: String,
    /// Who approved the override.
    pub approved_by: String,
    /// Override creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Whether the override is still active.
    pub active: bool,
}

/// Which policy field is being overridden.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyField {
    Timeout,
    RetryBudget,
    TokenBudget,
    Concurrency,
    AdapterSelection,
    OutputFormat,
}

// ── POL-012: Policy validation ───────────────────────────────────────────
//
// Validates policy consistency and completeness.

/// POL-012 -- Policy validation result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyValidationResult {
    /// Whether the policy is valid.
    pub valid: bool,
    /// Validation errors (empty if valid).
    pub errors: Vec<PolicyValidationError>,
    /// Validation warnings (non-blocking).
    pub warnings: Vec<String>,
}

/// A policy validation error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyValidationError {
    /// Which field or section has the error.
    pub field: String,
    /// Description of the error.
    pub message: String,
}

// ── POL-013: Policy diff ─────────────────────────────────────────────────
//
// Computes the difference between two policy versions.

/// POL-013 -- A single field change between policy versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDiffEntry {
    /// Path to the changed field.
    pub field_path: String,
    /// Previous value.
    pub old_value: serde_json::Value,
    /// New value.
    pub new_value: serde_json::Value,
}

/// POL-013 -- Full diff between two policy versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDiff {
    /// Source version.
    pub from_version: u32,
    /// Target version.
    pub to_version: u32,
    /// Changed fields.
    pub changes: Vec<PolicyDiffEntry>,
}

// ── POL-014: Policy event ────────────────────────────────────────────────
//
// Audit trail for policy changes.

/// POL-014 -- Policy change event kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEventKind {
    /// A new policy version was created.
    VersionCreated,
    /// A per-cycle snapshot was taken.
    SnapshotTaken,
    /// A policy override was applied.
    OverrideApplied,
    /// A policy override was revoked.
    OverrideRevoked,
    /// Policy validation was performed.
    ValidationPerformed,
}

/// POL-014 -- Policy change event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyEvent {
    /// Unique event identifier.
    pub event_id: String,
    /// Kind of policy event.
    pub kind: PolicyEventKind,
    /// Related policy version.
    pub policy_version: Option<u32>,
    /// Related cycle ID (for snapshots).
    pub cycle_id: Option<String>,
    /// Related override ID (for override events).
    pub override_id: Option<String>,
    /// Description of the event.
    pub description: String,
    /// Event timestamp.
    pub created_at: DateTime<Utc>,
}

// ── POL-015: Aggregate execution policy ──────────────────────────────────
//
// Top-level policy snapshot aggregating all POL sub-policies.

/// POL-015 -- Aggregate execution policy for the control plane.
///
/// This is the single source of truth for all execution policy.
/// It is versioned, serializable, and snapshotted per cycle.
/// Policy never lives only as env vars.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPolicy {
    /// Policy version metadata.
    pub version: PolicyVersion,
    /// Worker dispatch policy.
    pub dispatch: WorkerDispatchPolicy,
    /// Adapter selection policies (per task kind).
    pub adapter_selection: Vec<AdapterSelectionPolicy>,
    /// Token budget policies (per worker role).
    pub token_budgets: Vec<TokenBudgetPolicy>,
    /// Timeout policies (per task kind).
    pub timeouts: Vec<TimeoutPolicy>,
    /// Retry policies (per task kind).
    pub retries: Vec<RetryPolicy>,
    /// Concurrency policy.
    pub concurrency: ConcurrencyPolicy,
    /// Caution policies (per task kind).
    pub caution_policies: Vec<CautionPolicy>,
    /// Output format policies (per task kind).
    pub output_formats: Vec<OutputFormatPolicy>,
    /// Active overrides.
    pub active_overrides: Vec<PolicyOverride>,
}
