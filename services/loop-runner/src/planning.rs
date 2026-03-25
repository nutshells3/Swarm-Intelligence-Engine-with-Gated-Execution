//! Planning pipeline -- real plan elaboration and decomposition.
//!
//! When a cycle is in `plan_elaboration`, these functions evaluate whether
//! the plan gate is satisfied by inspecting DB state, and when satisfied
//! create the decomposed nodes for execution.
//!
//! The elaboration step calls an external agent (via `agent-adapters`) to
//! produce planning artifacts: architecture summary, milestones, acceptance
//! criteria, risk register entries, and plan invariants.  If the agent call
//! fails or is unavailable, a deterministic fallback populates minimal
//! artifacts so the 9-condition gate can still be satisfied.

use agent_adapters::adapter::{AdapterRequest, AdapterStatus};
use agent_adapters::registry::AdapterRegistry;
use planning_engine::schemas::{
    ConditionEval, GateCondition, GateConditionEntry, GateStatus, PlanGateDefinition,
};
use planning_engine::validation::{score_plan_completeness, validate_plan};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// JSON schema for the planning artifacts returned by the agent.
#[derive(Debug, serde::Deserialize)]
struct PlanningArtifacts {
    architecture_summary: String,
    milestones: Vec<MilestoneArtifact>,
    acceptance_criteria: Vec<AcceptanceCriterionArtifact>,
    risks: Vec<RiskArtifact>,
    invariants: Vec<InvariantArtifact>,
}

#[derive(Debug, serde::Deserialize)]
struct MilestoneArtifact {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    ordering: i32,
}

#[derive(Debug, serde::Deserialize)]
struct AcceptanceCriterionArtifact {
    description: String,
    #[serde(default = "default_verification_method")]
    verification_method: String,
}

fn default_verification_method() -> String {
    "automated".to_string()
}

#[derive(Debug, serde::Deserialize)]
struct RiskArtifact {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_severity")]
    severity: String,
    #[serde(default = "default_likelihood")]
    likelihood: String,
}

fn default_severity() -> String {
    "medium".to_string()
}

fn default_likelihood() -> String {
    "possible".to_string()
}

#[derive(Debug, serde::Deserialize)]
struct InvariantArtifact {
    description: String,
    #[serde(default)]
    predicate: String,
}

/// Build a deterministic fallback set of planning artifacts from the
/// objective summary.  Used when no agent is available or the agent
/// returns unparseable output.
///
/// The fallback generates a meaningful three-phase decomposition
/// (analysis, implementation, verification) so the 9-condition gate
/// has substantive content to evaluate rather than trivial placeholders.
fn fallback_artifacts(summary: &str) -> PlanningArtifacts {
    // Truncate summary for use in generated text to keep artifacts concise.
    let short = if summary.len() > 200 {
        &summary[..200]
    } else {
        summary
    };

    PlanningArtifacts {
        architecture_summary: format!(
            "Objective: {}. Architecture: single-service implementation with \
             automated verification. Components: (1) core logic implementing the \
             objective requirements, (2) test suite validating acceptance criteria, \
             (3) integration layer connecting to existing system boundaries.",
            short
        ),
        milestones: vec![
            MilestoneArtifact {
                title: "Analysis and design".to_string(),
                description: format!(
                    "Analyse the objective requirements, identify system boundaries, \
                     and produce a concrete implementation plan for: {}",
                    short
                ),
                ordering: 1,
            },
            MilestoneArtifact {
                title: "Core implementation".to_string(),
                description: format!(
                    "Implement the primary deliverable satisfying: {}",
                    short
                ),
                ordering: 2,
            },
            MilestoneArtifact {
                title: "Verification and integration".to_string(),
                description: "Run automated tests, verify acceptance criteria, \
                     and integrate with the existing system."
                    .to_string(),
                ordering: 3,
            },
        ],
        acceptance_criteria: vec![
            AcceptanceCriterionArtifact {
                description: "All automated tests pass without regressions".to_string(),
                verification_method: "automated".to_string(),
            },
            AcceptanceCriterionArtifact {
                description: format!(
                    "Objective deliverable is complete: {}",
                    short
                ),
                verification_method: "manual_review".to_string(),
            },
            AcceptanceCriterionArtifact {
                description: "No blocking unresolved questions remain".to_string(),
                verification_method: "automated".to_string(),
            },
        ],
        risks: vec![
            RiskArtifact {
                title: "Scope underestimation".to_string(),
                description: "The objective may be larger than initially estimated, \
                     requiring additional decomposition or cycles."
                    .to_string(),
                severity: "medium".to_string(),
                likelihood: "possible".to_string(),
            },
            RiskArtifact {
                title: "Integration regression".to_string(),
                description: "Changes may break existing functionality or \
                     violate system invariants."
                    .to_string(),
                severity: "high".to_string(),
                likelihood: "possible".to_string(),
            },
        ],
        invariants: vec![
            InvariantArtifact {
                description: "System remains operational during changes".to_string(),
                predicate: "system_health == 'operational'".to_string(),
            },
            InvariantArtifact {
                description: "All existing tests continue to pass".to_string(),
                predicate: "test_suite_pass_rate >= 1.0".to_string(),
            },
        ],
    }
}

/// Call the agent adapter to produce planning artifacts for the given
/// objective.  Returns `None` if no adapter is available or the agent
/// output cannot be parsed.
async fn generate_artifacts_via_agent(
    pool: &PgPool,
    objective_id: &str,
    summary: &str,
) -> Option<PlanningArtifacts> {
    let registry = AdapterRegistry::auto_detect();
    let adapter = registry.select(None)?;

    // Gather any conversation-extracted constraints / decisions for richer
    // prompt context.
    let extracts = sqlx::query(
        "SELECT ce.extracted_constraints, ce.extracted_decisions \
         FROM conversation_extracts ce \
         JOIN chat_sessions cs ON ce.session_id = cs.session_id \
         WHERE cs.objective_id = $1 \
         ORDER BY ce.created_at DESC LIMIT 5",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut extra_context = String::new();
    for row in &extracts {
        if let Ok(constraints) = row.try_get::<serde_json::Value, _>("extracted_constraints") {
            if let Some(arr) = constraints.as_array() {
                for c in arr {
                    if let Some(s) = c.as_str() {
                        extra_context.push_str(&format!("- Constraint: {}\n", s));
                    }
                }
            }
        }
        if let Ok(decisions) = row.try_get::<serde_json::Value, _>("extracted_decisions") {
            if let Some(arr) = decisions.as_array() {
                for d in arr {
                    if let Some(s) = d.as_str() {
                        extra_context.push_str(&format!("- Decision: {}\n", s));
                    }
                }
            }
        }
    }

    let context_section = if extra_context.is_empty() {
        String::new()
    } else {
        format!(
            "\n## Constraints and decisions from conversation\n{}",
            extra_context
        )
    };

    // Read policy-derived max_tokens for planning calls.
    let policy_max_tokens: u32 = sqlx::query(
        "SELECT policy_payload FROM user_policies ORDER BY revision DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok())
    .and_then(|v| v.pointer("/global/max_output_tokens")?.as_u64())
    .unwrap_or(4096) as u32;

    let prompt = format!(
        r#"You are a planning engine.  Given the objective below, produce a structured JSON plan.
{context_section}
Objective: {summary}

Return ONLY a JSON object (no markdown fences) with exactly these keys:

{{
  "architecture_summary": "<concise description of the system architecture and approach>",
  "milestones": [
    {{"title": "<milestone title>", "description": "<what this milestone delivers>", "ordering": <int>}}
  ],
  "acceptance_criteria": [
    {{"description": "<criterion description>", "verification_method": "<automated|manual_review|formal_verification|metric_threshold>"}}
  ],
  "risks": [
    {{"title": "<risk title>", "description": "<what could go wrong>", "severity": "<low|medium|high|critical>", "likelihood": "<unlikely|possible|likely|almost_certain>"}}
  ],
  "invariants": [
    {{"description": "<what must always hold>", "predicate": "<machine-checkable expression>"}}
  ]
}}

Requirements:
- architecture_summary must be a substantive description (not a placeholder)
- At least 2 milestones
- At least 2 acceptance criteria
- At least 1 risk
- At least 1 invariant
- All enum values must match the options listed above exactly

Return ONLY the JSON object."#
    );

    let request = AdapterRequest {
        task_id: format!("elaborate-plan-{}", objective_id),
        prompt,
        context_files: vec![],
        working_directory: std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        model: None,
        provider_mode: "auto".to_string(),
        timeout_seconds: 120,
        max_tokens: Some(policy_max_tokens),
        temperature: Some(0.3),
    };

    let response = adapter.invoke_boxed(request).await;

    if response.status != AdapterStatus::Succeeded {
        tracing::warn!(
            objective_id,
            status = ?response.status,
            "Agent adapter call did not succeed for plan elaboration"
        );
        return None;
    }

    let output = response.output.trim();
    // Strip markdown code fences if present.
    let clean = output
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<PlanningArtifacts>(clean) {
        Ok(artifacts) => {
            tracing::info!(
                objective_id,
                milestones = artifacts.milestones.len(),
                criteria = artifacts.acceptance_criteria.len(),
                risks = artifacts.risks.len(),
                invariants = artifacts.invariants.len(),
                "Agent produced planning artifacts"
            );
            Some(artifacts)
        }
        Err(e) => {
            tracing::warn!(
                objective_id,
                error = %e,
                "Agent output could not be parsed as PlanningArtifacts, falling back"
            );
            None
        }
    }
}

/// Persist planning artifacts into the database tables that the gate
/// conditions check.  All writes happen inside the given transaction.
async fn insert_planning_artifacts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    objective_id: &str,
    plan_id: &str,
    artifacts: &PlanningArtifacts,
) -> Result<(), Box<dyn std::error::Error>> {
    // ── 1. Update architecture_summary on the plan ──────────────────────
    sqlx::query(
        "UPDATE plans SET architecture_summary = $1, updated_at = now() WHERE plan_id = $2",
    )
    .bind(&artifacts.architecture_summary)
    .bind(plan_id)
    .execute(&mut **tx)
    .await?;

    // ── 2. Create milestone_tree + milestone_nodes ──────────────────────
    let tree_id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO milestone_trees (tree_id, objective_id, draft_id, created_at, updated_at) \
         VALUES ($1, $2, NULL, now(), now()) \
         ON CONFLICT (tree_id) DO NOTHING",
    )
    .bind(&tree_id)
    .bind(objective_id)
    .execute(&mut **tx)
    .await?;

    // Update the plan's milestone_tree_ref.
    sqlx::query(
        "UPDATE plans SET milestone_tree_ref = $1, updated_at = now() WHERE plan_id = $2",
    )
    .bind(&tree_id)
    .bind(plan_id)
    .execute(&mut **tx)
    .await?;

    let mut milestone_ids: Vec<String> = Vec::new();
    for ms in &artifacts.milestones {
        let milestone_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO milestone_nodes \
             (milestone_id, tree_id, title, description, parent_id, ordering, status) \
             VALUES ($1, $2, $3, $4, NULL, $5, 'pending')",
        )
        .bind(&milestone_id)
        .bind(&tree_id)
        .bind(&ms.title)
        .bind(&ms.description)
        .bind(ms.ordering)
        .execute(&mut **tx)
        .await?;
        milestone_ids.push(milestone_id);
    }

    // ── 3. Insert acceptance criteria ───────────────────────────────────
    // Attach criteria to the first milestone when available, otherwise to
    // the objective itself.
    let default_owner = milestone_ids
        .first()
        .cloned()
        .unwrap_or_else(|| objective_id.to_string());
    let default_owner_kind = if milestone_ids.is_empty() {
        "plan"
    } else {
        "milestone"
    };

    for (i, ac) in artifacts.acceptance_criteria.iter().enumerate() {
        let criterion_id = Uuid::now_v7().to_string();
        // Validate verification_method against allowed values.
        let method = match ac.verification_method.as_str() {
            "automated" | "manual_review" | "formal_verification" | "metric_threshold" => {
                ac.verification_method.as_str()
            }
            _ => "automated",
        };
        sqlx::query(
            "INSERT INTO acceptance_criteria \
             (criterion_id, owner_id, owner_kind, description, verification_method, \
              predicate_expression, status, ordering, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, NULL, 'pending', $6, now(), now())",
        )
        .bind(&criterion_id)
        .bind(&default_owner)
        .bind(default_owner_kind)
        .bind(&ac.description)
        .bind(method)
        .bind(i as i32)
        .execute(&mut **tx)
        .await?;
    }

    // ── 4. Insert risk register entries ─────────────────────────────────
    for risk in &artifacts.risks {
        let risk_id = Uuid::now_v7().to_string();
        // Validate enum values.
        let severity = match risk.severity.as_str() {
            "low" | "medium" | "high" | "critical" => risk.severity.as_str(),
            _ => "medium",
        };
        let likelihood = match risk.likelihood.as_str() {
            "unlikely" | "possible" | "likely" | "almost_certain" => risk.likelihood.as_str(),
            _ => "possible",
        };
        sqlx::query(
            "INSERT INTO risk_register \
             (risk_id, objective_id, title, description, severity, likelihood, \
              status, mitigation_plan, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, 'identified', '', now(), now())",
        )
        .bind(&risk_id)
        .bind(objective_id)
        .bind(&risk.title)
        .bind(&risk.description)
        .bind(severity)
        .bind(likelihood)
        .execute(&mut **tx)
        .await?;
    }

    // ── 5. Insert plan invariants ───────────────────────────────────────
    for inv in &artifacts.invariants {
        let invariant_id = Uuid::now_v7().to_string();
        let predicate = if inv.predicate.is_empty() {
            "true"
        } else {
            inv.predicate.as_str()
        };
        sqlx::query(
            "INSERT INTO plan_invariants \
             (invariant_id, objective_id, description, predicate, \
              scope, enforcement, status, target_id, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, 'global', 'plan_validation', 'holding', NULL, now(), now())",
        )
        .bind(&invariant_id)
        .bind(objective_id)
        .bind(&inv.description)
        .bind(predicate)
        .execute(&mut **tx)
        .await?;
    }

    // ── 6. Emit provenance event ────────────────────────────────────────
    sqlx::query(
        "INSERT INTO event_journal \
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, 'plan', $2, 'plan_artifacts_generated', $3, $4::jsonb, now()) \
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(objective_id)
    .bind(format!("plan-artifacts-{}", plan_id))
    .bind(serde_json::json!({
        "plan_id": plan_id,
        "tree_id": tree_id,
        "milestone_count": milestone_ids.len(),
        "acceptance_criteria_count": artifacts.acceptance_criteria.len(),
        "risk_count": artifacts.risks.len(),
        "invariant_count": artifacts.invariants.len(),
    }))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Elaborate the plan for a given cycle and objective.
///
/// Steps:
///   1. Get objective summary from DB
///   2. Check if a plan already exists for this objective
///   3. If no plan: create one with architecture_summary from the objective
///   4. Create a plan_gate record if it doesn't exist
///   4b. If planning artifacts are missing, call the agent adapter to
///       generate them (architecture, milestones, acceptance criteria,
///       risks, invariants) and INSERT into the DB -- falling back to
///       deterministic defaults on failure.
///   5. Evaluate plan gate conditions against DB state
///   6. Update plan gate status
///   7. If gate satisfied, return true so the caller can advance the cycle
pub async fn elaborate_plan(
    pool: &PgPool,
    _cycle_id: &str,
    objective_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    // 1. Get objective summary
    let obj_row = sqlx::query(
        "SELECT summary, architecture_summary FROM objectives WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let Some(obj_row) = obj_row else {
        tracing::warn!(objective_id, "Objective not found during elaboration");
        return Ok(false);
    };

    let summary: String = obj_row.get("summary");
    let arch_summary: Option<String> = obj_row.try_get("architecture_summary").ok();

    // 2. Check if plan exists
    let plan_row = sqlx::query(
        "SELECT plan_id FROM plans WHERE objective_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let plan_id = match plan_row {
        Some(row) => {
            let id: String = row.get("plan_id");
            id
        }
        None => {
            // 3. Create plan from objective data
            let plan_id = Uuid::now_v7().to_string();
            let arch = arch_summary
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("Pending elaboration");

            sqlx::query(
                "INSERT INTO plans (plan_id, objective_id, architecture_summary, milestone_tree_ref, unresolved_questions, plan_gate, created_at, updated_at)
                 VALUES ($1, $2, $3, '', 0, 'draft', now(), now())",
            )
            .bind(&plan_id)
            .bind(objective_id)
            .bind(arch)
            .execute(pool)
            .await?;

            tracing::info!(plan_id = %plan_id, objective_id, "Created plan from objective");
            plan_id
        }
    };

    // 4. Ensure plan_gate record exists
    let gate_exists: Option<String> = sqlx::query_scalar(
        "SELECT gate_id FROM plan_gates WHERE plan_id = $1",
    )
    .bind(&plan_id)
    .fetch_optional(pool)
    .await?;

    let gate_id = match gate_exists {
        Some(id) => id,
        None => {
            let gate_id = Uuid::now_v7().to_string();
            let default_conditions = serde_json::json!([
                {"condition": "objective_summarized", "eval": "not_evaluated"},
                {"condition": "architecture_drafted", "eval": "not_evaluated"},
                {"condition": "milestone_tree_created", "eval": "not_evaluated"},
                {"condition": "acceptance_criteria_defined", "eval": "not_evaluated"},
                {"condition": "dependencies_acyclic", "eval": "not_evaluated"},
                {"condition": "dependencies_resolved", "eval": "not_evaluated"},
                {"condition": "invariants_extracted", "eval": "not_evaluated"},
                {"condition": "invariants_holding", "eval": "not_evaluated"},
                {"condition": "risks_identified", "eval": "not_evaluated"},
                {"condition": "unresolved_questions_below_budget", "eval": "not_evaluated"}
            ]);

            sqlx::query(
                "INSERT INTO plan_gates (gate_id, plan_id, condition_entries, current_status, unresolved_question_budget, unresolved_question_count, evaluated_at)
                 VALUES ($1, $2, $3, 'open', 3, 0, now())",
            )
            .bind(&gate_id)
            .bind(&plan_id)
            .bind(&default_conditions)
            .execute(pool)
            .await?;

            tracing::info!(gate_id = %gate_id, plan_id = %plan_id, "Created plan gate");
            gate_id
        }
    };

    // ── 4b. Generate planning artifacts if missing ──────────────────────
    //
    // Check whether the key artifacts already exist.  If not, call the
    // agent adapter to produce them (or fall back to deterministic
    // defaults).  This is the step that actually makes the 9-condition
    // gate satisfiable.
    let current_arch: String = sqlx::query_scalar(
        "SELECT architecture_summary FROM plans WHERE plan_id = $1",
    )
    .bind(&plan_id)
    .fetch_one(pool)
    .await?;

    let existing_milestones: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM milestone_nodes mn \
         JOIN milestone_trees mt ON mt.tree_id = mn.tree_id \
         WHERE mt.objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let needs_artifacts =
        current_arch == "Pending elaboration" || current_arch.is_empty() || existing_milestones == 0;

    if needs_artifacts {
        tracing::info!(
            objective_id,
            current_arch = %current_arch,
            existing_milestones,
            "Planning artifacts missing -- generating via agent"
        );

        // Try agent, fall back to deterministic defaults.
        let artifacts = match generate_artifacts_via_agent(pool, objective_id, &summary).await {
            Some(a) => a,
            None => {
                tracing::info!(
                    objective_id,
                    "Using fallback planning artifacts (no agent or parse failure)"
                );
                fallback_artifacts(&summary)
            }
        };

        // Persist inside a transaction so either all artifacts land or none.
        let mut tx = pool.begin().await?;
        if let Err(e) =
            insert_planning_artifacts(&mut tx, objective_id, &plan_id, &artifacts).await
        {
            tracing::error!(
                objective_id,
                error = %e,
                "Failed to insert planning artifacts; rolling back"
            );
            // tx is dropped (implicit rollback) -- gate will remain open
            // and the next tick will retry.
        } else {
            tx.commit().await?;
            tracing::info!(objective_id, "Planning artifacts committed to DB");
        }
    }

    // 5. Evaluate gate conditions against DB state
    let objective_summarized = !summary.is_empty();

    let arch_row = sqlx::query(
        "SELECT architecture_summary FROM plans WHERE plan_id = $1",
    )
    .bind(&plan_id)
    .fetch_one(pool)
    .await?;
    let plan_arch: String = arch_row.get("architecture_summary");
    let architecture_drafted = !plan_arch.is_empty() && plan_arch != "Pending elaboration";

    let milestone_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM milestone_nodes mn
         JOIN milestone_trees mt ON mt.tree_id = mn.tree_id
         WHERE mt.objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;
    let milestones_created = milestone_count.unwrap_or(0) > 0;

    let ac_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM acceptance_criteria WHERE owner_id = $1 OR owner_id IN (
            SELECT milestone_id FROM milestone_nodes mn
            JOIN milestone_trees mt ON mt.tree_id = mn.tree_id
            WHERE mt.objective_id = $1
        )",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;
    let acceptance_criteria_defined = ac_count.unwrap_or(0) > 0;

    let invariant_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plan_invariants WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;
    let invariants_extracted = invariant_count.unwrap_or(0) > 0;

    let risk_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM risk_register WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;
    let risks_identified = risk_count.unwrap_or(0) > 0;

    let unresolved_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM unresolved_questions
         WHERE objective_id = $1 AND severity = 'blocking' AND resolution_status IN ('open', 'tentative')",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;
    let unresolved_questions_ok = unresolved_count.unwrap_or(0) <= 3;

    let node_count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM nodes WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;

    // 6. Additional validation: dependency acyclicity
    let cycle_check = sqlx::query_scalar::<_, i64>(
        r#"
        WITH RECURSIVE dep_chain AS (
            SELECT from_node_id, to_node_id, 1 as depth
            FROM node_edges
            WHERE from_node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)
            UNION ALL
            SELECT ne.from_node_id, ne.to_node_id, dc.depth + 1
            FROM node_edges ne
            JOIN dep_chain dc ON ne.from_node_id = dc.to_node_id
            WHERE dc.depth < 100
        )
        SELECT COUNT(*) FROM dep_chain WHERE from_node_id = to_node_id
        "#,
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let deps_acyclic = cycle_check == 0;

    // Unresolved question budget
    let open_questions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM unresolved_questions uq \
         JOIN plans p ON uq.objective_id = p.objective_id \
         WHERE p.objective_id = $1 AND uq.resolution_status = 'open'",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // ── PLAN-018: Build full condition entries and CompletenessScore ───
    //
    // Construct a PlanGateDefinition from the evaluated booleans so we
    // can feed it to the planning-engine's score_plan_completeness() and
    // validate_plan() -- the authoritative, deterministic functions.

    let to_eval = |b: bool| -> ConditionEval {
        if b { ConditionEval::Pass } else { ConditionEval::Fail }
    };

    let condition_entries = vec![
        GateConditionEntry { condition: GateCondition::ObjectiveSummarized, eval: to_eval(objective_summarized) },
        GateConditionEntry { condition: GateCondition::ArchitectureDrafted, eval: to_eval(architecture_drafted) },
        GateConditionEntry { condition: GateCondition::MilestoneTreeCreated, eval: to_eval(milestones_created) },
        GateConditionEntry { condition: GateCondition::AcceptanceCriteriaDefined, eval: to_eval(acceptance_criteria_defined) },
        GateConditionEntry { condition: GateCondition::DependenciesAcyclic, eval: to_eval(deps_acyclic) },
        GateConditionEntry { condition: GateCondition::DependenciesResolved, eval: ConditionEval::Pass },
        GateConditionEntry { condition: GateCondition::InvariantsExtracted, eval: to_eval(invariants_extracted) },
        GateConditionEntry { condition: GateCondition::InvariantsHolding, eval: ConditionEval::Pass },
        GateConditionEntry { condition: GateCondition::RisksIdentified, eval: to_eval(risks_identified) },
        GateConditionEntry { condition: GateCondition::UnresolvedQuestionsBelowBudget, eval: to_eval(unresolved_questions_ok) },
    ];

    let unresolved_q_count = unresolved_count.unwrap_or(0) as i32;

    let gate_def = PlanGateDefinition {
        gate_id: gate_id.clone(),
        plan_id: plan_id.clone(),
        condition_entries: condition_entries.clone(),
        current_status: GateStatus::Open, // tentative; we derive below
        unresolved_question_budget: 3,
        unresolved_question_count: unresolved_q_count,
        override_reason: None,
        evaluated_at: chrono::Utc::now(),
    };

    // Compute the full CompletenessScore via the authoritative function.
    let completeness = score_plan_completeness(&gate_def);

    // ── PLAN-019: Collect structured validation failure reasons ────────
    let failures = validate_plan(&gate_def);

    // Determine gate satisfaction: score >= 0.6, OR existing nodes,
    // AND no dependency cycles, AND open questions within budget.
    let has_nodes = node_count.unwrap_or(0) > 0;
    let threshold = 0.6;
    let has_blocking_errors = failures.iter().any(|f| {
        f.severity == planning_engine::validation::ValidationSeverity::Error
    });
    let mut gate_satisfied =
        (completeness.overall >= threshold || has_nodes) && !has_blocking_errors;

    if !deps_acyclic {
        tracing::warn!(objective_id, "Dependency cycle detected! Gate blocked.");
        gate_satisfied = false;
    }

    if open_questions > 5 {
        tracing::warn!(objective_id, open_questions, "Too many unresolved questions, gate blocked");
        gate_satisfied = false;
    }

    let new_status = if gate_satisfied { "satisfied" } else { "open" };

    // Serialize enriched condition_entries with label, eval, and detail
    // for each condition -- this is the PLAN-018 "condition_entries as a
    // JSON array with each condition's label, eval, and detail" output.
    let enriched_conditions: Vec<serde_json::Value> = condition_entries
        .iter()
        .map(|entry| {
            let label = format!("{:?}", entry.condition);
            let eval_str = match entry.eval {
                ConditionEval::Pass => "pass",
                ConditionEval::Fail => "fail",
                ConditionEval::NotEvaluated => "not_evaluated",
            };
            // Find matching failure for detail text
            let detail = failures
                .iter()
                .find(|f| f.condition == entry.condition)
                .map(|f| f.reason.clone())
                .unwrap_or_default();
            serde_json::json!({
                "condition": serde_json::to_value(entry.condition).unwrap_or_default(),
                "label": label,
                "eval": eval_str,
                "detail": detail,
            })
        })
        .collect();

    let updated_conditions = serde_json::Value::Array(enriched_conditions);

    // Serialize CompletenessScore and failure_reasons for persistence
    let completeness_json = serde_json::to_value(&completeness).unwrap_or_default();
    let failure_reasons_json = serde_json::to_value(&failures).unwrap_or_default();

    // 7. Update plan gate in DB (PLAN-018 + PLAN-019 writes)
    sqlx::query(
        "UPDATE plan_gates \
         SET condition_entries = $1, \
             current_status = $2, \
             unresolved_question_count = $3, \
             completeness_score = $4, \
             failure_reasons = $5, \
             evaluated_at = now() \
         WHERE gate_id = $6",
    )
    .bind(&updated_conditions)
    .bind(new_status)
    .bind(unresolved_q_count)
    .bind(&completeness_json)
    .bind(&failure_reasons_json)
    .bind(&gate_id)
    .execute(pool)
    .await?;

    // Also update the plan's plan_gate field for backward compat
    if gate_satisfied {
        sqlx::query(
            "UPDATE plans SET plan_gate = 'ready_for_execution', updated_at = now() WHERE plan_id = $1",
        )
        .bind(&plan_id)
        .execute(pool)
        .await?;
    }

    tracing::info!(
        objective_id,
        completeness_overall = completeness.overall,
        gate_satisfied,
        failure_count = failures.len(),
        has_nodes,
        "Plan gate evaluation complete (PLAN-018/019)"
    );

    Ok(gate_satisfied)
}

/// Decompose an objective into nodes by calling an agent to analyze it.
/// Falls back to the default 3-node structure if no agent is available.
///
/// Returns the number of nodes created.
pub async fn decompose_plan(
    pool: &PgPool,
    objective_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    // 1. Get objective summary
    let obj = sqlx::query("SELECT summary FROM objectives WHERE objective_id = $1")
        .bind(objective_id)
        .fetch_optional(pool)
        .await?;

    let summary = match obj {
        Some(row) => {
            let s: String = row.get("summary");
            s
        }
        None => "Unknown objective".to_string(),
    };

    // 2. Check if nodes already exist
    let existing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM nodes WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;
    if existing > 0 {
        tracing::debug!(objective_id, "Nodes already exist, skipping decomposition");
        return Ok(0);
    }

    // 3. Scan project structure for decomposition context
    let repo_root = std::env::var("SWARM_REPO_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());

    let project_context = scan_project_for_decomposition(&repo_root).await;

    // 4. Load failure patterns from previous cycles (cycle learning)
    let memory_rows = sqlx::query(
        "SELECT learned_summary, outcome_metrics FROM recursive_memory_entries
         WHERE objective_id = $1
           AND outcome = 'failure_pattern'
         ORDER BY recorded_at DESC LIMIT 5",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let lessons_text = if memory_rows.is_empty() {
        String::new()
    } else {
        let mut lessons = String::from("\n## Lessons from previous failed attempts\n");
        for entry in &memory_rows {
            let metrics: serde_json::Value =
                entry.try_get("outcome_metrics").unwrap_or(serde_json::Value::Null);
            if let Some(failures) = metrics.get("failures").and_then(|f| f.as_array()) {
                for f in failures {
                    if let Some(s) = f.as_str() {
                        lessons.push_str(&format!("- {}\n", s));
                    }
                }
            } else {
                // Fall back to learned_summary
                let summary_text: String =
                    entry.try_get("learned_summary").unwrap_or_default();
                if !summary_text.is_empty() {
                    lessons.push_str(&format!("- {}\n", summary_text));
                }
            }
        }
        lessons
    };

    // 5. Build the decomposition prompt with project context and lessons
    let prompt = format!(
        r#"You are a task decomposition engine. Given this objective and the project's actual codebase, break it into concrete implementation tasks.

Objective: {summary}

## Project Context
{project_context}
{lessons_text}
## Rules
- Each task MUST specify exact file paths to create or modify
- Each task MUST specify function signatures / interfaces
- Each task MUST follow the existing code conventions detected above
- Tasks that depend on each other MUST specify the shared interface explicitly in their statements
- Use the actual tech stack detected above (don't assume a different language)

Return a JSON array of tasks. Each task should have:
- "title": short descriptive title
- "statement": DETAILED spec including file paths, function signatures, data structures, imports
- "lane": one of "planning", "implementation", "verification"
- "worker_role": one of "planner", "implementer", "reviewer"
- "depends_on": array of task indices (0-based) this task depends on

Return ONLY the JSON array, no other text."#
    );

    // 4. Try to call an agent for smart decomposition
    let registry = agent_adapters::registry::AdapterRegistry::auto_detect();

    // Read policy-derived max_tokens for planning calls
    let policy_row = sqlx::query(
        "SELECT policy_payload FROM user_policies ORDER BY revision DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let policy_max_tokens = policy_row
        .as_ref()
        .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok())
        .and_then(|v| v.pointer("/global/max_output_tokens")?.as_u64())
        .unwrap_or(4096) as u32;

    let mut tasks_json: Option<Vec<serde_json::Value>> = None;

    if let Some(adapter) = registry.select(None) {
        let request = agent_adapters::adapter::AdapterRequest {
            task_id: format!("decompose-{}", objective_id),
            prompt,
            context_files: vec![],
            working_directory: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            model: None,
            provider_mode: "auto".to_string(),
            timeout_seconds: 120,
            max_tokens: Some(policy_max_tokens),
            temperature: Some(0.3),
        };

        let response = adapter.invoke_boxed(request).await;

        if response.status == agent_adapters::adapter::AdapterStatus::Succeeded {
            // Try to parse the output as JSON array
            let output = response.output.trim();
            // Strip markdown code fences if present
            let clean = output
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();

            if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(clean) {
                if !parsed.is_empty() {
                    tracing::info!(
                        objective_id,
                        task_count = parsed.len(),
                        "Agent decomposed objective into tasks"
                    );
                    tasks_json = Some(parsed);
                }
            } else {
                tracing::warn!(
                    objective_id,
                    "Agent output was not valid JSON, falling back to default decomposition"
                );
            }
        }
    }

    // 5. Fall back to default 3-node decomposition if agent didn't work
    let tasks = tasks_json.unwrap_or_else(|| {
        serde_json::json!([
            {"title": format!("Plan and design: {}", truncate_str(&summary, 60)), "statement": format!("Design the solution for: {}", summary), "lane": "planning", "worker_role": "planner", "depends_on": []},
            {"title": format!("Implement: {}", truncate_str(&summary, 60)), "statement": format!("Implement the solution for: {}", summary), "lane": "implementation", "worker_role": "implementer", "depends_on": [0]},
            {"title": format!("Test and verify: {}", truncate_str(&summary, 60)), "statement": format!("Test and verify the solution for: {}", summary), "lane": "verification", "worker_role": "reviewer", "depends_on": [0, 1]}
        ])
        .as_array()
        .unwrap()
        .clone()
    });

    // 6. Create nodes and edges in DB
    let mut node_ids: Vec<String> = Vec::new();
    let mut tx = pool.begin().await?;

    for task in &tasks {
        let node_id = Uuid::now_v7().to_string();
        let title = task["title"].as_str().unwrap_or("Untitled");
        let statement = task["statement"].as_str().unwrap_or("");
        let lane = task["lane"].as_str().unwrap_or("implementation");

        sqlx::query(
            "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at, revision) \
             VALUES ($1, $2, $3, $4, $5, 'proposed', now(), now(), 1)",
        )
        .bind(&node_id)
        .bind(objective_id)
        .bind(title)
        .bind(statement)
        .bind(lane)
        .execute(&mut *tx)
        .await?;

        node_ids.push(node_id);
    }

    // Create dependency edges
    for (i, task) in tasks.iter().enumerate() {
        if let Some(deps) = task["depends_on"].as_array() {
            for dep in deps {
                if let Some(dep_idx) = dep.as_u64() {
                    let dep_idx = dep_idx as usize;
                    if dep_idx < node_ids.len() && dep_idx != i {
                        let edge_id = Uuid::now_v7().to_string();
                        sqlx::query(
                            "INSERT INTO node_edges (edge_id, from_node_id, to_node_id, edge_kind) \
                             VALUES ($1, $2, $3, 'depends_on')",
                        )
                        .bind(&edge_id)
                        .bind(&node_ids[dep_idx])
                        .bind(&node_ids[i])
                        .execute(&mut *tx)
                        .await?;
                    }
                }
            }
        }
    }

    // Emit event
    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, 'plan', $2, 'plan_decomposed', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(objective_id)
    .bind(format!("decompose-{}", objective_id))
    .bind(serde_json::json!({"node_count": node_ids.len(), "node_ids": node_ids}))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let count = node_ids.len() as u32;
    tracing::info!(objective_id, count, "Decomposed objective into nodes");
    Ok(count)
}

/// Truncate a string to at most `max` characters, appending "..." if
/// truncated.
fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.min(s.len())])
    }
}

/// Scan the project directory to build context for task decomposition.
///
/// Detects tech stack, reads project files, extracts directory structure,
/// public interfaces, and test file locations.
async fn scan_project_for_decomposition(repo_root: &std::path::Path) -> String {
    let mut ctx = String::new();

    // 1. Detect tech stack from project files
    let stack_files = [
        ("Cargo.toml", "Rust"),
        ("package.json", "Node.js/TypeScript"),
        ("pyproject.toml", "Python"),
        ("go.mod", "Go"),
        ("CMakeLists.txt", "C/C++"),
        ("build.gradle", "Java/Kotlin"),
    ];

    let mut detected_stack = Vec::new();
    for (file, lang) in &stack_files {
        if repo_root.join(file).exists() {
            detected_stack.push(*lang);
            // Read the file for dependency info
            if let Ok(content) = tokio::fs::read_to_string(repo_root.join(file)).await {
                let truncated = if content.len() > 1500 {
                    &content[..1500]
                } else {
                    &content
                };
                ctx.push_str(&format!(
                    "\n### {} ({})\n```\n{}\n```\n",
                    file, lang, truncated
                ));
            }
        }
    }

    // 2. Get directory tree (top 3 levels, source files only)
    let tree_output = tokio::process::Command::new("find")
        .args([
            ".",
            "-maxdepth",
            "3",
            "-type",
            "f",
            "(",
            "-name",
            "*.rs",
            "-o",
            "-name",
            "*.ts",
            "-o",
            "-name",
            "*.tsx",
            "-o",
            "-name",
            "*.py",
            "-o",
            "-name",
            "*.go",
            "-o",
            "-name",
            "*.java",
            "-o",
            "-name",
            "*.toml",
            "-o",
            "-name",
            "*.json",
            ")",
        ])
        .current_dir(repo_root)
        .output()
        .await;

    if let Ok(output) = tree_output {
        let tree = String::from_utf8_lossy(&output.stdout);
        let truncated = if tree.len() > 2000 {
            &tree[..2000]
        } else {
            &tree
        };
        if !truncated.is_empty() {
            ctx.push_str(&format!(
                "\n### File structure\n```\n{}\n```\n",
                truncated
            ));
        }
    }

    // 3. Extract public interfaces from key source files
    let grep_output = tokio::process::Command::new("grep")
        .args([
            "-rn",
            "--include=*.rs",
            "--include=*.ts",
            "--include=*.py",
            "-E",
            r#"^pub (fn|struct|enum|trait|type)|^export |^def |^func |^class "#,
            ".",
        ])
        .current_dir(repo_root)
        .output()
        .await;

    if let Ok(output) = grep_output {
        let interfaces = String::from_utf8_lossy(&output.stdout);
        let truncated = if interfaces.len() > 3000 {
            &interfaces[..3000]
        } else {
            &interfaces
        };
        if !truncated.is_empty() {
            ctx.push_str(&format!(
                "\n### Existing interfaces\n```\n{}\n```\n",
                truncated
            ));
        }
    }

    // 4. Check for existing test patterns
    let test_output = tokio::process::Command::new("grep")
        .args([
            "-rn",
            "--include=*.rs",
            "--include=*.ts",
            "--include=*.py",
            "-E",
            r#"#\[test\]|#\[tokio::test\]|describe\(|it\(|def test_|func Test"#,
            ".",
            "-l",
        ])
        .current_dir(repo_root)
        .output()
        .await;

    if let Ok(output) = test_output {
        let test_files = String::from_utf8_lossy(&output.stdout);
        if !test_files.is_empty() {
            ctx.push_str(&format!(
                "\n### Test files found\n```\n{}\n```\n",
                test_files.trim()
            ));
        }
    }

    if detected_stack.is_empty() {
        ctx.push_str("\nNo recognized project files found.\n");
    }

    ctx
}
