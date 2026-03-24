//! Formal-readiness package (M8: FRM-001 through FRM-010).
//!
//! Translates planning/orchestration invariants into machine-readable
//! predicates for later formal validation (Lean/Isabelle) WITHOUT
//! making Lean/Isabelle a current dependency.
//!
//! Design constraints:
//! - NO Lean/Isabelle imports or dependencies -- readiness only.
//! - All predicates must be replayable from durable state alone.
//! - Export format must be backend-neutral (no Lean syntax, no
//!   Isabelle syntax).
//! - FRM-010 must explicitly assert no runtime dependency on any prover.
//! - Do not couple runtime to any prover backend; keep
//!   representation-stable and backend-neutral.
//! - Do not sneak in policy changes under formalization.
//!
//! Proof/check hooks: predicate replay; export roundtrip;
//! no-runtime-dependency assertion.

pub mod consistency;
pub mod export;
pub mod predicates;
pub mod readiness;

// ── Re-exports for ergonomic imports ─────────────────────────────────────

// FRM-001 through FRM-007: predicate types
pub use predicates::{
    AcyclicityPredicate, AdmissibilityRequirement, CertificationSelectionPredicate,
    CertificationSelectionReason, ConflictAdmissibilityPredicate, CycleDetectionResult,
    GraphScope, InputType, MilestoneGraphPredicate, MilestoneValidityAspect,
    PlanInvariantCategory, PlanInvariantPredicate, PredicateEvaluation, PredicateInput,
    PredicateOutcome, PromotionConditionCategory, PromotionPredicate, TransitionLegalityMatrix,
    TransitionRule,
};

// FRM-008: export types
pub use export::{
    ApprovalEffectFact, CertificationFact, ExportedInput, ExportedPredicate, FormalExport,
    GraphFact, LifecycleStateFact,
};

// FRM-010: plan-to-formal export bridge
pub use export::export_plan_for_verification;

// FRM-009: consistency check types
pub use consistency::{
    ConsistencyStatus, ProjectionConsistencyCheck, ProjectionDomain, ProjectionMismatch,
    RebuildAction,
};

// FRM-010: readiness contract types
pub use readiness::{
    NamingConvention, ProofCandidateInventory, ProofReadinessLevel, ReadinessContract,
    ReadinessReport, check_readiness_for_export, validate_readiness_contract,
};
