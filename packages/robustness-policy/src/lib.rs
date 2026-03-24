//! Robustness policy schema definitions.
//!
//! This crate encodes robustness laws as typed, machine-readable schema rather
//! than implicit retry loops or ad-hoc operator heuristics. Every robustness
//! failure mode covered here is bounded by explicit policy so the control plane
//! can enforce it uniformly.
//!
//! Items: ROB-001 through ROB-020.

use serde::{Deserialize, Serialize};

// ── ROB-001: Malformed-output taxonomy ──────────────────────────────────
//
// CSV guardrail: "Malformed-output taxonomy covering empty output,
//   almost-JSON, mixed prose+JSON, and structurally invalid payloads."
// Acceptance: taxonomy is explicit, machine-readable, and enforced by
//   the control plane.

/// Classifies the kind of malformed output received from an AI worker.
///
/// Each variant maps to a distinct failure mode so the control plane can
/// route to the correct recovery policy (ROB-003..ROB-005) without
/// guessing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MalformedOutputKind {
    /// Output is not valid JSON at all (e.g. raw prose, binary garbage).
    InvalidJson,
    /// Parseable JSON but one or more required fields are absent.
    MissingRequiredFields,
    /// A field is present but carries the wrong JSON type.
    WrongFieldType,
    /// Output references identifiers (task IDs, file paths, symbols) that
    /// do not exist in the current state.
    HallucinatedReferences,
    /// Output was cut off mid-stream (e.g. token limit hit).
    TruncatedOutput,
    /// Worker returned an empty body or a null payload.
    EmptyOutput,
    /// Output exceeds the declared length budget for this task class.
    ExceededLengthBudget,
    /// Output is valid JSON but does not match the expected schema
    /// envelope (e.g. array where object was expected).
    UnexpectedFormat,
    /// Output mixes prose and JSON in a single payload (the "almost-JSON"
    /// case from the CSV).
    MixedProseAndJson,
}

// ── ROB-002: Structured-output preference policy ────────────────────────
//
// CSV guardrail: "strict structured output preferred; one fuzzy repair
//   pass allowed; escalate after repeated malformed planning output."
// Acceptance: policy is explicit enum, machine-readable.

/// Determines how strictly the control plane enforces structured output
/// conformance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OutputPreference {
    /// Reject all non-conforming output immediately.
    Strict,
    /// Attempt one bounded repair pass (ROB-003), then reject on failure.
    Fuzzy,
    /// Best-effort extraction; log the deviation but do not block.
    Lenient,
}

// ── ROB-003: Fuzzy JSON repair rules ────────────────────────────────────
//
// CSV guardrail: "Fuzzy JSON repair rules bounded enough not to invent
//   meaning."  "one bounded repair pass then fail or escalate."
// Acceptance: repair operations are enumerated; attempt count is bounded.

/// An individual repair operation the fuzzy-repair pass is allowed to
/// perform. Each operation is syntactic; none may invent or infer
/// semantic content.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RepairOperation {
    /// Strip leading/trailing prose around a JSON block.
    StripSurroundingProse,
    /// Close unclosed braces/brackets at the end of truncated output.
    CloseUnclosedDelimiters,
    /// Fix trailing commas before closing delimiters.
    RemoveTrailingCommas,
    /// Normalise single-quoted strings to double-quoted.
    FixQuoteStyle,
    /// Attempt to extract the first JSON object from a mixed payload.
    ExtractFirstJsonObject,
    /// Replace known control characters that break JSON parsing.
    SanitizeControlCharacters,
}

/// Policy governing how many repair attempts are allowed and which
/// operations may be applied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FuzzyRepairPolicy {
    /// Maximum number of sequential repair attempts before giving up.
    /// The CSV mandates "one bounded repair pass", so the typical value
    /// is 1.
    pub max_repair_attempts: u32,
    /// The set of syntactic operations permitted during repair. Operations
    /// not listed here are forbidden even if they would succeed.
    pub allowed_operations: Vec<RepairOperation>,
}

// ── ROB-004: Bounded parse retry rules ──────────────────────────────────
//
// CSV guardrail: "define explicit recovery rather than relying on blind
//   retry."
// Acceptance: retry count, backoff, and escalation are all explicit.

/// Strategy for delaying between retry attempts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// No delay between retries.
    None,
    /// Fixed delay (in milliseconds) between each retry.
    FixedMs,
    /// Exponential backoff: base_ms * 2^attempt.
    Exponential,
}

/// Rules bounding how many times the system may re-request output from a
/// worker after a parse failure, and how it escalates when retries are
/// exhausted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParseRetryPolicy {
    /// Hard ceiling on the number of retry attempts. Zero means no
    /// retries; fail immediately.
    pub max_attempts: u32,
    /// How to space out retries.
    pub backoff_strategy: BackoffStrategy,
    /// Base delay in milliseconds (used by FixedMs and Exponential).
    pub backoff_base_ms: u32,
    /// What to do when all retries are exhausted.
    pub escalation_action: EscalationAction,
}

// ── ROB-005: Parse-failure fallback policy ──────────────────────────────
//
// CSV guardrail: "define explicit recovery rather than relying on blind
//   retry."
// Acceptance: fallback action is a typed enum, not a string.

/// The action the control plane takes when a parse failure persists after
/// all retries (ROB-004) are exhausted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ParseFailureFallback {
    /// Re-submit with a simplified prompt (one more chance).
    RetryWithSimplifiedPrompt,
    /// Route the failure to a supervisor agent for resolution.
    Escalate,
    /// Flag the output for human review; block the task until reviewed.
    HumanReview,
    /// Skip the task and mark it as failed (non-critical tasks only).
    Skip,
    /// Quarantine the output and continue with a fallback value.
    QuarantineAndFallback,
}

/// Full parse-failure fallback policy, combining the fallback action with
/// task-criticality context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParseFailureFallbackPolicy {
    /// Fallback for tasks classified as critical.
    pub critical_task_fallback: ParseFailureFallback,
    /// Fallback for tasks classified as non-critical.
    pub non_critical_task_fallback: ParseFailureFallback,
}

// ── ROB-006: Context budget policy by task class ────────────────────────
//
// CSV guardrail: "Context budget policy by task class so larger projects
//   do not collapse from context overload."
//   "task-class specific budget with top-k retrieval and summary
//   preference."
// Acceptance: budgets are per-task-class, machine-readable.

/// The class of task, used to key context budget limits.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TaskClass {
    /// Planning and elaboration tasks.
    Planning,
    /// Code implementation tasks.
    Implementation,
    /// Code review and quality checks.
    Review,
    /// Certification and formal-claim tasks.
    Certification,
    /// Debugging and diagnostic tasks.
    Debugging,
    /// Research and analysis tasks.
    Research,
}

/// Token budget for a single task class. The control plane enforces
/// these ceilings before dispatching context to a worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBudgetPolicy {
    /// Which task class this budget applies to.
    pub task_class: TaskClass,
    /// Maximum input tokens allowed in the worker prompt.
    pub max_input_tokens: u32,
    /// Maximum output tokens the worker is allowed to produce.
    pub max_output_tokens: u32,
    /// Whether the system may substitute a summary artifact when the raw
    /// artifact exceeds the budget (see ROB-008).
    pub allow_summary_fallback: bool,
    /// Maximum number of context items retrieved (top-k).
    pub max_context_items: u32,
}

// ── ROB-007: Retrieval ranking policy for context selection ─────────────
//
// CSV guardrail: "must always prefer bounded retrieval over full-history
//   expansion."
// Acceptance: ranking dimensions are typed enum, weights are explicit.

/// A dimension used to rank retrieved context items.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RankingDimension {
    /// More recently created/modified items rank higher.
    Recency,
    /// Items semantically closer to the query rank higher.
    Relevance,
    /// Items that are direct dependencies of the target rank higher.
    Dependency,
    /// Items from the same task/node lineage rank higher.
    Lineage,
}

/// A single ranking rule: a dimension and its relative weight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RankingRule {
    pub dimension: RankingDimension,
    /// Relative weight (higher = more important). Integer to keep Eq.
    pub weight: u32,
}

/// Full retrieval ranking policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetrievalRankingPolicy {
    /// Ordered list of ranking rules. The control plane applies them in
    /// priority order, breaking ties with subsequent rules.
    pub ranking_rules: Vec<RankingRule>,
    /// Hard ceiling on retrieved items regardless of ranking.
    pub max_retrieved_items: u32,
}

// ── ROB-008: Summary-artifact preference rules ──────────────────────────
//
// CSV guardrail: "must always prefer bounded retrieval over full-history
//   expansion."
// Acceptance: preference is a typed enum; threshold is explicit.

/// When to prefer a summary artifact over the raw source artifact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SummaryPreference {
    /// Always use the summary if one exists.
    AlwaysPreferSummary,
    /// Use the summary only when the raw artifact exceeds the token
    /// budget.
    PreferSummaryWhenOverBudget,
    /// Never use summaries; always include raw artifacts (use with
    /// caution -- may blow context budgets).
    NeverUseSummary,
}

/// Policy for summary-artifact substitution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummaryArtifactPolicy {
    /// Default preference across all task classes.
    pub default_preference: SummaryPreference,
    /// Token threshold above which a summary is preferred, regardless of
    /// the default preference setting.
    pub summary_threshold_tokens: u32,
}

// ── ROB-009: Direct-dependency context expansion rules ──────────────────
//
// CSV guardrail: "must always prefer bounded retrieval over full-history
//   expansion."  "use affected files, changed symbols, and direct
//   dependents only."
// Acceptance: expansion depth is capped; types of expansion are typed.

/// How far context expansion may follow dependency edges.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionScope {
    /// Only the artifact itself, no expansion.
    SelfOnly,
    /// The artifact plus its immediate (depth-1) dependents.
    DirectDependentsOnly,
    /// The artifact plus dependents up to a configured depth.
    BoundedTransitive,
}

/// Rules governing how far the context assembler may expand from a
/// target artifact into its dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependencyExpansionPolicy {
    /// The furthest the expansion may go.
    pub scope: ExpansionScope,
    /// When scope is BoundedTransitive, this is the maximum graph depth.
    /// Ignored for other scopes.
    pub max_expansion_depth: u32,
    /// Maximum number of expanded artifacts added to context.
    pub max_expanded_items: u32,
}

// ── ROB-010: Full-history denial rules ──────────────────────────────────
//
// CSV guardrail: "must always prefer bounded retrieval over full-history
//   expansion."  "forbid full-history expansion."
// Acceptance: denial is unconditional and machine-readable.

/// Modes of history access. The control plane uses this to enforce that
/// full-history dumps are never assembled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAccessMode {
    /// Only the most recent N items (bounded window).
    RecentWindow,
    /// Only items matching the retrieval ranking policy (ROB-007).
    RankedSubset,
    /// Full history -- this mode is always denied by the policy.
    FullHistory,
}

/// The full-history denial policy. Any request for `FullHistory` access
/// must be rejected unconditionally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FullHistoryDenialPolicy {
    /// The set of access modes that are permitted. `FullHistory` must
    /// never appear here.
    pub permitted_modes: Vec<HistoryAccessMode>,
    /// Maximum window size when using `RecentWindow`.
    pub max_recent_window_items: u32,
    /// Whether to log a security event when full-history access is
    /// attempted.
    pub log_denied_attempts: bool,
}

// ── ROB-011: Planning iteration cap schema ──────────────────────────────
//
// CSV guardrail: "Planning iteration cap schema that prevents infinite
//   refinement loops."  "defines the planning cap itself."
// Acceptance: caps are numeric, escalation is typed.

/// What happens when a planning or question budget is exhausted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EscalationAction {
    /// Freeze the current plan and escalate to a supervisor agent.
    EscalateToSupervisor,
    /// Freeze the current plan and flag for human review.
    EscalateToHuman,
    /// Accept the current plan state as-is (lossy but unblocking).
    AcceptCurrentState,
    /// Abort the planning phase entirely and mark the objective as
    /// blocked.
    AbortAndBlock,
}

/// Caps on planning iterations. The control plane must enforce these
/// before allowing another elaboration or clarification round.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanningIterationCap {
    /// Maximum elaboration passes (spec refinement rounds).
    pub max_elaboration_passes: u32,
    /// Maximum clarification rounds (question-answer exchanges with the
    /// user or a supervisor).
    pub max_clarification_rounds: u32,
    /// What to do when either cap is hit.
    pub escalation_action: EscalationAction,
}

// ── ROB-012: Unresolved-question budget schema ──────────────────────────
//
// CSV guardrail: "Unresolved-question budget schema used to trigger
//   escalation before endless planning."
//   "defines unresolved-question exhaustion threshold."
// Acceptance: budget is numeric, escalation is typed.

/// Budget for unresolved questions during a planning phase. When the
/// count exceeds the budget, the control plane triggers escalation
/// rather than allowing the planning loop to continue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedQuestionBudget {
    /// Maximum number of open (unresolved) questions before escalation.
    pub max_open_questions: u32,
    /// What to do when the budget is exceeded.
    pub escalation_action: EscalationAction,
}

// ── ROB-013: Planning bailout escalation rules ──────────────────────────
//
// CSV guardrail: "Bailout escalation rules that route planning deadlocks
//   into review or human escalation."
//   auto_approval_policy: "never" -- human review is mandatory once
//   bailout threshold is hit.
// Acceptance: trigger conditions and escalation targets are typed.

/// A condition that can trigger a planning bailout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BailoutTrigger {
    /// The planning iteration cap (ROB-011) was hit.
    IterationCapExhausted,
    /// The unresolved-question budget (ROB-012) was exceeded.
    QuestionBudgetExhausted,
    /// No progress was made between two consecutive planning rounds.
    NoProgressDetected,
    /// The context budget (ROB-006) was exceeded during planning.
    ContextBudgetExhausted,
    /// Wall-clock time for the planning phase exceeded the limit.
    TimeoutExceeded,
}

/// What to do when a bailout is triggered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BailoutAction {
    /// Freeze the plan and send to human review (mandatory per CSV).
    FreezeAndEscalateToHuman,
    /// Freeze the plan and route to a supervisor agent for triage.
    FreezeAndEscalateToSupervisor,
    /// Abort the objective and mark it as blocked.
    AbortObjective,
}

/// A single bailout rule: when a trigger fires, take the given action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BailoutRule {
    pub trigger: BailoutTrigger,
    pub action: BailoutAction,
}

/// Full planning bailout escalation policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanningBailoutPolicy {
    /// Ordered list of bailout rules. The first matching rule fires.
    pub rules: Vec<BailoutRule>,
    /// Whether to automatically snapshot the plan state before bailout.
    pub snapshot_before_bailout: bool,
}

// ── ROB-014: Graded certification gate lattice ──────────────────────────
//
// CSV guardrail (M3/M5): "Graded certification gate lattice so useful
//   outcomes do not collapse into one blocked bucket."
//   "gate-lattice simulation; downstream admissibility matrix check."
// Acceptance: gate grades are typed enum; lattice ordering is explicit.

/// A certification grade returned by the formal-claim gateway or a
/// local reviewer. Grades form a lattice from strongest (Proven) to
/// weakest (Rejected).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CertificationGrade {
    /// Formally verified by the proof assistant.
    Proven,
    /// Passed all automated checks but not formally proven.
    CheckedPass,
    /// Reviewed and approved by a qualified agent or human.
    ReviewedApproved,
    /// Partially checked; some claims remain unverified.
    PartiallyChecked,
    /// Not yet evaluated.
    Unevaluated,
    /// Evaluation attempted but inconclusive.
    Inconclusive,
    /// Failed one or more checks.
    Failed,
    /// Explicitly rejected.
    Rejected,
}

/// A single entry in the gate lattice defining the ordering between
/// two grades.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateLatticeEntry {
    /// The grade that is strictly stronger.
    pub stronger: CertificationGrade,
    /// The grade that is strictly weaker.
    pub weaker: CertificationGrade,
}

/// The full gate lattice. The control plane uses this to decide whether
/// a given certification result is sufficient for a given downstream
/// action (ROB-015).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateLattice {
    /// The set of ordering relationships between grades.
    pub ordering: Vec<GateLatticeEntry>,
}

// ── ROB-015: Graded downstream admissibility rules ──────────────────────
//
// CSV guardrail (M3/M5): "Downstream admissibility rules tied to the
//   graded gate lattice."
// Acceptance: admissibility is per-action, keyed by grade.

/// An action that depends on certification grade to proceed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DownstreamAction {
    /// Promote a branch artifact to mainline candidate.
    PromoteToMainlineCandidate,
    /// Allow a task result to be used as input to another task.
    UseAsTaskInput,
    /// Allow the artifact to be included in a release.
    IncludeInRelease,
    /// Allow the artifact to serve as a dependency for planning.
    UseForPlanning,
    /// Allow the artifact to be presented to a human reviewer.
    PresentForHumanReview,
}

/// A single admissibility rule: a downstream action and the minimum
/// certification grade required to permit it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdmissibilityRule {
    pub action: DownstreamAction,
    /// The minimum grade (per the lattice in ROB-014) that permits this
    /// action.
    pub minimum_grade: CertificationGrade,
}

/// Full downstream admissibility policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownstreamAdmissibilityPolicy {
    /// List of admissibility rules. Each downstream action must appear
    /// at most once.
    pub rules: Vec<AdmissibilityRule>,
}

// ── ROB-016: Symbol-level overlap detection rules ───────────────────────
//
// CSV guardrail (M3): "Symbol-level overlap detection rules for semantic
//   conflict pre-screening."
//   "symbol overlap simulation; changed-symbol impact check."
//   context_selection: "use affected files, changed symbols, and direct
//   dependents only."
// Acceptance: overlap categories and thresholds are typed.

/// The kind of symbol overlap detected between two concurrent changes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OverlapKind {
    /// Both changes modify the same function/method.
    SameFunction,
    /// Both changes modify the same type definition (struct/enum/trait).
    SameTypeDefinition,
    /// Both changes modify the same module-level constant or static.
    SameConstant,
    /// Both changes affect the same file but different symbols.
    SameFileDifferentSymbols,
    /// One change modifies a symbol that the other change's code calls.
    CallerCalleeOverlap,
    /// Both changes modify the same trait implementation.
    SameTraitImpl,
}

/// Severity assigned to a detected symbol overlap.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OverlapSeverity {
    /// Informational only; no action required.
    Info,
    /// Warrants review but does not block.
    Warning,
    /// Blocks promotion until resolved.
    Blocking,
}

/// A rule mapping an overlap kind to a severity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlapDetectionRule {
    pub overlap_kind: OverlapKind,
    pub severity: OverlapSeverity,
}

/// Full symbol-level overlap detection policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolOverlapPolicy {
    /// Rules evaluated in order; every matching rule fires.
    pub rules: Vec<OverlapDetectionRule>,
    /// Whether to auto-escalate when any blocking overlap is detected.
    pub escalate_on_blocking: bool,
}

// ── ROB-017: Semantic conflict trigger rules ────────────────────────────
//
// CSV guardrail (M3/M5): "Semantic conflict trigger rules for cases
//   where merge is syntactically clean but semantically unsafe."
//   "post-merge semantic validation simulation; claim-impact conflict
//   simulation."
// Acceptance: trigger conditions are typed; conflict is a first-class
//   object.

/// A condition that fires a semantic conflict even though the syntactic
/// merge succeeded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SemanticConflictTrigger {
    /// A formal claim's assumption was invalidated by a concurrent
    /// change.
    ClaimAssumptionInvalidated,
    /// A type's invariant was weakened or strengthened by a concurrent
    /// change.
    InvariantDrift,
    /// A function's observable behaviour changed in a way that conflicts
    /// with another branch's expectations.
    BehaviourDivergence,
    /// Two branches introduced semantically incompatible API extensions.
    ApiIncompatibility,
    /// A dependency version was bumped in conflicting directions.
    DependencyVersionConflict,
    /// A symbol-level overlap (ROB-016) of blocking severity was found.
    BlockingSymbolOverlap,
}

/// A rule that maps a trigger condition to the conflict severity and
/// required resolution path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticConflictRule {
    pub trigger: SemanticConflictTrigger,
    /// Whether this trigger blocks promotion unconditionally.
    pub blocks_promotion: bool,
    /// Whether human review is required to resolve this trigger.
    pub requires_human_review: bool,
}

/// Full semantic conflict trigger policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticConflictTriggerPolicy {
    pub rules: Vec<SemanticConflictRule>,
}

// ── ROB-018: Semantic conflict artifact schema ──────────────────────────
//
// CSV guardrail (M5): "Semantic conflict artifact schema that survives
//   beyond plain merge warnings."
//   "same semantic conflict trigger on same artifact set should not
//   create duplicates."
//   "Keep semantic conflict history even after resolution for future
//   drift analysis."
// Acceptance: conflict is a first-class, deduplicated, persistent
//   record.

/// Lifecycle state of a semantic conflict artifact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SemanticConflictStatus {
    /// Conflict detected and awaiting review.
    Open,
    /// Under active review or adjudication.
    UnderReview,
    /// Resolved; kept for drift-analysis history.
    Resolved,
    /// Dismissed as a false positive; kept for audit trail.
    Dismissed,
}

/// A reference to an affected symbol within a semantic conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AffectedSymbolRef {
    /// File path containing the symbol.
    pub file_path: String,
    /// Fully qualified symbol name.
    pub symbol_name: String,
    /// The branch or node that introduced the change.
    pub source_node_id: String,
}

/// A first-class semantic conflict artifact. Persists beyond merge
/// warnings and survives resolution for future drift analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticConflictArtifact {
    /// Unique identifier for this conflict. Used for deduplication:
    /// the same trigger on the same artifact set must not create a
    /// second record.
    pub conflict_id: String,
    /// Which trigger condition produced this conflict.
    pub trigger: SemanticConflictTrigger,
    /// Current lifecycle status.
    pub status: SemanticConflictStatus,
    /// The symbols involved in the conflict.
    pub affected_symbols: Vec<AffectedSymbolRef>,
    /// The node/branch IDs whose concurrent changes caused the
    /// conflict.
    pub conflicting_node_ids: Vec<String>,
    /// Human-readable description of the semantic conflict.
    pub description: String,
    /// Whether promotion is blocked while this conflict is open.
    pub blocks_promotion: bool,
    /// Resolution summary, populated when status transitions to
    /// Resolved or Dismissed.
    pub resolution_summary: Option<String>,
}

// ── ROB-019: Provisional retention window policy ────────────────────────
//
// CSV guardrail (M6): "Define provisional retention windows."
// Caution: "Do not archive artifacts still needed for unresolved
//   conflicts or active review."
// Acceptance: schema validation.

/// The scope of artifacts a retention window applies to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RetentionScope {
    /// Cycle event records and associated metrics.
    CycleEvents,
    /// Task attempt records and worker outputs.
    TaskAttempts,
    /// Certification results and formal-claim artifacts.
    CertificationResults,
    /// Semantic conflict artifacts (ROB-018).
    ConflictArtifacts,
    /// Review artifacts and approval records.
    ReviewArtifacts,
    /// Session heartbeat and diagnostic logs.
    DiagnosticLogs,
}

/// An exception that prevents an artifact from being archived even
/// when the retention window has expired.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RetentionException {
    /// Artifact is referenced by an unresolved semantic conflict.
    UnresolvedConflict,
    /// Artifact is under active human review.
    ActiveReview,
    /// Artifact is a dependency of a currently-running task.
    ActiveTaskDependency,
    /// Artifact is flagged for audit retention by policy.
    AuditHold,
}

/// Retention window policy for a given scope. Artifacts within scope
/// are retained for at least `min_retention_days` and then become
/// eligible for archival, unless an exception applies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionWindowPolicy {
    /// Which class of artifacts this window applies to.
    pub scope: RetentionScope,
    /// Minimum number of days to retain before archival eligibility.
    pub min_retention_days: u32,
    /// Maximum number of days to retain before mandatory archival
    /// (0 = no maximum, retain indefinitely until exception clears).
    pub max_retention_days: u32,
    /// Exceptions that prevent archival even after the window expires.
    pub exceptions: Vec<RetentionException>,
}

// ── ROB-020: Archive and compaction policy ──────────────────────────────
//
// CSV guardrail (M6): "Define archive/compaction policy."
// Caution: "Do not archive artifacts still needed for unresolved
//   conflicts or active review."
// Acceptance: schema validation.

/// A class of artifact eligible for compression during archival.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CompressibleArtifactKind {
    /// Raw worker output payloads.
    WorkerOutput,
    /// Event record payloads (after retention window expires).
    EventPayloads,
    /// Diagnostic and heartbeat logs.
    DiagnosticLogs,
    /// Superseded plan snapshots.
    SupersededPlans,
}

/// Rules governing how archived artifacts may be rebuilt or restored.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RebuildRule {
    /// Archived artifact can be fully restored from the archive.
    FullRestore,
    /// Only metadata can be restored; payload is discarded.
    MetadataOnly,
    /// Archived artifact is permanently deleted after compaction.
    Permanent,
}

/// Policy governing archive and compaction of artifacts that have
/// passed their retention window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveCompactionPolicy {
    /// Which artifact kinds may be compressed during archival.
    pub compressible_kinds: Vec<CompressibleArtifactKind>,
    /// Whether append-only event streams are protected from compaction.
    /// When true, event records are never compacted -- only their
    /// payloads may be compressed.
    pub append_only_protection: bool,
    /// Rebuild rule for each compressible kind. If a kind is not listed,
    /// the default is FullRestore.
    pub rebuild_rules: Vec<ArchiveRebuildEntry>,
}

/// Maps a compressible artifact kind to its rebuild rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveRebuildEntry {
    pub kind: CompressibleArtifactKind,
    pub rule: RebuildRule,
}

// ── Enforcement functions (Tier 3) ──────────────────────────────────────
//
// Pure functions that apply robustness policy to concrete inputs.
// No I/O -- callers supply the values.

/// Result of checking input tokens against a context budget.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BudgetDecision {
    /// Input is within the allowed budget.
    Within,
    /// Input exceeds the budget.
    Exceeded {
        /// The policy limit.
        limit: u32,
        /// The actual token count.
        actual: u32,
    },
}

/// Check whether `input_tokens` fits within the context budget.
pub fn enforce_context_budget(
    policy: &ContextBudgetPolicy,
    input_tokens: u32,
) -> BudgetDecision {
    if input_tokens > policy.max_input_tokens {
        BudgetDecision::Exceeded {
            limit: policy.max_input_tokens,
            actual: input_tokens,
        }
    } else {
        BudgetDecision::Within
    }
}

/// Classification of worker output after inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputClassification {
    /// Output is valid and within expectations.
    Valid,
    /// Output is empty (zero-length or whitespace-only).
    Empty,
    /// Output is malformed in a specific way.
    Malformed(MalformedOutputKind),
}

/// Classify worker output according to the output preference policy.
///
/// Rules:
/// - Empty / whitespace-only output is always `Empty`.
/// - Output exceeding 100 000 characters is `Malformed(ExceededLengthBudget)`.
/// - Under `Strict` preference, output must parse as valid JSON.
/// - Otherwise, output is `Valid`.
pub fn classify_output(
    output: &str,
    preference: OutputPreference,
) -> OutputClassification {
    if output.trim().is_empty() {
        return OutputClassification::Empty;
    }

    if output.len() > 100_000 {
        return OutputClassification::Malformed(
            MalformedOutputKind::ExceededLengthBudget,
        );
    }

    if preference == OutputPreference::Strict {
        if serde_json::from_str::<serde_json::Value>(output).is_err() {
            return OutputClassification::Malformed(
                MalformedOutputKind::InvalidJson,
            );
        }
    }

    OutputClassification::Valid
}

/// Determine the timeout (in seconds) to apply from the robustness
/// policy's parse-retry settings.
///
/// Returns the backoff base converted to seconds, or 0 if backoff
/// strategy is `None`.
pub fn timeout_from_policy(policy: &ParseRetryPolicy) -> u64 {
    match policy.backoff_strategy {
        BackoffStrategy::None => 0,
        BackoffStrategy::FixedMs => (policy.backoff_base_ms as u64) / 1000,
        BackoffStrategy::Exponential => (policy.backoff_base_ms as u64) / 1000,
    }
}

/// Return the retry budget from the parse-retry policy.
pub fn retry_budget_from_policy(policy: &ParseRetryPolicy) -> u32 {
    policy.max_attempts
}

// ── Aggregate policy snapshot ───────────────────────────────────────────

/// Top-level robustness policy snapshot aggregating all ROB-001..ROB-020
/// sub-policies. Serializable for storage, versioning, and control-plane
/// consumption.
///
/// Policy revisions supersede earlier revisions; the control plane must
/// never silently mutate live semantics (CSV idempotency rule).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RobustnessPolicy {
    /// Monotonically increasing revision number for policy versioning.
    pub revision: u32,

    // ROB-002
    pub output_preference: OutputPreference,
    // ROB-003
    pub fuzzy_repair: FuzzyRepairPolicy,
    // ROB-004
    pub parse_retry: ParseRetryPolicy,
    // ROB-005
    pub parse_failure_fallback: ParseFailureFallbackPolicy,
    // ROB-006
    pub context_budgets: Vec<ContextBudgetPolicy>,
    // ROB-007
    pub retrieval_ranking: RetrievalRankingPolicy,
    // ROB-008
    pub summary_artifact: SummaryArtifactPolicy,
    // ROB-009
    pub dependency_expansion: DependencyExpansionPolicy,
    // ROB-010
    pub full_history_denial: FullHistoryDenialPolicy,
    // ROB-011
    pub planning_iteration_cap: PlanningIterationCap,
    // ROB-012
    pub unresolved_question_budget: UnresolvedQuestionBudget,
    // ROB-013
    pub planning_bailout: PlanningBailoutPolicy,
    // ROB-014
    pub gate_lattice: GateLattice,
    // ROB-015
    pub downstream_admissibility: DownstreamAdmissibilityPolicy,
    // ROB-016
    pub symbol_overlap: SymbolOverlapPolicy,
    // ROB-017
    pub semantic_conflict_triggers: SemanticConflictTriggerPolicy,
    // ROB-019
    pub retention_windows: Vec<RetentionWindowPolicy>,
    // ROB-020
    pub archive_compaction: ArchiveCompactionPolicy,
}
