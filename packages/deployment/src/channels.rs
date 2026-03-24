//! DEP-004: Update channel schema.
//!
//! CSV guardrail: "Define update channel schema."
//! Acceptance: schema validation.

use serde::{Deserialize, Serialize};

/// The update channel controlling how the system receives version
/// updates. Explicit enum -- never inferred from env vars.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    /// Receive only stable, fully-tested releases.
    Stable,
    /// Receive beta releases for early testing.
    Beta,
    /// Pinned to a specific version; no automatic updates.
    Pinned,
    /// All automatic updates are disabled.
    Disabled,
}

/// Policy governing which update channel is active and any constraints
/// on automatic application of updates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateChannelPolicy {
    /// The active update channel.
    pub channel: UpdateChannel,
    /// When channel is Pinned, the exact version string to pin to.
    pub pinned_version: Option<String>,
    /// Whether the system may automatically apply updates without
    /// operator confirmation.
    pub auto_apply: bool,
    /// Whether to notify the operator when an update is available.
    pub notify_on_available: bool,
}
