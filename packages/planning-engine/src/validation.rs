//! Plan validation and dispatch gating (PLAN-018, PLAN-019, PLAN-020).
//!
//! These are concrete, deterministic functions (not traits) because they
//! operate on pure data from the plan gate schema.  No AI worker is
//! needed -- the control plane calls these directly.
//!
//! CSV acceptance criterion (shared): "The planning rule or schema is
//! explicit, machine-readable, and sufficient for later control-plane
//! execution."
//!
//! CSV caution: "Do not let planning prose substitute for executable
//! gate logic; do not unlock implementation from weak plans."

use crate::schemas::{ConditionEval, GateCondition, GateStatus, PlanGateDefinition};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Completeness score for a plan.
///
/// Each boolean field indicates whether the corresponding planning
/// artifact is present and valid.  The `overall` score is the fraction
/// of fields that are `true` (0.0 -- 1.0).
///
/// CSV expected output: "Plan completeness scoring that can drive plan
/// gate decisions and review policy."
///
/// CSV proof hooks: "score reproducibility check; threshold simulation."
///
/// CSV dependencies: PLAN-005, PLAN-007, PLAN-008, PLAN-009.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletenessScore {
    /// Overall completeness as a fraction (0.0 -- 1.0).
    pub overall: f64,
    /// Gate condition: `ObjectiveSummarized` is passing.
    pub has_objective: bool,
    /// Gate condition: `ArchitectureDrafted` is passing.
    pub has_architecture: bool,
    /// Gate condition: `MilestoneTreeCreated` is passing.
    pub has_milestones: bool,
    /// Gate conditions: `DependenciesAcyclic` AND `DependenciesResolved`
    /// are both passing.
    pub has_dependencies: bool,
    /// Gate condition: `AcceptanceCriteriaDefined` is passing.
    pub has_acceptance_criteria: bool,
    /// Gate conditions: `InvariantsExtracted` AND `InvariantsHolding`
    /// are both passing.
    pub has_invariants: bool,
    /// Gate condition: `RisksIdentified` is passing.
    pub has_risks: bool,
    /// Gate condition: `UnresolvedQuestionsBelowBudget` is passing.
    pub unresolved_questions_within_budget: bool,
}

/// Evaluate plan completeness from a [`PlanGateDefinition`].
///
/// The scoring logic is deterministic and reproducible: it reads the
/// `condition_entries` on the gate and checks each condition's `eval`
/// field.  The `overall` score is the count of passing sub-scores
/// divided by the total number of sub-score fields (8).
///
/// # Idempotency
///
/// Pure function -- calling it multiple times on the same gate yields
/// the same score.
pub fn score_plan_completeness(gate: &PlanGateDefinition) -> CompletenessScore {
    let lookup = |cond: GateCondition| -> bool {
        gate.condition_entries
            .iter()
            .any(|e| e.condition == cond && e.eval == ConditionEval::Pass)
    };

    let has_objective = lookup(GateCondition::ObjectiveSummarized);
    let has_architecture = lookup(GateCondition::ArchitectureDrafted);
    let has_milestones = lookup(GateCondition::MilestoneTreeCreated);
    let has_dependencies = lookup(GateCondition::DependenciesAcyclic)
        && lookup(GateCondition::DependenciesResolved);
    let has_acceptance_criteria = lookup(GateCondition::AcceptanceCriteriaDefined);
    let has_invariants = lookup(GateCondition::InvariantsExtracted)
        && lookup(GateCondition::InvariantsHolding);
    let has_risks = lookup(GateCondition::RisksIdentified);
    let unresolved_questions_within_budget =
        lookup(GateCondition::UnresolvedQuestionsBelowBudget);

    let checks: [bool; 8] = [
        has_objective,
        has_architecture,
        has_milestones,
        has_dependencies,
        has_acceptance_criteria,
        has_invariants,
        has_risks,
        unresolved_questions_within_budget,
    ];
    let passing = checks.iter().filter(|&&b| b).count();
    let overall = passing as f64 / checks.len() as f64;

    CompletenessScore {
        overall,
        has_objective,
        has_architecture,
        has_milestones,
        has_dependencies,
        has_acceptance_criteria,
        has_invariants,
        has_risks,
        unresolved_questions_within_budget,
    }
}

/// Severity of a validation failure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// Blocks the plan gate from passing.
    Error,
    /// Does not block but should be addressed.
    Warning,
}

impl fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
        }
    }
}

/// A single structured validation failure.
///
/// Each failure identifies the gate condition that failed, a
/// human-readable reason, and a severity level.
///
/// CSV expected output: "A machine-readable schema, rule, or planning
/// component for the named planning concept."
///
/// CSV proof hooks: "schema validation; dependency consistency check;
/// plan-gate simulation; cross-doc consistency check."
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationFailure {
    /// The gate condition that failed.
    pub condition: GateCondition,
    /// Human-readable explanation of why it failed.
    pub reason: String,
    /// Severity classification.
    pub severity: ValidationSeverity,
}

impl fmt::Display for ValidationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}: {}", self.condition, self.severity, self.reason)
    }
}

/// Validate a plan gate and return structured failure reasons.
///
/// Returns an empty `Vec` when all conditions pass. Each failing or
/// not-yet-evaluated condition produces a [`ValidationFailure`] entry.
///
/// Conditions that are `NotEvaluated` are reported as warnings (they do
/// not block gating but indicate incomplete evaluation).  Conditions
/// that are `Fail` are reported as errors.
///
/// # Idempotency
///
/// Pure function -- deterministic for a given gate state.
pub fn validate_plan(gate: &PlanGateDefinition) -> Vec<ValidationFailure> {
    let mut failures = Vec::new();

    for entry in &gate.condition_entries {
        match entry.eval {
            ConditionEval::Pass => { /* nothing to report */ }
            ConditionEval::Fail => {
                failures.push(ValidationFailure {
                    condition: entry.condition,
                    reason: reason_for(entry.condition),
                    severity: ValidationSeverity::Error,
                });
            }
            ConditionEval::NotEvaluated => {
                failures.push(ValidationFailure {
                    condition: entry.condition,
                    reason: format!(
                        "{} has not been evaluated yet",
                        label_for(entry.condition)
                    ),
                    severity: ValidationSeverity::Warning,
                });
            }
        }
    }

    // Extra check: if the unresolved question count exceeds the budget,
    // ensure we flag it even if the condition entry is somehow missing.
    if gate.unresolved_question_count > gate.unresolved_question_budget {
        let already_reported = failures
            .iter()
            .any(|f| f.condition == GateCondition::UnresolvedQuestionsBelowBudget);
        if !already_reported {
            failures.push(ValidationFailure {
                condition: GateCondition::UnresolvedQuestionsBelowBudget,
                reason: format!(
                    "unresolved blocking questions ({}) exceed budget ({})",
                    gate.unresolved_question_count, gate.unresolved_question_budget
                ),
                severity: ValidationSeverity::Error,
            });
        }
    }

    failures
}

/// Human-readable label for a gate condition.
fn label_for(cond: GateCondition) -> &'static str {
    match cond {
        GateCondition::ObjectiveSummarized => "Objective summary",
        GateCondition::ArchitectureDrafted => "Architecture draft",
        GateCondition::MilestoneTreeCreated => "Milestone tree",
        GateCondition::AcceptanceCriteriaDefined => "Acceptance criteria",
        GateCondition::DependenciesAcyclic => "Dependency acyclicity",
        GateCondition::DependenciesResolved => "Dependency resolution",
        GateCondition::InvariantsExtracted => "Invariant extraction",
        GateCondition::InvariantsHolding => "Invariant holding",
        GateCondition::RisksIdentified => "Risk identification",
        GateCondition::UnresolvedQuestionsBelowBudget => "Unresolved question budget",
    }
}

/// Generate a human-readable failure reason for a gate condition.
fn reason_for(cond: GateCondition) -> String {
    match cond {
        GateCondition::ObjectiveSummarized => {
            "objective is missing summary, desired outcome, or success metric".into()
        }
        GateCondition::ArchitectureDrafted => {
            "no accepted architecture draft exists".into()
        }
        GateCondition::MilestoneTreeCreated => {
            "milestone tree is missing or has no milestone nodes".into()
        }
        GateCondition::AcceptanceCriteriaDefined => {
            "one or more milestones lack acceptance criteria".into()
        }
        GateCondition::DependenciesAcyclic => {
            "dependency graph contains a cycle among Blocks edges".into()
        }
        GateCondition::DependenciesResolved => {
            "one or more Blocks edges reference nonexistent entities".into()
        }
        GateCondition::InvariantsExtracted => {
            "no invariants have been defined".into()
        }
        GateCondition::InvariantsHolding => {
            "one or more PlanValidation-scoped invariants are not Holding".into()
        }
        GateCondition::RisksIdentified => {
            "no risks have been identified or assessed".into()
        }
        GateCondition::UnresolvedQuestionsBelowBudget => {
            "unresolved blocking questions exceed the allowed budget".into()
        }
    }
}

/// Dispatch decision returned by the gate check.
///
/// This is the single enforcement point that prevents implementation
/// dispatch from weak plans.
///
/// CSV expected output: "Executable enforcement that blocks
/// implementation dispatch from weak plans."
///
/// CSV proof hooks: "dispatch denial simulation; escalation trigger
/// simulation."
///
/// CSV dependencies: PLAN-009, PLAN-018, ROB-011, ROB-012, ROB-013.
///
/// CSV caution: "Do not let planning prose substitute for executable
/// gate logic; do not unlock implementation from weak plans."
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchDecision {
    /// Whether implementation dispatch is allowed.
    pub allowed: bool,
    /// The gate's overall status at the time of the check.
    pub gate_status: GateStatus,
    /// Blocking reasons (non-empty when `allowed` is `false`).
    pub blocking_reasons: Vec<ValidationFailure>,
    /// The completeness score at the time of the check.
    pub completeness: CompletenessScore,
}

/// Check whether implementation dispatch is allowed.
///
/// Implementation is allowed only when:
///
/// 1. The gate status is `Satisfied` (all conditions pass), **or**
/// 2. The gate status is `Overridden` (manual policy override with a
///    reason).
///
/// In all other cases, dispatch is denied and the `blocking_reasons`
/// field explains why.
///
/// # Idempotency
///
/// Pure function -- deterministic for a given gate state.
///
/// # CSV enforcement
///
/// This function is the executable implementation of: "Do not unlock
/// implementation until the plan gate and unresolved-question budget
/// are satisfied."
pub fn check_dispatch_gate(gate: &PlanGateDefinition) -> DispatchDecision {
    let completeness = score_plan_completeness(gate);
    let all_failures = validate_plan(gate);

    // Only error-severity failures block dispatch.
    let blocking_reasons: Vec<ValidationFailure> = all_failures
        .into_iter()
        .filter(|f| f.severity == ValidationSeverity::Error)
        .collect();

    let allowed = match gate.current_status {
        GateStatus::Satisfied => blocking_reasons.is_empty(),
        GateStatus::Overridden => true,
        GateStatus::Open => false,
    };

    DispatchDecision {
        allowed,
        gate_status: gate.current_status,
        blocking_reasons,
        completeness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::{ConditionEval, GateConditionEntry, GateStatus, PlanGateDefinition};
    use chrono::Utc;

    /// Helper: build a gate with all conditions set to the given eval.
    fn gate_with_all(eval: ConditionEval, status: GateStatus) -> PlanGateDefinition {
        use GateCondition::*;
        let conditions = [
            ObjectiveSummarized,
            ArchitectureDrafted,
            MilestoneTreeCreated,
            AcceptanceCriteriaDefined,
            DependenciesAcyclic,
            DependenciesResolved,
            InvariantsExtracted,
            InvariantsHolding,
            RisksIdentified,
            UnresolvedQuestionsBelowBudget,
        ];
        PlanGateDefinition {
            gate_id: "gate-1".into(),
            plan_id: "plan-1".into(),
            condition_entries: conditions
                .iter()
                .map(|&c| GateConditionEntry {
                    condition: c,
                    eval,
                })
                .collect(),
            current_status: status,
            unresolved_question_budget: 3,
            unresolved_question_count: 0,
            override_reason: None,
            evaluated_at: Utc::now(),
        }
    }

    #[test]
    fn completeness_all_pass_yields_1_0() {
        let gate = gate_with_all(ConditionEval::Pass, GateStatus::Satisfied);
        let score = score_plan_completeness(&gate);
        assert!((score.overall - 1.0).abs() < f64::EPSILON);
        assert!(score.has_objective);
        assert!(score.has_architecture);
        assert!(score.has_milestones);
        assert!(score.has_dependencies);
        assert!(score.has_acceptance_criteria);
        assert!(score.has_invariants);
        assert!(score.has_risks);
        assert!(score.unresolved_questions_within_budget);
    }

    #[test]
    fn completeness_all_fail_yields_0_0() {
        let gate = gate_with_all(ConditionEval::Fail, GateStatus::Open);
        let score = score_plan_completeness(&gate);
        assert!((score.overall - 0.0).abs() < f64::EPSILON);
        assert!(!score.has_objective);
    }

    #[test]
    fn completeness_partial_pass() {
        let mut gate = gate_with_all(ConditionEval::Fail, GateStatus::Open);
        // Pass only ObjectiveSummarized and RisksIdentified
        for entry in &mut gate.condition_entries {
            if entry.condition == GateCondition::ObjectiveSummarized
                || entry.condition == GateCondition::RisksIdentified
            {
                entry.eval = ConditionEval::Pass;
            }
        }
        let score = score_plan_completeness(&gate);
        // 2 of 8 sub-scores pass: objective=true, risks=true
        assert!((score.overall - 0.25).abs() < f64::EPSILON);
        assert!(score.has_objective);
        assert!(score.has_risks);
        assert!(!score.has_architecture);
    }

    #[test]
    fn dependencies_require_both_conditions() {
        let mut gate = gate_with_all(ConditionEval::Fail, GateStatus::Open);
        // Pass only DependenciesAcyclic but not DependenciesResolved
        for entry in &mut gate.condition_entries {
            if entry.condition == GateCondition::DependenciesAcyclic {
                entry.eval = ConditionEval::Pass;
            }
        }
        let score = score_plan_completeness(&gate);
        assert!(!score.has_dependencies);
    }

    #[test]
    fn validate_all_pass_returns_empty() {
        let gate = gate_with_all(ConditionEval::Pass, GateStatus::Satisfied);
        let failures = validate_plan(&gate);
        assert!(failures.is_empty());
    }

    #[test]
    fn validate_fail_returns_errors() {
        let gate = gate_with_all(ConditionEval::Fail, GateStatus::Open);
        let failures = validate_plan(&gate);
        assert_eq!(failures.len(), 10); // all 10 conditions
        assert!(failures
            .iter()
            .all(|f| f.severity == ValidationSeverity::Error));
    }

    #[test]
    fn validate_not_evaluated_returns_warnings() {
        let gate = gate_with_all(ConditionEval::NotEvaluated, GateStatus::Open);
        let failures = validate_plan(&gate);
        assert_eq!(failures.len(), 10);
        assert!(failures
            .iter()
            .all(|f| f.severity == ValidationSeverity::Warning));
    }

    #[test]
    fn validate_catches_budget_overrun_even_without_entry() {
        let mut gate = gate_with_all(ConditionEval::Pass, GateStatus::Satisfied);
        // Remove the UnresolvedQuestionsBelowBudget entry
        gate.condition_entries
            .retain(|e| e.condition != GateCondition::UnresolvedQuestionsBelowBudget);
        // But set the count above budget
        gate.unresolved_question_count = 5;
        gate.unresolved_question_budget = 2;
        let failures = validate_plan(&gate);
        assert_eq!(failures.len(), 1);
        assert_eq!(
            failures[0].condition,
            GateCondition::UnresolvedQuestionsBelowBudget
        );
    }

    #[test]
    fn dispatch_allowed_when_all_pass() {
        let gate = gate_with_all(ConditionEval::Pass, GateStatus::Satisfied);
        let decision = check_dispatch_gate(&gate);
        assert!(decision.allowed);
        assert_eq!(decision.gate_status, GateStatus::Satisfied);
        assert!(decision.blocking_reasons.is_empty());
    }

    #[test]
    fn dispatch_denied_when_gate_open() {
        let gate = gate_with_all(ConditionEval::Fail, GateStatus::Open);
        let decision = check_dispatch_gate(&gate);
        assert!(!decision.allowed);
        assert_eq!(decision.gate_status, GateStatus::Open);
        assert!(!decision.blocking_reasons.is_empty());
    }

    #[test]
    fn dispatch_allowed_when_overridden() {
        let mut gate = gate_with_all(ConditionEval::Fail, GateStatus::Overridden);
        gate.override_reason = Some("manual override for prototype".into());
        let decision = check_dispatch_gate(&gate);
        assert!(decision.allowed);
        assert_eq!(decision.gate_status, GateStatus::Overridden);
    }

    #[test]
    fn dispatch_includes_completeness_score() {
        let gate = gate_with_all(ConditionEval::Pass, GateStatus::Satisfied);
        let decision = check_dispatch_gate(&gate);
        assert!((decision.completeness.overall - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dispatch_warnings_do_not_block() {
        // All conditions are NotEvaluated (warnings), gate is Open
        let gate = gate_with_all(ConditionEval::NotEvaluated, GateStatus::Open);
        let decision = check_dispatch_gate(&gate);
        // Gate is Open so dispatch is denied regardless
        assert!(!decision.allowed);
        // But blocking_reasons should be empty since NotEvaluated = Warning
        assert!(decision.blocking_reasons.is_empty());
    }
}
