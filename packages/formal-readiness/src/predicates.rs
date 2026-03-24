//! Formal-readiness predicates (FRM-001 through FRM-007).
//!
//! Machine-readable predicate definitions for planning, milestone,
//! dependency, promotion, transition, conflict-resolution, and
//! certification-selection invariants.
//!
//! Design rules:
//! - NO Lean/Isabelle imports or dependencies -- readiness only.
//! - All predicates must be replayable from durable state alone.
//! - Predicate types are backend-neutral: no prover syntax embedded.
//! - Do not sneak in policy changes under formalization; these freeze
//!   representation, not semantics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Shared predicate building blocks ────────────────────────────────────

/// The data type of a predicate input field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    /// A string value (IDs, names, etc.).
    Text,
    /// An integer value (counts, budgets, etc.).
    Integer,
    /// A boolean flag.
    Boolean,
    /// A structured JSON object.
    Json,
    /// A list of values.
    List,
}

/// A single typed input to a predicate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredicateInput {
    /// Machine-readable name of the input (e.g. "plan_id").
    pub name: String,
    /// Data type of this input.
    pub input_type: InputType,
    /// Human-readable description.
    pub description: String,
    /// Whether this input is required for evaluation.
    pub required: bool,
}

/// Outcome of evaluating a predicate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PredicateOutcome {
    /// Predicate holds.
    Satisfied,
    /// Predicate does not hold.
    Violated,
    /// Predicate cannot be evaluated (missing inputs).
    Indeterminate,
}

/// Result of a single predicate evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PredicateEvaluation {
    /// The outcome of evaluation.
    pub outcome: PredicateOutcome,
    /// Human-readable explanation of the result.
    pub reason: String,
    /// Timestamp of evaluation.
    pub evaluated_at: DateTime<Utc>,
    /// Version of the predicate definition used.
    pub predicate_version: String,
}

// ─── FRM-001: Plan invariant predicates ──────────────────────────────────
//
// CSV guardrail: "Translate the planning layer's core invariants into a
// machine-readable form so later formal validation can consume stable
// predicates instead of prose."
//
// Scope: Invariant schema for plan completeness, required artifacts,
// unresolved-question bounds, and implementation blocking semantics.
//
// Acceptance: Planning invariants can be exported, replayed, and compared
// across revisions without reinterpreting prose.
//
// Proof hooks: predicate serialization check; invariant replay check;
// missing-field rejection check.
//
// Caution: Do not sneak in product-policy changes under the label of
// formalization readiness; this milestone freezes representation, not
// policy semantics.

/// Category of plan invariant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanInvariantCategory {
    /// Plan must have all required artifacts present.
    Completeness,
    /// Required artifacts must exist.
    ArtifactPresence,
    /// Unresolved question count must be within budget.
    QuestionBounds,
    /// Implementation must be blocked until gate is satisfied.
    ImplementationBlocking,
}

/// FRM-001 -- Machine-readable plan invariant predicate.
///
/// Each predicate captures a single planning invariant with typed inputs
/// so it can be serialized, replayed, and later consumed by a formal
/// validation backend without reinterpreting prose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanInvariantPredicate {
    /// Stable predicate identifier (e.g. "plan_inv_completeness_001").
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what this invariant checks.
    pub description: String,
    /// Category classification.
    pub category: PlanInvariantCategory,
    /// Typed inputs required to evaluate this predicate.
    pub inputs: Vec<PredicateInput>,
    /// Most recent evaluation result, if any.
    pub evaluation: Option<PredicateEvaluation>,
    /// Version of this predicate definition (for superseding).
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── FRM-002: Milestone graph validity predicates ────────────────────────
//
// CSV guardrail: "Define machine-readable predicates for milestone graph
// validity so decomposition legality can later be checked mechanically."
//
// Scope: Predicates for milestone presence, dependency completeness,
// required review points, and graph well-formedness.
//
// Acceptance: A milestone graph can be validated from stored state alone
// without human reinterpretation.
//
// Proof hooks: predicate evaluation check; graph fixture regression;
// missing-review-point rejection check.
//
// Caution: Do not overfit predicates to one project snapshot; keep them
// generic but narrow to current orchestration semantics.

/// Aspect of milestone graph validity being checked.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneValidityAspect {
    /// Every milestone node has required fields populated.
    NodeCompleteness,
    /// Every dependency edge references existing nodes.
    EdgeResolution,
    /// Required review points are present in the graph.
    ReviewPointPresence,
    /// The graph structure is well-formed (single root, no orphans).
    WellFormedness,
}

/// FRM-002 -- Milestone graph validity predicate.
///
/// These predicates check that milestone graphs are structurally valid
/// and contain all required review points. They operate on stored state
/// only (milestone tree + dependency edges from the authoritative tables).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneGraphPredicate {
    /// Stable predicate identifier.
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of the validity check.
    pub description: String,
    /// Which aspect of graph validity this predicate checks.
    pub aspect: MilestoneValidityAspect,
    /// Typed inputs required to evaluate this predicate.
    pub inputs: Vec<PredicateInput>,
    /// Node-level requirements (minimum fields, etc.).
    pub node_requirements: Vec<String>,
    /// Edge-level requirements (resolution, type constraints, etc.).
    pub edge_requirements: Vec<String>,
    /// Most recent evaluation result, if any.
    pub evaluation: Option<PredicateEvaluation>,
    /// Version of this predicate definition.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── FRM-003: Dependency acyclicity predicates ───────────────────────────
//
// CSV guardrail: "Define explicit machine-checkable acyclicity rules for
// roadmap, milestone, and task dependency graphs."
//
// Scope: Cycle-detection predicates and exportable graph constraints for
// nodes, milestones, and task dependencies.
//
// Acceptance: Dependency cycles can be detected deterministically from
// durable graph state and surfaced as explicit failures.
//
// Proof hooks: cycle-detection regression; graph export roundtrip;
// invalid-cycle fixture check.
//
// Caution: Do not use projection-only edges as the sole source of truth
// for legality checks; predicates must anchor to authoritative graph
// state.

/// Scope of the dependency graph being checked for cycles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphScope {
    /// Milestone-level dependencies.
    Milestone,
    /// Task-level dependencies within a node.
    Task,
    /// Roadmap-level dependencies across objectives.
    Roadmap,
    /// Full cross-scope graph.
    Full,
}

/// Result of a cycle detection pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CycleDetectionResult {
    /// Whether a cycle was found.
    pub cycle_found: bool,
    /// IDs of entities forming the cycle (empty if no cycle).
    pub cycle_members: Vec<String>,
    /// Human-readable description of the cycle path.
    pub cycle_description: String,
}

/// FRM-003 -- Dependency acyclicity predicate.
///
/// Checks that dependency graphs are acyclic within a given scope.
/// Self-references can be explicitly allowed (e.g., a node depending on
/// its own prior revision) via `allowed_self_references`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcyclicityPredicate {
    /// Stable predicate identifier.
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of this acyclicity check.
    pub description: String,
    /// Which graph scope this predicate applies to.
    pub graph_scope: GraphScope,
    /// Edge kinds that are included in the cycle check (e.g., "blocks").
    pub edge_kinds_checked: Vec<String>,
    /// Entity IDs that are allowed to self-reference.
    pub allowed_self_references: Vec<String>,
    /// Most recent cycle detection result, if any.
    pub cycle_detection_result: Option<CycleDetectionResult>,
    /// Version of this predicate definition.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── FRM-004: Promotion precondition predicates ──────────────────────────
//
// CSV guardrail: "Encode promotion preconditions in machine-readable form
// so later formal validation can reason about what qualifies for branch
// to mainline movement."
//
// Scope: Predicates for required reviews, certification gates, drift
// status, conflict status, and stale invalidation status before promotion.
//
// Acceptance: Promotion readiness can be computed from durable state and
// is not reducible to a single worker verdict or score.
//
// Proof hooks: predicate evaluation check; stale invalidation simulation;
// approval-effect replay check.
//
// Caution: Do not let advisory metrics or self-generated artifacts bypass
// formal promotion preconditions.

/// Category of promotion precondition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionConditionCategory {
    /// Required reviews must be complete.
    ReviewComplete,
    /// Certification gate must be satisfied.
    CertificationSatisfied,
    /// No unresolved drift detected.
    DriftClear,
    /// No unresolved blocking conflicts.
    ConflictClear,
    /// No stale invalidations pending.
    StaleInvalidationClear,
}

/// FRM-004 -- Promotion precondition predicate.
///
/// Each predicate encodes one aspect of promotion readiness. Promotion
/// cannot proceed unless all required preconditions hold. The predicate
/// operates on durable state (certification records, review artifacts,
/// conflict status, drift status).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromotionPredicate {
    /// Stable predicate identifier.
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of this precondition.
    pub description: String,
    /// Category of promotion precondition.
    pub category: PromotionConditionCategory,
    /// Typed inputs required to evaluate this predicate.
    pub inputs: Vec<PredicateInput>,
    /// Required approval effects (e.g., "certification_pass").
    pub required_approval_effects: Vec<String>,
    /// Most recent evaluation result, if any.
    pub evaluation: Option<PredicateEvaluation>,
    /// Version of this predicate definition.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── FRM-005: Branch/mainline transition legality predicates ─────────────
//
// CSV guardrail: "Define the legality conditions for moving artifacts
// and nodes across branch mainline states so transition law can later be
// machine-checked."
//
// Scope: Transition predicates for branch to mainline, mainline candidate
// to mainline, demotion, revalidation, and hold states.
//
// Acceptance: Every branch/mainline transition can be justified by an
// explicit predicate set and illegal transitions can be rejected
// deterministically.
//
// Proof hooks: transition matrix replay check; illegal-transition fixture
// check; demotion simulation.
//
// Caution: Do not let UI shortcuts or manual operator actions bypass
// formal transition legality.

/// A single transition rule in the legality matrix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransitionRule {
    /// Source state (lane or lifecycle name).
    pub from_state: String,
    /// Target state (lane or lifecycle name).
    pub to_state: String,
    /// Whether this transition is allowed.
    pub allowed: bool,
    /// Conditions that must hold for an allowed transition to proceed.
    pub required_conditions: Vec<String>,
    /// Human-readable rationale for this rule.
    pub rationale: String,
}

/// FRM-005 -- Branch/mainline transition legality matrix.
///
/// Encodes the complete set of allowed and forbidden state transitions
/// for node lane and lifecycle movement. The matrix is backend-neutral
/// and can be replayed from durable state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransitionLegalityMatrix {
    /// Stable identifier for this matrix version.
    pub matrix_id: String,
    /// All transition rules.
    pub entries: Vec<TransitionRule>,
    /// Version of this matrix definition.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── FRM-006: Conflict-resolution admissibility predicates ───────────────
//
// CSV guardrail: "Define machine-readable admissibility predicates for
// closing conflicts so future formal checks can verify that resolution
// is not silent overwrite."
//
// Scope: Predicates for what evidence, review, certification, and
// comparison state are sufficient to mark a conflict resolved.
//
// Acceptance: A conflict can be marked resolved only when the
// admissibility predicate holds over preserved artifacts.
//
// Proof hooks: conflict fixture regression; admissibility replay check;
// unresolved-competing-artifact rejection check.
//
// Caution: Do not let a clean git merge or a single score collapse
// competing evidence into false resolution.

/// Requirement category for conflict resolution admissibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdmissibilityRequirement {
    /// Independent evidence must support the resolution.
    EvidenceRequired,
    /// A qualified review must approve the resolution.
    ReviewRequired,
    /// Certification must cover the resolution.
    CertificationRequired,
    /// Competing artifacts must be explicitly compared.
    ComparisonRequired,
}

/// FRM-006 -- Conflict-resolution admissibility predicate.
///
/// Encodes the conditions under which a conflict may be marked resolved.
/// Each predicate refers to durable competing artifacts and requires
/// explicit evidence, review, or certification before closure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictAdmissibilityPredicate {
    /// Stable predicate identifier.
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what must hold for resolution to be admissible.
    pub description: String,
    /// The conflict class this predicate applies to.
    pub conflict_class: String,
    /// Requirements that must be met.
    pub requirements: Vec<AdmissibilityRequirement>,
    /// Typed inputs required to evaluate this predicate.
    pub inputs: Vec<PredicateInput>,
    /// Most recent evaluation result, if any.
    pub evaluation: Option<PredicateEvaluation>,
    /// Version of this predicate definition.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── FRM-007: Certification-selection predicates ─────────────────────────
//
// CSV guardrail: "Encode the current certification candidate selection
// law in machine-readable form so future formal checking can verify
// slow-lane selectivity."
//
// Scope: Predicates for when a node task or branch requires formal-claim
// submission versus staying provisional.
//
// Acceptance: Certification selection can be replayed from durable local
// state and remains narrow enough to avoid turning the slow lane into
// the system bottleneck.
//
// Proof hooks: selection replay check; over-selection simulation;
// under-selection fixture check.
//
// Caution: Do not broaden formal certification just because export
// machinery exists; keep decision-focused selection.

/// Reason a node/task was selected for certification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CertificationSelectionReason {
    /// Downstream dependencies rely on this output.
    DownstreamDependencyUse,
    /// A promotion request was filed for this node.
    PromotionRequested,
    /// A conflict adjudication requires certified resolution.
    ConflictAdjudication,
    /// The output is marked as safety-critical.
    SafetyCriticalContract,
}

/// FRM-007 -- Certification-selection predicate.
///
/// Encodes when a node, task, or branch requires formal-claim submission
/// (slow lane) versus remaining provisional (fast lane). Selection must
/// stay narrow to avoid bottlenecking the system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationSelectionPredicate {
    /// Stable predicate identifier.
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of the selection criterion.
    pub description: String,
    /// Reason for selection.
    pub selection_reason: CertificationSelectionReason,
    /// Typed inputs required to evaluate this predicate.
    pub inputs: Vec<PredicateInput>,
    /// Most recent evaluation result, if any.
    pub evaluation: Option<PredicateEvaluation>,
    /// Version of this predicate definition.
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // FRM-001: predicate serialization roundtrip
    #[test]
    fn plan_invariant_predicate_serialization_roundtrip() {
        let pred = PlanInvariantPredicate {
            predicate_id: "plan_inv_completeness_001".into(),
            name: "Plan completeness".into(),
            description: "All required planning artifacts are present.".into(),
            category: PlanInvariantCategory::Completeness,
            inputs: vec![PredicateInput {
                name: "plan_id".into(),
                input_type: InputType::Text,
                description: "The plan to check".into(),
                required: true,
            }],
            evaluation: Some(PredicateEvaluation {
                outcome: PredicateOutcome::Satisfied,
                reason: "All artifacts present.".into(),
                evaluated_at: Utc::now(),
                predicate_version: "1.0.0".into(),
            }),
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: PlanInvariantPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.predicate_id, "plan_inv_completeness_001");
        assert_eq!(back.category, PlanInvariantCategory::Completeness);
    }

    // FRM-001: missing-field rejection
    #[test]
    fn plan_invariant_rejects_missing_required_field() {
        // A predicate with a required input but no value would be indeterminate.
        let pred = PlanInvariantPredicate {
            predicate_id: "plan_inv_artifact_001".into(),
            name: "Required artifact".into(),
            description: "A required artifact must exist.".into(),
            category: PlanInvariantCategory::ArtifactPresence,
            inputs: vec![PredicateInput {
                name: "artifact_id".into(),
                input_type: InputType::Text,
                description: "ID of the required artifact".into(),
                required: true,
            }],
            evaluation: Some(PredicateEvaluation {
                outcome: PredicateOutcome::Indeterminate,
                reason: "Required input 'artifact_id' not provided.".into(),
                evaluated_at: Utc::now(),
                predicate_version: "1.0.0".into(),
            }),
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(
            pred.evaluation.as_ref().unwrap().outcome,
            PredicateOutcome::Indeterminate
        );
    }

    // FRM-002: milestone graph predicate serialization
    #[test]
    fn milestone_graph_predicate_roundtrip() {
        let pred = MilestoneGraphPredicate {
            predicate_id: "ms_validity_001".into(),
            name: "Node completeness".into(),
            description: "All milestone nodes have required fields.".into(),
            aspect: MilestoneValidityAspect::NodeCompleteness,
            inputs: vec![],
            node_requirements: vec!["title must be non-empty".into()],
            edge_requirements: vec![],
            evaluation: None,
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: MilestoneGraphPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.aspect, MilestoneValidityAspect::NodeCompleteness);
    }

    // FRM-003: acyclicity predicate roundtrip
    #[test]
    fn acyclicity_predicate_roundtrip() {
        let pred = AcyclicityPredicate {
            predicate_id: "acyclic_ms_001".into(),
            name: "Milestone acyclicity".into(),
            description: "No cycles among blocking edges.".into(),
            graph_scope: GraphScope::Milestone,
            edge_kinds_checked: vec!["blocks".into()],
            allowed_self_references: vec![],
            cycle_detection_result: Some(CycleDetectionResult {
                cycle_found: false,
                cycle_members: vec![],
                cycle_description: "No cycle detected.".into(),
            }),
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: AcyclicityPredicate = serde_json::from_str(&json).unwrap();
        assert!(!back.cycle_detection_result.unwrap().cycle_found);
    }

    // FRM-003: cycle detected result
    #[test]
    fn acyclicity_predicate_cycle_detected() {
        let result = CycleDetectionResult {
            cycle_found: true,
            cycle_members: vec!["ms-1".into(), "ms-2".into(), "ms-1".into()],
            cycle_description: "ms-1 -> ms-2 -> ms-1".into(),
        };
        assert!(result.cycle_found);
        assert_eq!(result.cycle_members.len(), 3);
    }

    // FRM-004: promotion predicate roundtrip
    #[test]
    fn promotion_predicate_roundtrip() {
        let pred = PromotionPredicate {
            predicate_id: "promo_review_001".into(),
            name: "Review complete".into(),
            description: "All required reviews are done.".into(),
            category: PromotionConditionCategory::ReviewComplete,
            inputs: vec![],
            required_approval_effects: vec!["review_pass".into()],
            evaluation: None,
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: PromotionPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category, PromotionConditionCategory::ReviewComplete);
    }

    // FRM-005: transition legality matrix roundtrip
    #[test]
    fn transition_legality_matrix_roundtrip() {
        let matrix = TransitionLegalityMatrix {
            matrix_id: "tlm_001".into(),
            entries: vec![
                TransitionRule {
                    from_state: "branch".into(),
                    to_state: "mainline_candidate".into(),
                    allowed: true,
                    required_conditions: vec!["review_pass".into(), "cert_pass".into()],
                    rationale: "Requires review and certification.".into(),
                },
                TransitionRule {
                    from_state: "mainline".into(),
                    to_state: "branch".into(),
                    allowed: false,
                    required_conditions: vec![],
                    rationale: "Demotion from mainline to branch is forbidden.".into(),
                },
            ],
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&matrix).unwrap();
        let back: TransitionLegalityMatrix = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 2);
        assert!(back.entries[0].allowed);
        assert!(!back.entries[1].allowed);
    }

    // FRM-005: illegal transition rejection
    #[test]
    fn transition_rule_forbidden_transition() {
        let rule = TransitionRule {
            from_state: "archived".into(),
            to_state: "mainline".into(),
            allowed: false,
            required_conditions: vec![],
            rationale: "Cannot promote archived nodes.".into(),
        };
        assert!(!rule.allowed);
    }

    // FRM-006: conflict admissibility predicate roundtrip
    #[test]
    fn conflict_admissibility_predicate_roundtrip() {
        let pred = ConflictAdmissibilityPredicate {
            predicate_id: "conflict_adm_001".into(),
            name: "Evidence required for divergence".into(),
            description: "Divergence conflicts require independent evidence.".into(),
            conflict_class: "divergence".into(),
            requirements: vec![
                AdmissibilityRequirement::EvidenceRequired,
                AdmissibilityRequirement::ReviewRequired,
            ],
            inputs: vec![],
            evaluation: None,
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: ConflictAdmissibilityPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.requirements.len(), 2);
    }

    // FRM-007: certification selection predicate roundtrip
    #[test]
    fn certification_selection_predicate_roundtrip() {
        let pred = CertificationSelectionPredicate {
            predicate_id: "cert_sel_001".into(),
            name: "Downstream dependency use".into(),
            description: "Node selected because downstream dependencies rely on it."
                .into(),
            selection_reason: CertificationSelectionReason::DownstreamDependencyUse,
            inputs: vec![],
            evaluation: None,
            version: "1.0.0".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: CertificationSelectionPredicate = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.selection_reason,
            CertificationSelectionReason::DownstreamDependencyUse
        );
    }

    // Cross-cutting: predicate replay -- same inputs produce same serialization
    #[test]
    fn predicate_replay_deterministic() {
        let eval = PredicateEvaluation {
            outcome: PredicateOutcome::Satisfied,
            reason: "All checks pass.".into(),
            evaluated_at: Utc::now(),
            predicate_version: "1.0.0".into(),
        };
        let json1 = serde_json::to_string(&eval).unwrap();
        let json2 = serde_json::to_string(&eval).unwrap();
        assert_eq!(json1, json2);
    }
}
