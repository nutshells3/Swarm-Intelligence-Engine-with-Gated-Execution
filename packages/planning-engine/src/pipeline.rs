//! Planning pipeline traits (PLAN-010 through PLAN-017).
//!
//! Each trait defines a single pipeline step that transforms planning state.
//! Concrete implementations will be provided by AI workers or deterministic
//! processors.  Every trait method:
//!
//! - Uses schema types from [`crate::schemas`] as input/output.
//! - Returns `Result<T, PlanningError>` so callers get structured failures.
//! - Is documented with the CSV acceptance criterion:
//!   "The planning rule or schema is explicit, machine-readable, and
//!    sufficient for later control-plane execution."
//!
//! CSV caution reminder: Do not let planning prose substitute for executable
//! gate logic; do not unlock implementation from weak plans.

use crate::schemas::{
    AcceptanceCriterion, ArchitectureDraft, DependencyGraph, MilestoneTree,
    ObjectiveIntake, PlanInvariant, RiskRegisterEntry, UnresolvedQuestion,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Errors that any planning pipeline step can produce.
///
/// This is the unified error type across PLAN-010 through PLAN-017.
/// Callers can inspect the variant to decide whether to retry, escalate,
/// or abort.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanningError {
    /// The input was syntactically or semantically invalid.
    InvalidInput {
        field: String,
        reason: String,
    },
    /// A required upstream artifact is missing (e.g. no ObjectiveIntake
    /// exists when trying to generate an ArchitectureDraft).
    MissingDependency {
        dependency: String,
        reason: String,
    },
    /// The pipeline step exceeded its iteration budget
    /// (CSV: "max 3 planning refinements before escalation").
    IterationBudgetExceeded {
        step: String,
        attempts: u32,
    },
    /// The AI worker produced malformed output that could not be repaired.
    /// CSV parse_recovery_policy: "strict structured output preferred;
    /// one fuzzy repair pass allowed; escalate after repeated malformed
    /// planning output."
    MalformedOutput {
        step: String,
        raw_output: String,
        reason: String,
    },
    /// An internal error that does not fit the above categories.
    Internal {
        reason: String,
    },
}

impl fmt::Display for PlanningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid input on field `{field}`: {reason}")
            }
            Self::MissingDependency { dependency, reason } => {
                write!(f, "missing dependency `{dependency}`: {reason}")
            }
            Self::IterationBudgetExceeded { step, attempts } => {
                write!(
                    f,
                    "iteration budget exceeded for step `{step}` after {attempts} attempts"
                )
            }
            Self::MalformedOutput {
                step,
                raw_output: _,
                reason,
            } => {
                write!(f, "malformed output from step `{step}`: {reason}")
            }
            Self::Internal { reason } => write!(f, "internal error: {reason}"),
        }
    }
}

impl std::error::Error for PlanningError {}

/// Raw objective text as received from a user conversation before
/// structured expansion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawObjective {
    /// Free-form description of what the user wants.
    pub description: String,
    /// Optional conversation ID that originated this objective.
    pub source_conversation_id: Option<String>,
}

/// Objective expansion pipeline.
///
/// Expands a vague, free-form objective into a structured
/// [`ObjectiveIntake`] record.  The implementation must populate at least
/// `summary`, `desired_outcome`, and `success_metric` to satisfy the
/// schema validation.
///
/// CSV expected output: "Objective expansion pipeline that turns vague
/// goals into architecture and milestone drafts."
///
/// CSV proof hooks: "objective-to-plan integration smoke;
/// malformed-output recovery check; roadmap absorption compatibility
/// check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait ObjectiveExpander {
    /// Expand a raw objective into a structured intake record.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::InvalidInput`] if the description is empty.
    /// - [`PlanningError::MalformedOutput`] if the AI worker produces
    ///   output that cannot be parsed into [`ObjectiveIntake`].
    /// - [`PlanningError::IterationBudgetExceeded`] if more than 3
    ///   refinement attempts fail.
    fn expand_objective(
        &self,
        raw: &RawObjective,
    ) -> Result<ObjectiveIntake, PlanningError>;
}

/// Architecture draft generation.
///
/// Generates an [`ArchitectureDraft`] from a validated
/// [`ObjectiveIntake`].  The draft decomposes the objective into typed
/// components with roles and inter-component dependencies.
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding revision (incremented
/// `revision` field on the draft).
pub trait ArchitectureDrafter {
    /// Generate an architecture draft from a validated objective.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if the objective is still
    ///   in `Draft` stage.
    /// - [`PlanningError::MalformedOutput`] if the AI worker output
    ///   cannot be parsed.
    fn draft_architecture(
        &self,
        objective: &ObjectiveIntake,
    ) -> Result<ArchitectureDraft, PlanningError>;
}

/// Milestone explosion.
///
/// Creates a [`MilestoneTree`] from an [`ArchitectureDraft`].  Each
/// architecture component should map to one or more milestone nodes,
/// producing many small executable items rather than a vague plan blob.
///
/// CSV caution: "Do not produce giant unstructured milestone lists that
/// still require human decomposition."
///
/// CSV dependencies: PLAN-003, PLAN-010.
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait MilestoneExploder {
    /// Explode an architecture draft into a milestone tree.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if the draft status is not
    ///   `Accepted`.
    /// - [`PlanningError::InvalidInput`] if the draft has zero
    ///   components.
    fn explode_milestones(
        &self,
        draft: &ArchitectureDraft,
    ) -> Result<MilestoneTree, PlanningError>;
}

/// Optional roadmap context supplied to the dependency extractor so it
/// can create cross-reference edges (RoadmapLink).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoadmapContext {
    /// Known roadmap node IDs available for linking.
    pub roadmap_node_ids: Vec<String>,
}

/// Dependency extraction.
///
/// Builds a [`DependencyGraph`] from a [`MilestoneTree`] and optional
/// [`RoadmapContext`].  The resulting graph must be acyclic for `Blocks`
/// edges (PLAN-004 schema invariant).
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait DependencyExtractor {
    /// Extract dependencies from a milestone tree.
    ///
    /// # Arguments
    ///
    /// * `tree` -- The milestone tree to analyze.
    /// * `roadmap` -- Optional roadmap context for cross-reference edges.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if the tree has no
    ///   milestones.
    /// - [`PlanningError::InvalidInput`] if the extractor detects a
    ///   cycle among `Blocks` edges.
    fn extract_dependencies(
        &self,
        tree: &MilestoneTree,
        roadmap: Option<&RoadmapContext>,
    ) -> Result<DependencyGraph, PlanningError>;
}

/// Acceptance criteria generation.
///
/// Generates [`AcceptanceCriterion`] entries for each milestone in a
/// [`MilestoneTree`].  Every milestone must have at least one criterion
/// for the plan gate (`AcceptanceCriteriaDefined`) to pass.
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait AcceptanceCriteriaGenerator {
    /// Generate acceptance criteria for all milestones in the tree.
    ///
    /// Returns one or more criteria per milestone.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if the tree is empty.
    /// - [`PlanningError::MalformedOutput`] if the AI worker output
    ///   cannot be parsed into valid criteria.
    fn generate_acceptance_criteria(
        &self,
        tree: &MilestoneTree,
    ) -> Result<Vec<AcceptanceCriterion>, PlanningError>;
}

/// Aggregated plan state passed to the question extractor so it can
/// identify gaps and open questions across all planning artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanState {
    /// The objective that anchors this plan.
    pub objective: ObjectiveIntake,
    /// The accepted architecture draft (if any).
    pub architecture: Option<ArchitectureDraft>,
    /// The milestone tree (if any).
    pub milestones: Option<MilestoneTree>,
    /// The dependency graph (if any).
    pub dependencies: Option<DependencyGraph>,
    /// Existing acceptance criteria.
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    /// Existing risks.
    pub risks: Vec<RiskRegisterEntry>,
    /// Existing invariants.
    pub invariants: Vec<PlanInvariant>,
}

/// Unresolved question extraction.
///
/// Identifies [`UnresolvedQuestion`] entries from the aggregated plan
/// state.  The count of blocking questions feeds into the plan gate's
/// `UnresolvedQuestionsBelowBudget` condition (PLAN-009).
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait QuestionExtractor {
    /// Identify unresolved questions from the current plan state.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if no objective exists.
    /// - [`PlanningError::MalformedOutput`] if the AI worker output
    ///   cannot be parsed.
    fn extract_questions(
        &self,
        state: &PlanState,
    ) -> Result<Vec<UnresolvedQuestion>, PlanningError>;
}

/// Risk register generation.
///
/// Creates [`RiskRegisterEntry`] entries from the aggregated plan state.
/// At least one risk must be identified for the plan gate's
/// `RisksIdentified` condition to pass.
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait RiskGenerator {
    /// Generate risk register entries from the current plan state.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if no objective exists.
    /// - [`PlanningError::MalformedOutput`] if the AI worker output
    ///   cannot be parsed.
    fn generate_risks(
        &self,
        state: &PlanState,
    ) -> Result<Vec<RiskRegisterEntry>, PlanningError>;
}

/// Invariant extraction.
///
/// Identifies [`PlanInvariant`] entries from the architecture draft and
/// aggregated plan state.  At least one invariant must exist for the
/// plan gate's `InvariantsExtracted` condition to pass, and all
/// `PlanValidation`-scoped invariants must be `Holding` for
/// `InvariantsHolding` to pass.
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
///
/// Idempotency: rerun creates a superseding planning revision.
pub trait InvariantExtractor {
    /// Extract invariants from the architecture and plan state.
    ///
    /// # Errors
    ///
    /// - [`PlanningError::MissingDependency`] if no architecture draft
    ///   exists.
    /// - [`PlanningError::MalformedOutput`] if the AI worker output
    ///   cannot be parsed.
    fn extract_invariants(
        &self,
        state: &PlanState,
    ) -> Result<Vec<PlanInvariant>, PlanningError>;
}
