//! Recursive improvement.
//!
//! This crate provides the types and schemas for recursive
//! self-improvement with strict safety gates, drift checks,
//! anti-self-promotion, and durable memory.
//!
//! Key design rules:
//! - Do not allow hidden self-modification.
//! - Do not let self-generated artifacts promote the same loop (P0!).
//! - Do not let recursive memory override current objectives.
//! - Every recursive action must leave a durable artifact (never_silent).
//! - Drift checks detect semantic drift, not just file diffs.

pub mod comparison;
pub mod drift;
pub mod memory;
pub mod objective;
pub mod repo_target;
pub mod report;
pub mod safety_gates;
pub mod scoring;
pub mod self_promotion;
pub mod templates;

// Re-export primary types for ergonomic imports.
pub use comparison::{ComparisonArtifact, ComparisonBaseline, MetricDelta, RegressionRisk};
pub use drift::{
    ApprovalDrift, DriftCheckArtifact, DriftKind, PolicyDrift, SchemaDrift, SkillDrift,
};
pub use memory::{LearningReinjection, MemoryEntry, RecursiveMemory, ReuseSignal, SupersessionChain};
pub use objective::{ObjectiveClassification, SelfImprovementObjective, TrustBoundaryImpact};
pub use repo_target::{
    AllowDenyRule, RepoTargetPolicy, RepoTargetRule, TargetScope, WorktreeIsolationRequirement,
};
pub use report::{NextRecommendation, RecursiveReport, ReportSection};
pub use safety_gates::{AllowedActions, GateLevel, RecursiveGateCondition, SafetyGateLattice};
pub use scoring::{LoopScore, ScoreBreakdown, ScoringInput};
pub use self_promotion::{
    DenialResult, OverrideRequest, OverrideStatus, PromotionAttempt, SelfPromotionDenialRule,
};
pub use templates::{RollbackAnchor, SelfImprovementTemplate, TemplatePhase};
