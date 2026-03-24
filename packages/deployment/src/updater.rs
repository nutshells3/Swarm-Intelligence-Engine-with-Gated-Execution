//! DEP-010: Updater integration hooks.
//!
//! CSV guardrail: "Define updater integration hooks."
//! Acceptance: schema validation.

use serde::{Deserialize, Serialize};

/// The kind of update hook event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UpdateHookKind {
    /// Fired before an update is downloaded.
    PreDownload,
    /// Fired after download but before applying the update.
    PreApply,
    /// Fired after the update has been applied successfully.
    PostApply,
    /// Fired if the update application failed and a rollback occurred.
    PostRollback,
}

/// A single update hook definition. The updater invokes hooks at
/// defined lifecycle points so the control plane can pause cycles,
/// drain workers, or notify operators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateHook {
    /// Unique identifier for this hook.
    pub hook_id: String,
    /// When this hook fires.
    pub kind: UpdateHookKind,
    /// Human-readable description of what this hook does.
    pub description: String,
    /// Whether the hook is blocking (updater waits for completion)
    /// or fire-and-forget.
    pub blocking: bool,
    /// Timeout in milliseconds for blocking hooks.
    pub timeout_ms: Option<u64>,
}

/// Top-level updater integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdaterIntegration {
    /// Whether the updater integration is enabled.
    pub enabled: bool,
    /// Registered hooks.
    pub hooks: Vec<UpdateHook>,
    /// Whether to drain all workers before applying an update.
    pub drain_workers_before_update: bool,
    /// Whether to pause cycle intake before applying an update.
    pub pause_intake_before_update: bool,
}

/// Information about an available update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInfo {
    /// The version string of the available update.
    pub version: String,
    /// Release notes or changelog summary.
    pub release_notes: Option<String>,
    /// Whether this update requires a restart.
    pub requires_restart: bool,
}

/// Result of applying an update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateApplyResult {
    /// Whether the update was applied successfully.
    pub success: bool,
    /// The version that was applied.
    pub applied_version: String,
    /// Error message if the update failed.
    pub error: Option<String>,
    /// Whether a restart is required to complete the update.
    pub restart_required: bool,
}

/// Updater trait. Implementations are responsible for checking for
/// new versions and applying updates. The control plane calls these
/// methods at appropriate lifecycle points (after hooks fire, after
/// workers are drained, etc.).
///
/// No impl is provided yet -- this defines the interface contract
/// that future updater backends (GitHub releases, private registry,
/// etc.) will implement.
#[async_trait::async_trait]
pub trait Updater: Send + Sync {
    /// Check whether a new version is available.
    ///
    /// Returns `Ok(Some(info))` when an update is available,
    /// `Ok(None)` when the system is up-to-date, or `Err` on
    /// transport / registry errors.
    async fn check_for_updates(&self) -> Result<Option<UpdateInfo>, Box<dyn std::error::Error + Send + Sync>>;

    /// Apply the update described by `info`.
    ///
    /// Implementations must be idempotent: re-applying an already-applied
    /// version must succeed without side effects.
    async fn apply_update(&self, info: &UpdateInfo) -> Result<UpdateApplyResult, Box<dyn std::error::Error + Send + Sync>>;
}
