//! REC-007: Drift checks for self-improvement loops.
//!
//! CSV guardrail: "Implement drift checks for self-improvement loops
//!   (anti-drift across policy, schema, skill resolution, approval law)."
//! proof_or_check_hooks: drift check
//! auto_approval_policy: never_silent
//!
//! Acceptance: drift checks detect semantic drift (not just file diffs)
//! across policy definitions, schema versions, skill resolution rules,
//! and approval law.  Every drift check produces a durable artifact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The kind of drift detected.  CSV requires detection across four
/// domains: policy, schema, skill resolution, and approval law.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    /// Policy definition has drifted from its baseline.
    Policy,
    /// Schema (database, API, event) has drifted.
    Schema,
    /// Skill resolution rules have drifted.
    SkillResolution,
    /// Approval law / governance rules have drifted.
    ApprovalLaw,
}

/// Detailed policy drift description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyDrift {
    /// Which policy field drifted.
    pub policy_field: String,
    /// Baseline value (semantic summary, not raw bytes).
    pub baseline_semantic: String,
    /// Current value after self-improvement.
    pub current_semantic: String,
    /// Whether this drift is intentional (declared in the objective).
    pub intentional: bool,
}

/// Detailed schema drift description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaDrift {
    /// Which schema entity drifted.
    pub entity: String,
    /// Baseline version or hash.
    pub baseline_version: String,
    /// Current version or hash.
    pub current_version: String,
    /// Summary of structural changes.
    pub structural_changes: String,
}

/// Detailed skill resolution drift description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDrift {
    /// Which skill pack or resolution rule drifted.
    pub skill_ref: String,
    /// How the resolution changed.
    pub resolution_change: String,
    /// Whether the drift affects task dispatch.
    pub affects_dispatch: bool,
}

/// Detailed approval law drift description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDrift {
    /// Which approval rule drifted.
    pub rule_ref: String,
    /// How the approval semantics changed.
    pub semantic_change: String,
    /// Whether the drift weakens governance.
    pub weakens_governance: bool,
}

/// Drift check artifact.
///
/// A durable record of a drift check run across all four domains.
/// Detects semantic drift, not just file-level diffs (CSV constraint).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftCheckArtifact {
    /// Unique drift check identifier.
    pub drift_check_id: String,
    /// The self-improvement objective this check covers.
    pub objective_id: String,
    /// Loop iteration index.
    pub iteration_index: i32,
    /// Policy drifts detected.
    pub policy_drifts: Vec<PolicyDrift>,
    /// Schema drifts detected.
    pub schema_drifts: Vec<SchemaDrift>,
    /// Skill resolution drifts detected.
    pub skill_drifts: Vec<SkillDrift>,
    /// Approval law drifts detected.
    pub approval_drifts: Vec<ApprovalDrift>,
    /// Overall drift severity: "none", "low", "medium", "high", "critical".
    pub overall_severity: String,
    /// Whether any unintentional drift was found.
    pub has_unintentional_drift: bool,
    /// Whether drift blocks further self-improvement.
    pub blocks_continuation: bool,
    pub checked_at: DateTime<Utc>,
}
