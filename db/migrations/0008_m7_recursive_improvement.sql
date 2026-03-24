-- M7: Recursive Improvement
--
-- This migration adds tables for:
-- - Self-improvement objectives (REC-001)
-- - Repo-target policies (REC-002)
-- - Safety gate configurations (REC-003)
-- - Comparison artifacts (REC-004)
-- - Loop scores (REC-005)
-- - Drift check artifacts (REC-007)
-- - Self-promotion attempts (REC-008)
-- - Recursive reports (REC-009)
-- - Recursive memory entries (REC-010)

-- ── Self-improvement objectives (REC-001) ────────────────────────────────

CREATE TABLE IF NOT EXISTS self_improvement_objectives (
    objective_id            TEXT PRIMARY KEY,
    classification          TEXT NOT NULL DEFAULT 'self_improvement',
    summary                 TEXT NOT NULL,
    rationale               TEXT NOT NULL,
    repo_target             TEXT NOT NULL,
    trust_boundary_impact   TEXT NOT NULL,
    is_recursive            BOOLEAN NOT NULL DEFAULT FALSE,
    parent_objective_id     TEXT,
    required_gate_level     TEXT NOT NULL,
    status                  TEXT NOT NULL DEFAULT 'proposed',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_si_objectives_status
    ON self_improvement_objectives (status);

CREATE INDEX IF NOT EXISTS idx_si_objectives_parent
    ON self_improvement_objectives (parent_objective_id);

-- ── Repo-target policies (REC-002) ───────────────────────────────────────

CREATE TABLE IF NOT EXISTS repo_target_policies (
    policy_id               TEXT PRIMARY KEY,
    objective_id            TEXT NOT NULL REFERENCES self_improvement_objectives(objective_id),
    rules                   JSONB NOT NULL DEFAULT '[]'::jsonb,
    worktree_isolation      TEXT NOT NULL DEFAULT 'required',
    hard_block_undeclared   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_repo_target_policies_objective
    ON repo_target_policies (objective_id);

-- ── Safety gate configurations (REC-003) ─────────────────────────────────

CREATE TABLE IF NOT EXISTS safety_gate_configurations (
    objective_id            TEXT PRIMARY KEY REFERENCES self_improvement_objectives(objective_id),
    current_level           TEXT NOT NULL DEFAULT 'blocked',
    conditions              JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_actions         JSONB NOT NULL DEFAULT '{}'::jsonb,
    simulation_verified     BOOLEAN NOT NULL DEFAULT FALSE
);

-- ── Comparison artifacts (REC-004) ───────────────────────────────────────

CREATE TABLE IF NOT EXISTS comparison_artifacts (
    comparison_id           TEXT PRIMARY KEY,
    objective_id            TEXT NOT NULL REFERENCES self_improvement_objectives(objective_id),
    iteration_index         INTEGER NOT NULL,
    baseline                JSONB NOT NULL,
    proposal_summary        TEXT NOT NULL,
    changed_surfaces        JSONB NOT NULL DEFAULT '[]'::jsonb,
    metric_deltas           JSONB NOT NULL DEFAULT '[]'::jsonb,
    regression_risks        JSONB NOT NULL DEFAULT '[]'::jsonb,
    overall_assessment      TEXT NOT NULL DEFAULT 'needs_review',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_comparison_artifacts_objective
    ON comparison_artifacts (objective_id);

CREATE INDEX IF NOT EXISTS idx_comparison_artifacts_iteration
    ON comparison_artifacts (objective_id, iteration_index);

-- ── Loop scores (REC-005) ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS loop_scores (
    score_id                TEXT PRIMARY KEY,
    objective_id            TEXT NOT NULL REFERENCES self_improvement_objectives(objective_id),
    iteration_index         INTEGER NOT NULL,
    input                   JSONB NOT NULL,
    breakdown               JSONB NOT NULL,
    composite_score         DOUBLE PRECISION NOT NULL,
    advisory_only           BOOLEAN NOT NULL DEFAULT TRUE,
    recommendation          TEXT NOT NULL DEFAULT '',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_loop_scores_objective
    ON loop_scores (objective_id);

CREATE INDEX IF NOT EXISTS idx_loop_scores_iteration
    ON loop_scores (objective_id, iteration_index);

-- ── Drift check artifacts (REC-007) ──────────────────────────────────────

CREATE TABLE IF NOT EXISTS drift_check_artifacts (
    drift_check_id          TEXT PRIMARY KEY,
    objective_id            TEXT NOT NULL REFERENCES self_improvement_objectives(objective_id),
    iteration_index         INTEGER NOT NULL,
    policy_drifts           JSONB NOT NULL DEFAULT '[]'::jsonb,
    schema_drifts           JSONB NOT NULL DEFAULT '[]'::jsonb,
    skill_drifts            JSONB NOT NULL DEFAULT '[]'::jsonb,
    approval_drifts         JSONB NOT NULL DEFAULT '[]'::jsonb,
    overall_severity        TEXT NOT NULL DEFAULT 'none',
    has_unintentional_drift BOOLEAN NOT NULL DEFAULT FALSE,
    blocks_continuation     BOOLEAN NOT NULL DEFAULT FALSE,
    checked_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_drift_check_objective
    ON drift_check_artifacts (objective_id);

CREATE INDEX IF NOT EXISTS idx_drift_check_iteration
    ON drift_check_artifacts (objective_id, iteration_index);

-- ── Self-promotion attempts (REC-008, P0!) ───────────────────────────────

CREATE TABLE IF NOT EXISTS self_promotion_attempts (
    attempt_id              TEXT PRIMARY KEY,
    source_objective_id     TEXT NOT NULL REFERENCES self_improvement_objectives(objective_id),
    artifact_ref            TEXT NOT NULL,
    promotion_kind          TEXT NOT NULL,
    description             TEXT NOT NULL,
    denial_result           TEXT NOT NULL DEFAULT 'denied',
    override_id             TEXT,
    detected_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_self_promotion_objective
    ON self_promotion_attempts (source_objective_id);

CREATE INDEX IF NOT EXISTS idx_self_promotion_result
    ON self_promotion_attempts (denial_result);

CREATE TABLE IF NOT EXISTS self_promotion_overrides (
    override_id             TEXT PRIMARY KEY,
    attempt_id              TEXT NOT NULL REFERENCES self_promotion_attempts(attempt_id),
    requested_by            TEXT NOT NULL,
    justification           TEXT NOT NULL,
    status                  TEXT NOT NULL DEFAULT 'pending',
    reviewed_by             TEXT,
    review_notes            TEXT,
    requested_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at             TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_self_promotion_overrides_attempt
    ON self_promotion_overrides (attempt_id);

CREATE INDEX IF NOT EXISTS idx_self_promotion_overrides_status
    ON self_promotion_overrides (status);

-- ── Recursive reports (REC-009) ──────────────────────────────────────────

CREATE TABLE IF NOT EXISTS recursive_reports (
    report_id               TEXT PRIMARY KEY,
    objective_id            TEXT NOT NULL REFERENCES self_improvement_objectives(objective_id),
    iteration_index         INTEGER NOT NULL,
    sections                JSONB NOT NULL DEFAULT '[]'::jsonb,
    recommendations         JSONB NOT NULL DEFAULT '[]'::jsonb,
    related_artifact_refs   JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_complete             BOOLEAN NOT NULL DEFAULT FALSE,
    has_blockers            BOOLEAN NOT NULL DEFAULT FALSE,
    generated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_recursive_reports_objective
    ON recursive_reports (objective_id);

CREATE INDEX IF NOT EXISTS idx_recursive_reports_iteration
    ON recursive_reports (objective_id, iteration_index);

-- ── Recursive memory entries (REC-010) ───────────────────────────────────
-- Memory is append-only: no UPDATE or DELETE operations should be
-- performed on this table in normal operation.

CREATE TABLE IF NOT EXISTS recursive_memory_entries (
    entry_id                TEXT PRIMARY KEY,
    objective_id            TEXT NOT NULL,
    outcome                 TEXT NOT NULL,
    learned_summary         TEXT NOT NULL,
    outcome_metrics         JSONB NOT NULL DEFAULT '{}'::jsonb,
    supersedes_entry_id     TEXT,
    recorded_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_recursive_memory_objective
    ON recursive_memory_entries (objective_id);

CREATE INDEX IF NOT EXISTS idx_recursive_memory_supersedes
    ON recursive_memory_entries (supersedes_entry_id);
