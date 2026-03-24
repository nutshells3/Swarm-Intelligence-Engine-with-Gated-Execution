//! Planning-engine schemas (PLAN-001 through PLAN-009).
//!
//! Every type is machine-readable: statuses and kinds are typed enums (never raw
//! strings), and cross-references use explicit ID fields so the control plane can
//! validate linkage without parsing prose.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── PLAN-001: Objective intake schema ──────────────────────────────────────
//
// Captures the initial objective as received from a user conversation.
// Fields: summary, desired outcome, current stage (typed enum),
// source conversation linkage, and a machine-checkable success metric.

/// Stage of an objective through the planning lifecycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveStage {
    /// Initial capture; not yet validated.
    Draft,
    /// Under active elaboration with the user.
    Elaborating,
    /// All required fields populated and internally consistent.
    Validated,
    /// Plan gate satisfied; implementation in progress.
    Executing,
    /// All milestones complete and accepted.
    Completed,
    /// Abandoned by user or superseded by another objective.
    Abandoned,
}

/// PLAN-001 -- Objective intake record.
///
/// This is the canonical starting point of every planning flow.  The
/// control plane requires `summary`, `desired_outcome`, and
/// `success_metric` to be non-empty before the objective can leave
/// `Draft` stage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveIntake {
    /// Globally unique objective identifier (UUIDv7 recommended).
    pub objective_id: String,
    /// One-sentence summary of what the user wants.
    pub summary: String,
    /// Concrete, falsifiable desired outcome.
    pub desired_outcome: String,
    /// Current lifecycle stage of this objective.
    pub current_stage: ObjectiveStage,
    /// Optional link to the chat session that originated this objective.
    pub source_conversation_id: Option<String>,
    /// Machine-checkable success metric (e.g. "all tests pass", "latency < 200ms").
    pub success_metric: String,
    /// Free-form constraints extracted from the conversation.
    pub constraints: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-002: Architecture draft schema ────────────────────────────────────
//
// Machine-readable representation of an architecture proposal linked to
// an objective.  Components carry typed roles so the decomposer can map
// them to skill packs and worker templates.

/// Role a component plays in the system architecture.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    /// User-facing service (API, CLI, UI).
    Frontend,
    /// Core domain logic.
    Backend,
    /// Persistent storage layer.
    Storage,
    /// Asynchronous message / event bus.
    Messaging,
    /// External system adapter.
    Integration,
    /// Shared library consumed by multiple components.
    SharedLibrary,
    /// Infrastructure or deployment concern.
    Infrastructure,
}

/// A single component inside an architecture draft.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureComponent {
    /// Unique component identifier within the draft.
    pub component_id: String,
    /// Human-readable name.
    pub name: String,
    /// Typed role classification.
    pub role: ComponentRole,
    /// Prose description of responsibility.
    pub responsibility: String,
    /// IDs of other components this one depends on.
    pub depends_on: Vec<String>,
}

/// Review status of an architecture draft.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureDraftStatus {
    /// Initial proposal, not yet reviewed.
    Proposed,
    /// Under active review or iteration.
    UnderReview,
    /// Accepted as the basis for milestone decomposition.
    Accepted,
    /// Rejected; a new draft is required.
    Rejected,
    /// Replaced by a newer revision.
    Superseded,
}

/// PLAN-002 -- Architecture draft.
///
/// Each draft is immutable once created; revisions produce a new draft
/// with an incremented `revision` number and the same `objective_id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureDraft {
    /// Unique draft identifier.
    pub draft_id: String,
    /// The objective this architecture addresses.
    pub objective_id: String,
    /// Prose description of the system boundary.
    pub system_boundary: String,
    /// Typed component list.
    pub components: Vec<ArchitectureComponent>,
    /// Invariant IDs that this architecture must uphold.
    pub invariant_ids: Vec<String>,
    /// Design decisions captured during elaboration.
    pub design_decisions: Vec<String>,
    /// Monotonically increasing revision counter.
    pub revision: i32,
    /// Current review status.
    pub status: ArchitectureDraftStatus,
    pub created_at: DateTime<Utc>,
}

// ─── PLAN-003: Milestone tree schema ────────────────────────────────────────
//
// A tree of milestones decomposing an objective.  Each node carries its
// own acceptance-criteria links and ordering for sequencing.
// Dependencies: PLAN-001 (objective_id), PLAN-002 (draft_id).

/// Completion status of a single milestone.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneStatus {
    /// Not yet started.
    Pending,
    /// Actively being worked on.
    InProgress,
    /// All acceptance criteria met.
    Complete,
    /// Blocked by an unresolved dependency or question.
    Blocked,
    /// Removed from scope.
    Cancelled,
}

/// A single node in the milestone tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneNode {
    /// Unique milestone identifier.
    pub milestone_id: String,
    /// Human-readable title.
    pub title: String,
    /// Optional description of deliverables.
    pub description: String,
    /// Parent milestone ID (`None` for root milestones).
    pub parent_id: Option<String>,
    /// Determines display and execution ordering among siblings.
    pub ordering: i32,
    /// Current status.
    pub status: MilestoneStatus,
    /// IDs of acceptance criteria attached to this milestone.
    pub acceptance_criteria_ids: Vec<String>,
    /// IDs of dependency edges originating from this milestone.
    pub dependency_edge_ids: Vec<String>,
    /// Optional reference to the architecture component this milestone
    /// primarily implements.
    pub component_id: Option<String>,
}

/// PLAN-003 -- Milestone tree.
///
/// The tree is rooted at a single objective and optionally linked to an
/// architecture draft.  The `milestones` vector encodes the full tree
/// via `parent_id` back-references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneTree {
    /// Unique tree identifier.
    pub tree_id: String,
    /// Owning objective.
    pub objective_id: String,
    /// Architecture draft this tree was derived from.
    pub draft_id: Option<String>,
    /// Flat list of milestone nodes (tree structure encoded via `parent_id`).
    pub milestones: Vec<MilestoneNode>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-004: Dependency graph schema ──────────────────────────────────────
//
// Links milestones, roadmap nodes, and other planning elements.
// The graph must be acyclic for `blocks` edges.
// Dependencies: PLAN-003, RMS-001, RMS-002.

/// Classification of a dependency relationship.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    /// Hard blocker: `to_id` cannot start until `from_id` completes.
    Blocks,
    /// Soft ordering preference but not a hard gate.
    ShouldPrecede,
    /// Data or API dependency: `to_id` consumes output of `from_id`.
    DataFlow,
    /// Shared resource: both nodes contend for the same resource.
    SharedResource,
    /// Cross-reference to a roadmap node (RMS linkage).
    RoadmapLink,
}

/// The kind of entity referenced at either end of a dependency edge.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyNodeKind {
    /// A milestone in the milestone tree.
    Milestone,
    /// An execution node (from the node table).
    Node,
    /// A roadmap node (RMS-001/RMS-002 linkage).
    RoadmapNode,
}

/// A single directed edge in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyEdge {
    /// Unique edge identifier.
    pub edge_id: String,
    /// Source entity ID.
    pub from_id: String,
    /// Kind of the source entity.
    pub from_kind: DependencyNodeKind,
    /// Target entity ID.
    pub to_id: String,
    /// Kind of the target entity.
    pub to_kind: DependencyNodeKind,
    /// Relationship classification.
    pub edge_kind: DependencyKind,
    /// Optional rationale for why this dependency exists.
    pub rationale: Option<String>,
}

/// PLAN-004 -- Dependency graph.
///
/// The control plane must verify:
/// 1. Acyclicity for `Blocks` edges.
/// 2. All referenced IDs resolve to existing milestones, nodes, or
///    roadmap nodes (graph legality).
/// 3. Every `RoadmapLink` edge targets a valid roadmap node ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyGraph {
    /// Unique graph identifier.
    pub graph_id: String,
    /// Owning objective.
    pub objective_id: String,
    /// All edges in the graph.
    pub edges: Vec<DependencyEdge>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-005: Acceptance criteria schema ───────────────────────────────────
//
// Each criterion is attached to a milestone (or plan/node) and carries a
// typed verification method so the control plane knows how to evaluate it.

/// How an acceptance criterion is verified.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    /// Automated test or CI check.
    Automated,
    /// Manual human review required.
    ManualReview,
    /// Formal proof or model-checked property.
    FormalVerification,
    /// Observable metric crosses a threshold.
    MetricThreshold,
}

/// Current evaluation state of an acceptance criterion.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CriterionStatus {
    /// Not yet evaluated.
    Pending,
    /// Evaluation in progress.
    Evaluating,
    /// Criterion met.
    Satisfied,
    /// Criterion evaluated but not met.
    Failed,
    /// Criterion no longer applicable.
    Waived,
}

/// The kind of planning entity an acceptance criterion is attached to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CriterionOwnerKind {
    /// Attached to a plan.
    Plan,
    /// Attached to a milestone node.
    Milestone,
    /// Attached to an execution node.
    Node,
}

/// PLAN-005 -- Acceptance criterion.
///
/// Connected to a plan, milestone, or node via `owner_id` + `owner_kind`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceCriterion {
    /// Unique criterion identifier.
    pub criterion_id: String,
    /// ID of the owning entity (plan, milestone, or node).
    pub owner_id: String,
    /// Kind of the owning entity.
    pub owner_kind: CriterionOwnerKind,
    /// Human-readable description of the criterion.
    pub description: String,
    /// How this criterion is verified.
    pub verification_method: VerificationMethod,
    /// Machine-checkable predicate expression (optional).
    /// When present, the control plane can evaluate this automatically.
    pub predicate_expression: Option<String>,
    /// Current evaluation status.
    pub status: CriterionStatus,
    /// Ordering among criteria attached to the same owner.
    pub ordering: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-006: Unresolved question schema ───────────────────────────────────
//
// Tracks open questions with resolution status and blocking effects.
// The plan gate (PLAN-009) uses the count of unresolved questions against
// a budget to decide whether implementation may proceed.

/// Severity of an unresolved question's blocking effect.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionSeverity {
    /// Blocks all downstream work until resolved.
    Blocking,
    /// Does not block but may cause rework if resolved differently.
    Important,
    /// Nice to know; no downstream impact expected.
    Informational,
}

/// Resolution status of a question.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionResolutionStatus {
    /// Question is open and unanswered.
    Open,
    /// A tentative answer exists but is not yet confirmed.
    Tentative,
    /// Question has been definitively answered.
    Resolved,
    /// Question is no longer relevant.
    Dismissed,
}

/// PLAN-006 -- Unresolved question.
///
/// The `blocking_ids` field lists milestone or node IDs that cannot
/// proceed until this question reaches `Resolved` or `Dismissed`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedQuestion {
    /// Unique question identifier.
    pub question_id: String,
    /// Owning objective.
    pub objective_id: String,
    /// The question text.
    pub question: String,
    /// Why this question matters.
    pub context: String,
    /// Severity classification.
    pub severity: QuestionSeverity,
    /// Current resolution status.
    pub resolution_status: QuestionResolutionStatus,
    /// Answer text (populated when `Tentative` or `Resolved`).
    pub resolution_answer: Option<String>,
    /// IDs of milestones or nodes blocked by this question.
    pub blocking_ids: Vec<String>,
    /// Source conversation or review that raised this question.
    pub source_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-007: Risk register schema ────────────────────────────────────────
//
// Tracks identified risks with severity, likelihood, and mitigation plans.

/// Qualitative severity of a risk's impact if realized.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Qualitative likelihood of a risk occurring.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLikelihood {
    Unlikely,
    Possible,
    Likely,
    AlmostCertain,
}

/// Current disposition of a risk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    /// Identified but no mitigation plan yet.
    Identified,
    /// Mitigation plan defined and being executed.
    Mitigating,
    /// Risk has materialized; incident response in progress.
    Realized,
    /// Risk is no longer relevant.
    Closed,
    /// Risk accepted without mitigation.
    Accepted,
}

/// PLAN-007 -- Risk register entry.
///
/// Each entry captures a single risk, its qualitative severity/likelihood,
/// the affected milestones, and a concrete mitigation plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskRegisterEntry {
    /// Unique risk identifier.
    pub risk_id: String,
    /// Owning objective.
    pub objective_id: String,
    /// Short title for the risk.
    pub title: String,
    /// Detailed description of what could go wrong.
    pub description: String,
    /// Impact severity if the risk materializes.
    pub severity: RiskSeverity,
    /// Likelihood of occurrence.
    pub likelihood: RiskLikelihood,
    /// Current disposition.
    pub status: RiskStatus,
    /// Concrete mitigation plan (actions, owners, deadlines).
    pub mitigation_plan: String,
    /// IDs of milestones affected by this risk.
    pub affected_milestone_ids: Vec<String>,
    /// Optional trigger conditions that indicate the risk is materializing.
    pub trigger_conditions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-008: Invariant schema ────────────────────────────────────────────
//
// Machine-checkable invariants that must hold throughout the planning and
// execution lifecycle.

/// Scope at which an invariant applies.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvariantScope {
    /// Applies to the entire objective/plan.
    Global,
    /// Applies to a specific architecture component.
    Component,
    /// Applies to a specific milestone.
    Milestone,
    /// Applies to runtime/deployment.
    Runtime,
}

/// How an invariant is enforced.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvariantEnforcement {
    /// Checked at plan validation time.
    PlanValidation,
    /// Checked at each cycle gate transition.
    CycleGate,
    /// Checked continuously during execution.
    Continuous,
    /// Checked at integration/merge time.
    Integration,
}

/// Current evaluation state of an invariant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvariantStatus {
    /// Not yet checked.
    Unchecked,
    /// Invariant holds.
    Holding,
    /// Invariant violated.
    Violated,
    /// Invariant temporarily suspended (requires justification).
    Suspended,
}

/// PLAN-008 -- Plan invariant.
///
/// Invariants are machine-checkable properties. The `predicate` field
/// contains a structured expression the control plane evaluates.
/// When `enforcement` is `Continuous`, the orchestrator re-checks the
/// invariant on every state transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanInvariant {
    /// Unique invariant identifier.
    pub invariant_id: String,
    /// Owning objective.
    pub objective_id: String,
    /// Human-readable description of what must hold.
    pub description: String,
    /// Machine-readable predicate expression.
    pub predicate: String,
    /// Scope of applicability.
    pub scope: InvariantScope,
    /// When/how the invariant is enforced.
    pub enforcement: InvariantEnforcement,
    /// Current evaluation status.
    pub status: InvariantStatus,
    /// ID of the entity this invariant applies to (interpretation depends
    /// on `scope`: component_id for `Component`, milestone_id for
    /// `Milestone`, etc.).  `None` when scope is `Global`.
    pub target_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── PLAN-009: Plan gate schema ────────────────────────────────────────────
//
// The gate controls when implementation is unlocked.  It enumerates every
// condition that must be met and tracks the current evaluation of each.
// This is the most critical schema: if the gate is not explicit and
// machine-readable, premature implementation can begin.

/// Individual conditions that a plan gate evaluates.
///
/// Each variant maps to a concrete, verifiable property of the plan.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateCondition {
    /// Objective has a non-empty summary, desired outcome, and success metric.
    ObjectiveSummarized,
    /// At least one architecture draft exists with status `Accepted`.
    ArchitectureDrafted,
    /// A milestone tree exists with at least one milestone node.
    MilestoneTreeCreated,
    /// Every milestone has at least one acceptance criterion.
    AcceptanceCriteriaDefined,
    /// Dependency graph passes acyclicity check for `Blocks` edges.
    DependenciesAcyclic,
    /// All `Blocks` dependency edges resolve to existing entities.
    DependenciesResolved,
    /// At least one invariant is defined.
    InvariantsExtracted,
    /// All invariants with `PlanValidation` enforcement are `Holding`.
    InvariantsHolding,
    /// At least one risk has been identified and assessed.
    RisksIdentified,
    /// Count of unresolved (Open/Tentative) blocking questions is within budget.
    UnresolvedQuestionsBelowBudget,
}

/// Evaluation result for a single gate condition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConditionEval {
    /// Condition has not been evaluated yet.
    NotEvaluated,
    /// Condition is satisfied.
    Pass,
    /// Condition is not satisfied.
    Fail,
}

/// Snapshot of one gate condition's evaluation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateConditionEntry {
    /// Which condition this entry evaluates.
    pub condition: GateCondition,
    /// Current evaluation result.
    pub eval: ConditionEval,
}

/// Overall gate status (derived from individual condition evaluations).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// Not all conditions are met; implementation is locked.
    Open,
    /// All conditions are met; implementation is unlocked.
    Satisfied,
    /// Manually overridden by a user policy decision.
    /// The `override_reason` field on `PlanGateDefinition` must be set.
    Overridden,
}

/// PLAN-009 -- Plan gate definition.
///
/// The gate is the single point that controls whether implementation may
/// begin.  The control plane evaluates `condition_entries` and derives
/// `current_status`:
///
/// - `Open`      -- at least one condition has `eval == Fail`.
/// - `Satisfied` -- every condition has `eval == Pass`.
/// - `Overridden`-- a user explicitly overrode the gate (must set `override_reason`).
///
/// The `unresolved_question_budget` / `unresolved_question_count` pair
/// implements PLAN-006's "budget" mechanic: blocking questions exceeding
/// the budget keep the gate open.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanGateDefinition {
    /// Unique gate identifier.
    pub gate_id: String,
    /// The plan this gate controls.
    pub plan_id: String,
    /// Per-condition evaluation snapshots.
    pub condition_entries: Vec<GateConditionEntry>,
    /// Derived overall status.
    pub current_status: GateStatus,
    /// Maximum number of unresolved blocking questions allowed for the
    /// `UnresolvedQuestionsBelowBudget` condition to pass.
    pub unresolved_question_budget: i32,
    /// Current count of unresolved blocking questions.
    pub unresolved_question_count: i32,
    /// Required when `current_status` is `Overridden`.
    pub override_reason: Option<String>,
    pub evaluated_at: DateTime<Utc>,
}

// ─── Validation helpers ─────────────────────────────────────────────────────

/// Errors detected during dependency graph validation (PLAN-004 checks).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyGraphError {
    /// A cycle was detected among `Blocks` edges.  Contains the IDs
    /// forming the cycle.
    CycleDetected { cycle_ids: Vec<String> },
    /// An edge references an entity ID that does not exist.
    DanglingReference { edge_id: String, missing_id: String },
    /// A `RoadmapLink` edge targets an ID that is not a roadmap node.
    InvalidRoadmapLink { edge_id: String, target_id: String },
}

/// Result of evaluating a plan gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateEvaluationResult {
    /// Updated gate definition with fresh `condition_entries` and `current_status`.
    pub gate: PlanGateDefinition,
    /// Human-readable messages for any failing conditions.
    pub failure_reasons: Vec<String>,
}
