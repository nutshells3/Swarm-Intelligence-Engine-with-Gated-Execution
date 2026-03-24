//! REC-003: Self-improvement safety gates.
//!
//! CSV guardrail: "Define self-improvement safety gates (gate lattice:
//!   blocked, experimental, guarded, review-required, promotion-forbidden)."
//! proof_or_check_hooks: gate simulation
//! auto_approval_policy: never_silent
//!
//! Acceptance: gate levels form a lattice with explicit allowed actions
//! at each level.  No self-improvement action can proceed without
//! passing through the gate lattice.

use serde::{Deserialize, Serialize};

// ── REC-003: Gate level lattice ──────────────────────────────────────────

/// Gate levels for self-improvement, ordered from most restrictive to
/// least restrictive.  Forms a lattice: a task at a lower level cannot
/// escalate to a higher level without explicit approval.
///
/// CSV lattice: blocked > experimental > guarded > review-required >
///              promotion-forbidden
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GateLevel {
    /// Completely blocked -- no self-improvement actions permitted.
    Blocked = 0,
    /// Experimental -- changes may be attempted in isolation but cannot
    /// be integrated without review.
    Experimental = 1,
    /// Guarded -- changes proceed under heightened monitoring with
    /// automatic rollback on anomaly.
    Guarded = 2,
    /// Review required -- changes are permitted but must pass human
    /// or governance review before integration.
    ReviewRequired = 3,
    /// Promotion forbidden -- changes may integrate locally but must
    /// never self-promote to broader adoption (REC-008).
    PromotionForbidden = 4,
}

/// Condition that must be satisfied before a gate level is granted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecursiveGateCondition {
    /// Human-readable description of the condition.
    pub description: String,
    /// Whether this condition has been satisfied.
    pub satisfied: bool,
    /// Evidence reference (review_id, drift_check_id, etc.).
    pub evidence_ref: Option<String>,
}

/// Actions allowed at a given gate level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllowedActions {
    /// Whether the task may write to the repo target.
    pub may_write: bool,
    /// Whether the task may integrate changes into the main branch.
    pub may_integrate: bool,
    /// Whether the task may trigger downstream tasks.
    pub may_trigger_downstream: bool,
    /// Whether the task may self-promote its changes (always false
    /// for PromotionForbidden; enforced by REC-008).
    pub may_self_promote: bool,
    /// Whether rollback anchors are mandatory (REC-006).
    pub rollback_required: bool,
}

/// REC-003 -- Safety gate lattice configuration.
///
/// Defines the gate level, conditions, and allowed actions for a
/// self-improvement objective.  The lattice is evaluated before every
/// recursive action (gate simulation check hook).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyGateLattice {
    /// The self-improvement objective this gate governs.
    pub objective_id: String,
    /// Current gate level.
    pub current_level: GateLevel,
    /// Conditions that must be met to remain at or advance from
    /// this gate level.
    pub conditions: Vec<RecursiveGateCondition>,
    /// Actions allowed at the current gate level.
    pub allowed_actions: AllowedActions,
    /// Whether gate simulation has been run (check hook).
    pub simulation_verified: bool,
}

impl GateLevel {
    /// Returns the default allowed actions for this gate level.
    pub fn default_actions(self) -> AllowedActions {
        match self {
            GateLevel::Blocked => AllowedActions {
                may_write: false,
                may_integrate: false,
                may_trigger_downstream: false,
                may_self_promote: false,
                rollback_required: false,
            },
            GateLevel::Experimental => AllowedActions {
                may_write: true,
                may_integrate: false,
                may_trigger_downstream: false,
                may_self_promote: false,
                rollback_required: true,
            },
            GateLevel::Guarded => AllowedActions {
                may_write: true,
                may_integrate: false,
                may_trigger_downstream: false,
                may_self_promote: false,
                rollback_required: true,
            },
            GateLevel::ReviewRequired => AllowedActions {
                may_write: true,
                may_integrate: true,
                may_trigger_downstream: true,
                may_self_promote: false,
                rollback_required: true,
            },
            GateLevel::PromotionForbidden => AllowedActions {
                may_write: true,
                may_integrate: true,
                may_trigger_downstream: true,
                may_self_promote: false, // always false -- REC-008
                rollback_required: false,
            },
        }
    }
}
