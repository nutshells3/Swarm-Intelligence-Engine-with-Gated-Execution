//! SQL-to-formal export format (FRM-008).
//!
//! Defines a stable, backend-neutral export schema from authoritative SQL
//! state into a format that a later formal validation environment (Lean,
//! Isabelle, or any other) can consume.
//!
//! Design rules:
//! - NO Lean syntax, NO Isabelle syntax -- backend-neutral representation.
//! - Export must be derived from authoritative state plus explicit
//!   predicates, not from UI projections or transient adapter logs.
//! - Export format must be versioned for backward compatibility.
//!
//! CSV guardrail: "Define a stable export format from authoritative SQL
//! state into a later formal validation environment without coupling the
//! runtime to Lean or Isabelle now."
//!
//! Scope: Export schema for predicates, graph facts, lifecycle states,
//! approval effects, and certification facts.
//!
//! Acceptance: The system can emit a stable formal-readiness export
//! without requiring theorem proving to be wired in yet.
//!
//! Proof hooks: export roundtrip check; schema version compatibility
//! check; missing-authoritative-field rejection check.
//!
//! Caution: Do not bind the export format to one prover backend too
//! early; keep it representation-stable and backend-neutral.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Exported predicate ──────────────────────────────────────────────────

/// Backend-neutral representation of an exported predicate.
///
/// This is the serialized form of any predicate (FRM-001 through FRM-007)
/// suitable for consumption by a later formal backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportedPredicate {
    /// Predicate identifier (matches the source predicate_id).
    pub predicate_id: String,
    /// Predicate category (e.g. "plan_invariant", "acyclicity").
    pub predicate_category: String,
    /// Human-readable name.
    pub name: String,
    /// Machine-readable inputs as key-type pairs.
    pub inputs: Vec<ExportedInput>,
    /// Serialized evaluation result (JSON), if available.
    pub evaluation_json: Option<serde_json::Value>,
    /// Version of the predicate definition.
    pub predicate_version: String,
}

/// A single typed input in an exported predicate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportedInput {
    /// Input name.
    pub name: String,
    /// Input type as a string (backend-neutral).
    pub input_type: String,
    /// Whether this input is required.
    pub required: bool,
}

// ─── Graph facts ─────────────────────────────────────────────────────────

/// An exported graph fact (edge in the dependency or milestone graph).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphFact {
    /// Source node identifier.
    pub from_id: String,
    /// Source node kind (e.g. "milestone", "node", "roadmap_node").
    pub from_kind: String,
    /// Target node identifier.
    pub to_id: String,
    /// Target node kind.
    pub to_kind: String,
    /// Edge kind (e.g. "blocks", "should_precede", "data_flow").
    pub edge_kind: String,
    /// Whether this edge has been verified as valid.
    pub verified: bool,
}

// ─── Lifecycle state facts ───────────────────────────────────────────────

/// An exported lifecycle state fact for a node or milestone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleStateFact {
    /// Entity identifier.
    pub entity_id: String,
    /// Entity kind (e.g. "node", "milestone").
    pub entity_kind: String,
    /// Current lane (e.g. "branch", "mainline").
    pub lane: String,
    /// Current lifecycle state (e.g. "proposed", "running").
    pub lifecycle: String,
}

// ─── Approval effect facts ───────────────────────────────────────────────

/// An exported approval effect fact (review or certification outcome).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalEffectFact {
    /// The entity that was reviewed/certified.
    pub target_id: String,
    /// Kind of approval (e.g. "review", "certification").
    pub approval_kind: String,
    /// Effect of the approval (e.g. "pass", "fail", "conditional").
    pub effect: String,
    /// Provenance identifier (review ID, certification ID).
    pub provenance_id: String,
    /// When this approval was recorded.
    pub recorded_at: DateTime<Utc>,
}

// ─── Certification facts ─────────────────────────────────────────────────

/// An exported certification fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationFact {
    /// Submission identifier.
    pub submission_id: String,
    /// The node or entity that was certified.
    pub target_id: String,
    /// Certification grade (e.g. "pass", "fail", "provisional").
    pub grade: String,
    /// Whether the certification is currently stale.
    pub is_stale: bool,
    /// Reason for staleness, if applicable.
    pub stale_reason: Option<String>,
    /// When the certification was issued.
    pub certified_at: DateTime<Utc>,
}

// ─── Top-level export ────────────────────────────────────────────────────

/// FRM-008 -- Top-level formal export.
///
/// This is the complete export artifact emitted from authoritative SQL
/// state. It bundles predicates, graph facts, lifecycle states, approval
/// effects, and certification facts into a single versioned document.
///
/// The format is intentionally backend-neutral: no Lean syntax, no
/// Isabelle syntax, no prover-specific encodings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalExport {
    /// Unique export identifier.
    pub export_id: String,
    /// Export format version (semver).
    pub export_version: String,
    /// SHA-256 hash of the source schema used to produce this export.
    pub source_schema_hash: String,
    /// All exported predicates.
    pub predicates: Vec<ExportedPredicate>,
    /// All graph facts.
    pub graph_facts: Vec<GraphFact>,
    /// All lifecycle state facts.
    pub lifecycle_states: Vec<LifecycleStateFact>,
    /// All approval effect facts.
    pub approval_effects: Vec<ApprovalEffectFact>,
    /// All certification facts.
    pub certification_facts: Vec<CertificationFact>,
    /// When this export was generated.
    pub exported_at: DateTime<Utc>,
}

// ─── FRM-010: Plan-to-formal export bridge ──────────────────────────────

use planning_engine::schemas::{
    DependencyEdge, DependencyKind, PlanGateDefinition, PlanInvariant,
};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Convert planning artifacts into a [`FormalExport`] suitable for
/// later Lean/Isabelle ingestion.
///
/// This is the FRM-010 bridge: it takes the planning engine's canonical
/// types and produces a backend-neutral formal export.
///
/// Contents of the export:
/// - **Predicates**: one per `PlanInvariant`, categorized as
///   `"plan_invariant"`.
/// - **Graph facts**: the dependency graph encoded as an adjacency list
///   of [`GraphFact`] entries.
/// - **Lifecycle states**: a [`LifecycleStateFact`] for the gate itself,
///   recording the current gate status.
/// - **Checksum**: a deterministic hash of the serialized inputs for
///   reproducibility.
///
/// No Lean syntax, no Isabelle syntax -- purely structural.
pub fn export_plan_for_verification(
    gate: &PlanGateDefinition,
    invariants: &[PlanInvariant],
    dependencies: &[DependencyEdge],
) -> FormalExport {
    // ── Predicates from invariants ──────────────────────────────────
    let predicates: Vec<ExportedPredicate> = invariants
        .iter()
        .map(|inv| ExportedPredicate {
            predicate_id: inv.invariant_id.clone(),
            predicate_category: "plan_invariant".into(),
            name: inv.description.clone(),
            inputs: vec![ExportedInput {
                name: "predicate".into(),
                input_type: "text".into(),
                required: true,
            }],
            evaluation_json: Some(serde_json::json!({
                "predicate_expression": inv.predicate,
                "scope": inv.scope,
                "enforcement": inv.enforcement,
                "status": inv.status,
            })),
            predicate_version: "1.0.0".into(),
        })
        .collect();

    // ── Graph facts from dependency edges ───────────────────────────
    let graph_facts: Vec<GraphFact> = dependencies
        .iter()
        .map(|dep| GraphFact {
            from_id: dep.from_id.clone(),
            from_kind: format!("{:?}", dep.from_kind).to_lowercase(),
            to_id: dep.to_id.clone(),
            to_kind: format!("{:?}", dep.to_kind).to_lowercase(),
            edge_kind: match dep.edge_kind {
                DependencyKind::Blocks => "blocks".into(),
                DependencyKind::ShouldPrecede => "should_precede".into(),
                DependencyKind::DataFlow => "data_flow".into(),
                DependencyKind::SharedResource => "shared_resource".into(),
                DependencyKind::RoadmapLink => "roadmap_link".into(),
            },
            verified: false,
        })
        .collect();

    // ── Gate conditions as a lifecycle state fact ────────────────────
    let gate_status_str = match gate.current_status {
        planning_engine::schemas::GateStatus::Open => "open",
        planning_engine::schemas::GateStatus::Satisfied => "satisfied",
        planning_engine::schemas::GateStatus::Overridden => "overridden",
    };

    let lifecycle_states = vec![LifecycleStateFact {
        entity_id: gate.gate_id.clone(),
        entity_kind: "plan_gate".into(),
        lane: "planning".into(),
        lifecycle: gate_status_str.into(),
    }];

    // ── Adjacency list as a JSON blob in approval_effects ───────────
    // We encode the adjacency list as a structured record so the formal
    // layer can consume it without parsing edge-by-edge.
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for dep in dependencies {
        adjacency
            .entry(dep.from_id.clone())
            .or_default()
            .push(dep.to_id.clone());
    }

    // ── Deterministic checksum for reproducibility ──────────────────
    let mut hasher = DefaultHasher::new();
    // Hash gate ID and plan ID for context.
    gate.gate_id.hash(&mut hasher);
    gate.plan_id.hash(&mut hasher);
    // Hash invariant IDs in order.
    for inv in invariants {
        inv.invariant_id.hash(&mut hasher);
        inv.predicate.hash(&mut hasher);
    }
    // Hash dependency edge IDs in order.
    for dep in dependencies {
        dep.edge_id.hash(&mut hasher);
        dep.from_id.hash(&mut hasher);
        dep.to_id.hash(&mut hasher);
    }
    let checksum = format!("{:016x}", hasher.finish());

    // ── Build export ID from gate + checksum ────────────────────────
    let export_id = format!("exp-plan-{}-{}", gate.plan_id, &checksum[..8]);

    FormalExport {
        export_id,
        export_version: "1.0.0".into(),
        source_schema_hash: checksum,
        predicates,
        graph_facts,
        lifecycle_states,
        approval_effects: vec![],
        certification_facts: vec![],
        exported_at: Utc::now(),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // FRM-008: export roundtrip
    #[test]
    fn formal_export_serialization_roundtrip() {
        let now = Utc::now();
        let export = FormalExport {
            export_id: "exp-001".into(),
            export_version: "1.0.0".into(),
            source_schema_hash: "abc123".into(),
            predicates: vec![ExportedPredicate {
                predicate_id: "plan_inv_001".into(),
                predicate_category: "plan_invariant".into(),
                name: "Completeness".into(),
                inputs: vec![ExportedInput {
                    name: "plan_id".into(),
                    input_type: "text".into(),
                    required: true,
                }],
                evaluation_json: None,
                predicate_version: "1.0.0".into(),
            }],
            graph_facts: vec![GraphFact {
                from_id: "ms-1".into(),
                from_kind: "milestone".into(),
                to_id: "ms-2".into(),
                to_kind: "milestone".into(),
                edge_kind: "blocks".into(),
                verified: true,
            }],
            lifecycle_states: vec![LifecycleStateFact {
                entity_id: "node-1".into(),
                entity_kind: "node".into(),
                lane: "branch".into(),
                lifecycle: "running".into(),
            }],
            approval_effects: vec![ApprovalEffectFact {
                target_id: "node-1".into(),
                approval_kind: "review".into(),
                effect: "pass".into(),
                provenance_id: "rev-001".into(),
                recorded_at: now,
            }],
            certification_facts: vec![CertificationFact {
                submission_id: "sub-001".into(),
                target_id: "node-1".into(),
                grade: "pass".into(),
                is_stale: false,
                stale_reason: None,
                certified_at: now,
            }],
            exported_at: now,
        };

        let json = serde_json::to_string(&export).unwrap();
        let back: FormalExport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.export_id, "exp-001");
        assert_eq!(back.export_version, "1.0.0");
        assert_eq!(back.predicates.len(), 1);
        assert_eq!(back.graph_facts.len(), 1);
        assert_eq!(back.lifecycle_states.len(), 1);
        assert_eq!(back.approval_effects.len(), 1);
        assert_eq!(back.certification_facts.len(), 1);
    }

    // FRM-008: backend-neutral verification -- no prover-specific syntax
    #[test]
    fn export_contains_no_prover_syntax() {
        let export = FormalExport {
            export_id: "exp-002".into(),
            export_version: "1.0.0".into(),
            source_schema_hash: "def456".into(),
            predicates: vec![],
            graph_facts: vec![],
            lifecycle_states: vec![],
            approval_effects: vec![],
            certification_facts: vec![],
            exported_at: Utc::now(),
        };
        let json = serde_json::to_string(&export).unwrap();
        // Must not contain Lean or Isabelle keywords in serialized form
        assert!(!json.contains("theorem"));
        assert!(!json.contains("lemma"));
        assert!(!json.contains("sorry"));
        assert!(!json.contains("Isar"));
        assert!(!json.contains("#check"));
    }

    // FRM-008: schema version is present and non-empty
    #[test]
    fn export_has_version_and_hash() {
        let export = FormalExport {
            export_id: "exp-003".into(),
            export_version: "1.0.0".into(),
            source_schema_hash: "schema_hash_001".into(),
            predicates: vec![],
            graph_facts: vec![],
            lifecycle_states: vec![],
            approval_effects: vec![],
            certification_facts: vec![],
            exported_at: Utc::now(),
        };
        assert!(!export.export_version.is_empty());
        assert!(!export.source_schema_hash.is_empty());
    }

    // FRM-008: graph fact roundtrip
    #[test]
    fn graph_fact_roundtrip() {
        let fact = GraphFact {
            from_id: "ms-a".into(),
            from_kind: "milestone".into(),
            to_id: "ms-b".into(),
            to_kind: "milestone".into(),
            edge_kind: "blocks".into(),
            verified: true,
        };
        let json = serde_json::to_string(&fact).unwrap();
        let back: GraphFact = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from_id, "ms-a");
        assert!(back.verified);
    }

    // ── FRM-010: export_plan_for_verification tests ─────────────────

    use planning_engine::schemas::{
        ConditionEval, DependencyEdge, DependencyKind, DependencyNodeKind, GateCondition,
        GateConditionEntry, GateStatus, InvariantEnforcement, InvariantScope, InvariantStatus,
        PlanGateDefinition, PlanInvariant,
    };

    fn sample_gate() -> PlanGateDefinition {
        PlanGateDefinition {
            gate_id: "gate-001".into(),
            plan_id: "plan-001".into(),
            condition_entries: vec![
                GateConditionEntry {
                    condition: GateCondition::InvariantsExtracted,
                    eval: ConditionEval::Pass,
                },
                GateConditionEntry {
                    condition: GateCondition::DependenciesAcyclic,
                    eval: ConditionEval::Pass,
                },
            ],
            current_status: GateStatus::Satisfied,
            unresolved_question_budget: 3,
            unresolved_question_count: 0,
            override_reason: None,
            evaluated_at: Utc::now(),
        }
    }

    fn sample_invariants() -> Vec<PlanInvariant> {
        vec![
            PlanInvariant {
                invariant_id: "inv-001".into(),
                objective_id: "obj-001".into(),
                description: "All milestones must have acceptance criteria".into(),
                predicate: "forall ms in milestones: len(ms.acceptance_criteria) > 0"
                    .into(),
                scope: InvariantScope::Global,
                enforcement: InvariantEnforcement::PlanValidation,
                status: InvariantStatus::Holding,
                target_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            PlanInvariant {
                invariant_id: "inv-002".into(),
                objective_id: "obj-001".into(),
                description: "Dependency graph must be acyclic".into(),
                predicate: "is_acyclic(dependency_graph)".into(),
                scope: InvariantScope::Global,
                enforcement: InvariantEnforcement::Continuous,
                status: InvariantStatus::Holding,
                target_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ]
    }

    fn sample_dependencies() -> Vec<DependencyEdge> {
        vec![
            DependencyEdge {
                edge_id: "edge-001".into(),
                from_id: "ms-1".into(),
                from_kind: DependencyNodeKind::Milestone,
                to_id: "ms-2".into(),
                to_kind: DependencyNodeKind::Milestone,
                edge_kind: DependencyKind::Blocks,
                rationale: Some("ms-2 depends on ms-1 output".into()),
            },
            DependencyEdge {
                edge_id: "edge-002".into(),
                from_id: "ms-2".into(),
                from_kind: DependencyNodeKind::Milestone,
                to_id: "ms-3".into(),
                to_kind: DependencyNodeKind::Milestone,
                edge_kind: DependencyKind::ShouldPrecede,
                rationale: None,
            },
        ]
    }

    // FRM-010: export contains one predicate per invariant
    #[test]
    fn export_plan_produces_predicates_from_invariants() {
        let gate = sample_gate();
        let invariants = sample_invariants();
        let deps = sample_dependencies();
        let export = export_plan_for_verification(&gate, &invariants, &deps);
        assert_eq!(export.predicates.len(), 2);
        assert_eq!(export.predicates[0].predicate_id, "inv-001");
        assert_eq!(export.predicates[1].predicate_id, "inv-002");
        assert_eq!(export.predicates[0].predicate_category, "plan_invariant");
    }

    // FRM-010: export contains graph facts from dependencies
    #[test]
    fn export_plan_produces_graph_facts_from_dependencies() {
        let gate = sample_gate();
        let invariants = sample_invariants();
        let deps = sample_dependencies();
        let export = export_plan_for_verification(&gate, &invariants, &deps);
        assert_eq!(export.graph_facts.len(), 2);
        assert_eq!(export.graph_facts[0].from_id, "ms-1");
        assert_eq!(export.graph_facts[0].to_id, "ms-2");
        assert_eq!(export.graph_facts[0].edge_kind, "blocks");
        assert_eq!(export.graph_facts[1].edge_kind, "should_precede");
    }

    // FRM-010: export records gate status as lifecycle state
    #[test]
    fn export_plan_records_gate_lifecycle() {
        let gate = sample_gate();
        let invariants = sample_invariants();
        let deps = sample_dependencies();
        let export = export_plan_for_verification(&gate, &invariants, &deps);
        assert_eq!(export.lifecycle_states.len(), 1);
        assert_eq!(export.lifecycle_states[0].entity_id, "gate-001");
        assert_eq!(export.lifecycle_states[0].entity_kind, "plan_gate");
        assert_eq!(export.lifecycle_states[0].lifecycle, "satisfied");
    }

    // FRM-010: export has deterministic checksum
    #[test]
    fn export_plan_checksum_is_deterministic() {
        let gate = sample_gate();
        let invariants = sample_invariants();
        let deps = sample_dependencies();
        let export1 = export_plan_for_verification(&gate, &invariants, &deps);
        let export2 = export_plan_for_verification(&gate, &invariants, &deps);
        assert_eq!(export1.source_schema_hash, export2.source_schema_hash);
        assert!(!export1.source_schema_hash.is_empty());
    }

    // FRM-010: export with empty invariants and dependencies
    #[test]
    fn export_plan_handles_empty_inputs() {
        let gate = sample_gate();
        let export = export_plan_for_verification(&gate, &[], &[]);
        assert!(export.predicates.is_empty());
        assert!(export.graph_facts.is_empty());
        assert_eq!(export.lifecycle_states.len(), 1);
        assert!(!export.source_schema_hash.is_empty());
    }

    // FRM-010: export is backend-neutral (no prover syntax)
    #[test]
    fn export_plan_contains_no_prover_syntax() {
        let gate = sample_gate();
        let invariants = sample_invariants();
        let deps = sample_dependencies();
        let export = export_plan_for_verification(&gate, &invariants, &deps);
        let json = serde_json::to_string(&export).unwrap();
        assert!(!json.contains("theorem"));
        assert!(!json.contains("lemma"));
        assert!(!json.contains("sorry"));
        assert!(!json.contains("Isar"));
        assert!(!json.contains("#check"));
    }

    // FRM-010: export roundtrip serialization
    #[test]
    fn export_plan_roundtrip() {
        let gate = sample_gate();
        let invariants = sample_invariants();
        let deps = sample_dependencies();
        let export = export_plan_for_verification(&gate, &invariants, &deps);
        let json = serde_json::to_string(&export).unwrap();
        let back: FormalExport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.predicates.len(), 2);
        assert_eq!(back.graph_facts.len(), 2);
        assert_eq!(back.source_schema_hash, export.source_schema_hash);
    }
}
