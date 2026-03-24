//! Planning layer readiness for Lean/Isabelle validation (FRM-010).
//!
//! Readiness contracts, naming conventions, proof-candidate inventory,
//! and integration boundary notes ONLY -- no theorem-level integration
//! in this milestone.
//!
//! Design rules:
//! - NO Lean or Isabelle pulled into the hot path or current dependency
//!   chain. This milestone is readiness only.
//! - `runtime_dependency_assertion` MUST be `false` on all readiness
//!   contracts, asserting that no prover is a current runtime dependency.
//! - Do not let readiness work reintroduce prover-specific coupling into
//!   the orchestration core.
//!
//! CSV guardrail: "Prepare the planning layer so a later Lean or
//! Isabelle integration can attach to stable exported predicates without
//! becoming a current implementation dependency."
//!
//! Scope: Readiness contracts, naming conventions, proof-candidate
//! inventory, and integration boundary notes only; no theorem-level
//! integration in this milestone.
//!
//! Acceptance: The project is ready for later theorem-level integration
//! without changing current runtime or product boundaries.
//!
//! Proof hooks: readiness checklist review; boundary contract consistency
//! check; no-runtime-dependency assertion.
//!
//! Caution: Do not let readiness work propose current runtime dependence
//! on Lean or Isabelle.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Naming conventions ──────────────────────────────────────────────────

/// A naming convention for formal entities (predicates, exports, facts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingConvention {
    /// The domain this convention applies to (e.g. "predicates", "exports").
    pub domain: String,
    /// Pattern description (e.g. "snake_case prefix with domain tag").
    pub pattern: String,
    /// Example conforming name.
    pub example: String,
    /// Rationale for this convention.
    pub rationale: String,
}

// ─── Proof candidate inventory ───────────────────────────────────────────

/// Readiness level of a proof candidate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProofReadinessLevel {
    /// Predicate exists and is serializable but not yet exercised.
    Defined,
    /// Predicate has been exercised against test fixtures.
    Exercised,
    /// Predicate has been exported in the backend-neutral format.
    Exported,
    /// Predicate has naming and boundary notes ready for formal attach.
    ReadyForAttach,
}

/// A single proof candidate in the inventory.
///
/// This does NOT mean a proof exists; it means the predicate is
/// identified as something a later prover integration could target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofCandidateInventory {
    /// The predicate_id of the candidate.
    pub predicate_id: String,
    /// Human-readable name.
    pub name: String,
    /// Which FRM item defined this predicate.
    pub source_item: String,
    /// Current readiness level.
    pub readiness_level: ProofReadinessLevel,
    /// Known blockers for moving to the next readiness level.
    pub blockers: Vec<String>,
    /// Notes for the future formal integration team.
    pub integration_notes: String,
}

// ─── FRM-010: Readiness contract ─────────────────────────────────────────

/// FRM-010 -- Readiness contract for later Lean/Isabelle validation.
///
/// This is the top-level artifact that asserts the planning layer is
/// ready for future theorem-level integration without requiring any
/// prover as a current runtime dependency.
///
/// The `runtime_dependency_assertion` field MUST be `false`, meaning
/// no prover is wired into the current runtime. If this field is ever
/// `true`, the contract is invalid and must be rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadinessContract {
    /// Contract identifier.
    pub contract_id: String,
    /// Contract version (semver).
    pub contract_version: String,
    /// Naming conventions adopted for formal entities.
    pub naming_conventions: Vec<NamingConvention>,
    /// Proof candidates identified across FRM-001 through FRM-009.
    pub proof_candidates: Vec<ProofCandidateInventory>,
    /// Integration boundary notes (what the formal layer may and may not
    /// touch in the current runtime).
    pub integration_boundary_notes: Vec<String>,
    /// MUST be `false`. Asserts that no theorem prover (Lean, Isabelle,
    /// Coq, etc.) is a current runtime dependency.
    pub runtime_dependency_assertion: bool,
    /// When this contract was created.
    pub created_at: DateTime<Utc>,
    /// When this contract was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Validate a readiness contract.
///
/// Returns a list of validation errors. An empty list means the contract
/// is valid.
///
/// Key checks:
/// 1. `runtime_dependency_assertion` must be `false`.
/// 2. `contract_version` must be non-empty.
/// 3. At least one proof candidate must be listed.
/// 4. At least one naming convention must be specified.
/// 5. At least one integration boundary note must be present.
pub fn validate_readiness_contract(contract: &ReadinessContract) -> Vec<String> {
    let mut errors = Vec::new();

    if contract.runtime_dependency_assertion {
        errors.push(
            "runtime_dependency_assertion must be false: no prover may be a current runtime dependency".into(),
        );
    }

    if contract.contract_version.is_empty() {
        errors.push("contract_version must be non-empty".into());
    }

    if contract.proof_candidates.is_empty() {
        errors.push("at least one proof candidate must be listed".into());
    }

    if contract.naming_conventions.is_empty() {
        errors.push("at least one naming convention must be specified".into());
    }

    if contract.integration_boundary_notes.is_empty() {
        errors.push("at least one integration boundary note must be present".into());
    }

    errors
}

// ─── FRM-010: Readiness report ──────────────────────────────────────────

/// FRM-010 -- Readiness assessment for formal export.
///
/// Reports whether a plan is ready for formal export. The checks are:
/// 1. At least one invariant must be defined.
/// 2. Dependencies must be provided (non-zero count implies the graph
///    exists; acyclicity is verified separately by the planning engine).
/// 3. Gate conditions must be defined (non-empty condition list).
///
/// This is a lightweight pre-flight check, not a full formal proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadinessReport {
    /// Whether the plan is ready for formal export.
    pub ready: bool,
    /// Human-readable reasons why the plan is not ready (empty if ready).
    pub blockers: Vec<String>,
    /// Number of invariants found.
    pub invariant_count: usize,
    /// Number of dependency edges found.
    pub dependency_count: usize,
    /// Whether gate conditions are defined.
    pub gate_conditions_defined: bool,
}

/// Check whether a plan is ready for formal export.
///
/// Returns a [`ReadinessReport`] indicating readiness and any blockers.
///
/// Checks performed:
/// - `invariant_count > 0` (at least one invariant must exist).
/// - `dependency_count > 0` (dependency graph must be non-empty; acyclicity
///   is the responsibility of the planning engine, not this check).
/// - Gate must have at least one condition entry defined.
pub fn check_readiness_for_export(
    gate: &planning_engine::schemas::PlanGateDefinition,
    invariant_count: usize,
    dependency_count: usize,
) -> ReadinessReport {
    let mut blockers = Vec::new();

    if invariant_count == 0 {
        blockers.push("No invariants defined; at least one plan invariant is required.".into());
    }

    if dependency_count == 0 {
        blockers.push(
            "No dependency edges found; the dependency graph must be non-empty.".into(),
        );
    }

    let gate_conditions_defined = !gate.condition_entries.is_empty();
    if !gate_conditions_defined {
        blockers.push(
            "Gate has no condition entries; at least one gate condition must be defined.".into(),
        );
    }

    ReadinessReport {
        ready: blockers.is_empty(),
        blockers,
        invariant_count,
        dependency_count,
        gate_conditions_defined,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn valid_contract() -> ReadinessContract {
        ReadinessContract {
            contract_id: "rc-001".into(),
            contract_version: "1.0.0".into(),
            naming_conventions: vec![NamingConvention {
                domain: "predicates".into(),
                pattern: "snake_case with domain prefix".into(),
                example: "plan_inv_completeness_001".into(),
                rationale: "Stable identifiers for cross-version replay.".into(),
            }],
            proof_candidates: vec![ProofCandidateInventory {
                predicate_id: "plan_inv_completeness_001".into(),
                name: "Plan completeness invariant".into(),
                source_item: "FRM-001".into(),
                readiness_level: ProofReadinessLevel::Exported,
                blockers: vec![],
                integration_notes: "Stable inputs; ready for attach.".into(),
            }],
            integration_boundary_notes: vec![
                "Formal layer must not import runtime HTTP handlers.".into(),
                "Predicates are read-only consumers of exported JSON.".into(),
            ],
            runtime_dependency_assertion: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    // FRM-010: valid contract passes validation
    #[test]
    fn valid_contract_passes() {
        let contract = valid_contract();
        let errors = validate_readiness_contract(&contract);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    // FRM-010: no-runtime-dependency assertion
    #[test]
    fn runtime_dependency_assertion_must_be_false() {
        let mut contract = valid_contract();
        contract.runtime_dependency_assertion = true;
        let errors = validate_readiness_contract(&contract);
        assert!(errors.iter().any(|e| e.contains("runtime_dependency_assertion")));
    }

    // FRM-010: contract version must be non-empty
    #[test]
    fn empty_contract_version_rejected() {
        let mut contract = valid_contract();
        contract.contract_version = String::new();
        let errors = validate_readiness_contract(&contract);
        assert!(errors.iter().any(|e| e.contains("contract_version")));
    }

    // FRM-010: must have proof candidates
    #[test]
    fn empty_proof_candidates_rejected() {
        let mut contract = valid_contract();
        contract.proof_candidates = vec![];
        let errors = validate_readiness_contract(&contract);
        assert!(errors.iter().any(|e| e.contains("proof candidate")));
    }

    // FRM-010: must have naming conventions
    #[test]
    fn empty_naming_conventions_rejected() {
        let mut contract = valid_contract();
        contract.naming_conventions = vec![];
        let errors = validate_readiness_contract(&contract);
        assert!(errors.iter().any(|e| e.contains("naming convention")));
    }

    // FRM-010: must have boundary notes
    #[test]
    fn empty_boundary_notes_rejected() {
        let mut contract = valid_contract();
        contract.integration_boundary_notes = vec![];
        let errors = validate_readiness_contract(&contract);
        assert!(errors.iter().any(|e| e.contains("boundary note")));
    }

    // FRM-010: readiness contract serialization roundtrip
    #[test]
    fn readiness_contract_roundtrip() {
        let contract = valid_contract();
        let json = serde_json::to_string(&contract).unwrap();
        let back: ReadinessContract = serde_json::from_str(&json).unwrap();
        assert_eq!(back.contract_id, "rc-001");
        assert!(!back.runtime_dependency_assertion);
    }

    // FRM-010: boundary contract consistency
    #[test]
    fn boundary_notes_are_preserved() {
        let contract = valid_contract();
        assert_eq!(contract.integration_boundary_notes.len(), 2);
        assert!(contract.integration_boundary_notes[0]
            .contains("must not import runtime HTTP handlers"));
    }

    // ── ReadinessReport / check_readiness_for_export tests ──────────────

    use planning_engine::schemas::{
        ConditionEval, GateCondition, GateConditionEntry, GateStatus, PlanGateDefinition,
    };

    fn sample_gate(with_conditions: bool) -> PlanGateDefinition {
        PlanGateDefinition {
            gate_id: "gate-001".into(),
            plan_id: "plan-001".into(),
            condition_entries: if with_conditions {
                vec![GateConditionEntry {
                    condition: GateCondition::InvariantsExtracted,
                    eval: ConditionEval::Pass,
                }]
            } else {
                vec![]
            },
            current_status: GateStatus::Open,
            unresolved_question_budget: 3,
            unresolved_question_count: 0,
            override_reason: None,
            evaluated_at: Utc::now(),
        }
    }

    // FRM-010: plan ready when all pre-flight conditions met
    #[test]
    fn readiness_report_ready_when_all_conditions_met() {
        let gate = sample_gate(true);
        let report = check_readiness_for_export(&gate, 2, 3);
        assert!(report.ready);
        assert!(report.blockers.is_empty());
        assert_eq!(report.invariant_count, 2);
        assert_eq!(report.dependency_count, 3);
        assert!(report.gate_conditions_defined);
    }

    // FRM-010: plan not ready when no invariants
    #[test]
    fn readiness_report_blocks_on_zero_invariants() {
        let gate = sample_gate(true);
        let report = check_readiness_for_export(&gate, 0, 3);
        assert!(!report.ready);
        assert!(report.blockers.iter().any(|b| b.contains("invariant")));
    }

    // FRM-010: plan not ready when no dependencies
    #[test]
    fn readiness_report_blocks_on_zero_dependencies() {
        let gate = sample_gate(true);
        let report = check_readiness_for_export(&gate, 1, 0);
        assert!(!report.ready);
        assert!(report.blockers.iter().any(|b| b.contains("dependency")));
    }

    // FRM-010: plan not ready when gate has no conditions
    #[test]
    fn readiness_report_blocks_on_empty_gate_conditions() {
        let gate = sample_gate(false);
        let report = check_readiness_for_export(&gate, 1, 1);
        assert!(!report.ready);
        assert!(!report.gate_conditions_defined);
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("gate") || b.contains("condition")));
    }

    // FRM-010: multiple blockers reported at once
    #[test]
    fn readiness_report_accumulates_all_blockers() {
        let gate = sample_gate(false);
        let report = check_readiness_for_export(&gate, 0, 0);
        assert!(!report.ready);
        assert_eq!(report.blockers.len(), 3);
    }

    // FRM-010: readiness report serialization roundtrip
    #[test]
    fn readiness_report_roundtrip() {
        let gate = sample_gate(true);
        let report = check_readiness_for_export(&gate, 2, 5);
        let json = serde_json::to_string(&report).unwrap();
        let back: ReadinessReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ready, report.ready);
        assert_eq!(back.invariant_count, 2);
        assert_eq!(back.dependency_count, 5);
    }
}
