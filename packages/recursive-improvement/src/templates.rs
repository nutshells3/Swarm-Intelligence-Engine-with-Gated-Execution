//! REC-006: Self-improvement milestone templates.
//!
//! CSV guardrail: "Implement self-improvement milestone templates
//!   (template library with safety checkpoints and rollback anchors)."
//! proof_or_check_hooks: lifecycle simulation
//! auto_approval_policy: never_silent
//!
//! Acceptance: every self-improvement loop uses a template that
//! prescribes safety checkpoints and rollback anchors.  Templates
//! are a library -- not generated ad hoc.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── REC-006: Template phases and rollback anchors ────────────────────────

/// A phase within a self-improvement template.  Each phase has
/// explicit entry/exit conditions and optional safety checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplatePhase {
    /// Phase name (e.g., "baseline_capture", "proposal_generation",
    /// "integration", "validation").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Conditions that must be met to enter this phase.
    pub entry_conditions: Vec<String>,
    /// Conditions that must be met to exit this phase.
    pub exit_conditions: Vec<String>,
    /// Whether a rollback anchor is created on phase entry.
    pub creates_rollback_anchor: bool,
    /// Whether a safety checkpoint (gate simulation) runs on phase exit.
    pub safety_checkpoint_on_exit: bool,
}

/// A rollback anchor: a snapshot point to which the system can revert
/// if a self-improvement step fails or is rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RollbackAnchor {
    /// Unique anchor identifier.
    pub anchor_id: String,
    /// The template phase that created this anchor.
    pub phase_name: String,
    /// Git commit hash or state snapshot at the anchor point.
    pub snapshot_ref: String,
    /// Whether this anchor is still valid (not superseded).
    pub is_valid: bool,
    /// When the anchor was created.
    pub created_at: DateTime<Utc>,
}

/// REC-006 -- Self-improvement template.
///
/// A pre-defined template prescribing the phases, safety checkpoints,
/// and rollback anchors for a self-improvement loop.  Templates are
/// library items, not ad-hoc constructions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfImprovementTemplate {
    /// Unique template identifier.
    pub template_id: String,
    /// Human-readable template name.
    pub name: String,
    /// Description of what kind of self-improvement this template covers.
    pub description: String,
    /// Ordered list of phases.
    pub phases: Vec<TemplatePhase>,
    /// Rollback anchors created during template execution.
    pub rollback_anchors: Vec<RollbackAnchor>,
    /// Whether lifecycle simulation has been run for this template.
    pub lifecycle_simulated: bool,
    /// The minimum gate level (REC-003) required to use this template.
    pub minimum_gate_level: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
