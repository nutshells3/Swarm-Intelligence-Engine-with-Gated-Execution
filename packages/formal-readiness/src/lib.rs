//! Formal-readiness package.
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

// Re-export primary types for ergonomic imports.
pub use predicates::{
    AcyclicityPredicate, AdmissibilityRequirement, CertificationSelectionPredicate,
    CertificationSelectionReason, ConflictAdmissibilityPredicate, CycleDetectionResult,
    GraphScope, InputType, MilestoneGraphPredicate, MilestoneValidityAspect,
    PlanInvariantCategory, PlanInvariantPredicate, PredicateEvaluation, PredicateInput,
    PredicateOutcome, PromotionConditionCategory, PromotionPredicate, TransitionLegalityMatrix,
    TransitionRule,
};

pub use export::{
    ApprovalEffectFact, CertificationFact, ExportedInput, ExportedPredicate, FormalExport,
    GraphFact, LifecycleStateFact,
};

pub use export::export_plan_for_verification;

pub use consistency::{
    ConsistencyStatus, ProjectionConsistencyCheck, ProjectionDomain, ProjectionMismatch,
    RebuildAction,
};

pub use readiness::{
    NamingConvention, ProofCandidateInventory, ProofReadinessLevel, ReadinessContract,
    ReadinessReport, check_readiness_for_export, validate_readiness_contract,
};
