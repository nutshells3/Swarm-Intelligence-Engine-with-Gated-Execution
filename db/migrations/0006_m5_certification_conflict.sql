-- M5: Certification And Conflict Integration
--
-- This migration adds tables for:
-- - Certification candidates (FCG-001)
-- - Certification submissions (FCG-002)
-- - Certification result projections (FCG-003)
-- - Stale invalidation records (FCG-012)
-- - Conflict records (CNF-001 to CNF-003)
-- - Adjudication tasks (CNF-008)
-- - Conflict history (CNF-009)
-- - Conflict resolutions (CNF-010)
-- - Review scheduling policies (REV-004)
-- - Review artifacts (REV-001)
-- - Heartbeat review triggers (REV-005)
-- - Auto-approval thresholds (REV-006)

-- ── Certification candidates (FCG-001) ─────────────────────────────────

CREATE TABLE IF NOT EXISTS certification_candidates (
    candidate_id          TEXT PRIMARY KEY,
    node_id               TEXT NOT NULL,
    task_id               TEXT NOT NULL,
    claim_summary         TEXT NOT NULL,
    source_anchors        JSONB NOT NULL DEFAULT '[]'::jsonb,
    eligibility_reason    TEXT NOT NULL,
    provenance_task_attempt_id TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cert_candidates_node
    ON certification_candidates (node_id);

CREATE INDEX IF NOT EXISTS idx_cert_candidates_task
    ON certification_candidates (task_id);

-- ── Certification submissions (FCG-002) ────────────────────────────────

CREATE TABLE IF NOT EXISTS certification_submissions (
    submission_id         TEXT PRIMARY KEY,
    candidate_id          TEXT NOT NULL REFERENCES certification_candidates(candidate_id),
    idempotency_key       TEXT NOT NULL UNIQUE,
    submitted_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    queue_status          TEXT NOT NULL DEFAULT 'pending',
    retry_count           INTEGER NOT NULL DEFAULT 0,
    max_retries           INTEGER NOT NULL DEFAULT 3,
    status_changed_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cert_submissions_candidate
    ON certification_submissions (candidate_id);

CREATE INDEX IF NOT EXISTS idx_cert_submissions_status
    ON certification_submissions (queue_status);

CREATE INDEX IF NOT EXISTS idx_cert_submissions_idempotency
    ON certification_submissions (idempotency_key);

-- ── Certification result projections (FCG-003) ─────────────────────────

CREATE TABLE IF NOT EXISTS certification_result_projections (
    submission_id         TEXT PRIMARY KEY REFERENCES certification_submissions(submission_id),
    external_gate         TEXT NOT NULL,
    local_gate_effect     TEXT NOT NULL,
    lane_transition       TEXT,
    projected_grade       TEXT NOT NULL,
    projected_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Stale invalidation records (FCG-012) ───────────────────────────────

CREATE TABLE IF NOT EXISTS stale_invalidation_records (
    invalidation_id       TEXT PRIMARY KEY,
    submission_id         TEXT NOT NULL REFERENCES certification_submissions(submission_id),
    candidate_id          TEXT NOT NULL REFERENCES certification_candidates(candidate_id),
    stale_reason          TEXT NOT NULL,
    triggering_change_ref TEXT NOT NULL,
    lifecycle_at_invalidation TEXT NOT NULL,
    lane_at_invalidation  TEXT NOT NULL,
    lane_demoted          BOOLEAN NOT NULL DEFAULT FALSE,
    revalidation_triggered BOOLEAN NOT NULL DEFAULT FALSE,
    invalidated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_stale_invalidation_submission
    ON stale_invalidation_records (submission_id);

CREATE INDEX IF NOT EXISTS idx_stale_invalidation_candidate
    ON stale_invalidation_records (candidate_id);

-- ── Conflict records (CNF-001 to CNF-003) ──────────────────────────────

CREATE TABLE IF NOT EXISTS conflict_records (
    conflict_id           TEXT PRIMARY KEY,
    conflict_fingerprint  TEXT NOT NULL UNIQUE,
    conflict_class        TEXT NOT NULL,
    trigger               TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'open',
    competing_artifacts   JSONB NOT NULL DEFAULT '[]'::jsonb,
    description           TEXT NOT NULL,
    blocks_promotion      BOOLEAN NOT NULL DEFAULT TRUE,
    semantic_conflict_id  TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_conflict_records_status
    ON conflict_records (status);

CREATE INDEX IF NOT EXISTS idx_conflict_records_class
    ON conflict_records (conflict_class);

CREATE INDEX IF NOT EXISTS idx_conflict_records_fingerprint
    ON conflict_records (conflict_fingerprint);

-- ── Adjudication tasks (CNF-008) ───────────────────────────────────────

CREATE TABLE IF NOT EXISTS adjudication_tasks (
    adjudication_id       TEXT PRIMARY KEY,
    conflict_id           TEXT NOT NULL REFERENCES conflict_records(conflict_id),
    urgency               TEXT NOT NULL DEFAULT 'normal',
    required_reviewer_role TEXT NOT NULL,
    context_summary       TEXT NOT NULL,
    competing_artifacts   JSONB NOT NULL DEFAULT '[]'::jsonb,
    assigned_worker_id    TEXT,
    adjudication_status   TEXT NOT NULL DEFAULT 'pending',
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_adjudication_tasks_conflict
    ON adjudication_tasks (conflict_id);

CREATE INDEX IF NOT EXISTS idx_adjudication_tasks_status
    ON adjudication_tasks (adjudication_status);

-- ── Conflict history (CNF-009) ─────────────────────────────────────────

CREATE TABLE IF NOT EXISTS conflict_history (
    history_entry_id      TEXT PRIMARY KEY,
    conflict_id           TEXT NOT NULL REFERENCES conflict_records(conflict_id),
    status_at_snapshot    TEXT NOT NULL,
    change_description    TEXT NOT NULL,
    snapshot              JSONB NOT NULL,
    recorded_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_conflict_history_conflict
    ON conflict_history (conflict_id, recorded_at DESC);

-- ── Conflict resolutions (CNF-010) ─────────────────────────────────────

CREATE TABLE IF NOT EXISTS conflict_resolutions (
    resolution_id         TEXT PRIMARY KEY,
    conflict_id           TEXT NOT NULL REFERENCES conflict_records(conflict_id),
    strategy              TEXT NOT NULL,
    winner_node_id        TEXT,
    rationale             TEXT NOT NULL,
    adjudication_id       TEXT REFERENCES adjudication_tasks(adjudication_id),
    resolved_by           TEXT NOT NULL,
    lifecycle_effects     JSONB NOT NULL DEFAULT '[]'::jsonb,
    resolved_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_conflict_resolutions_conflict
    ON conflict_resolutions (conflict_id);

-- ── Review scheduling policies (REV-004) ───────────────────────────────

CREATE TABLE IF NOT EXISTS review_scheduling_policies (
    policy_id             TEXT PRIMARY KEY,
    review_kind           TEXT NOT NULL,
    trigger_kind          TEXT NOT NULL,
    periodic_interval_secs INTEGER,
    trigger_phases        JSONB NOT NULL DEFAULT '[]'::jsonb,
    trigger_events        JSONB NOT NULL DEFAULT '[]'::jsonb,
    max_concurrent_reviews INTEGER NOT NULL DEFAULT 1,
    skip_if_in_progress   BOOLEAN NOT NULL DEFAULT FALSE,
    active                BOOLEAN NOT NULL DEFAULT TRUE,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_review_sched_policies_kind
    ON review_scheduling_policies (review_kind);

-- ── Review artifacts (REV-001) ─────────────────────────────────────────

CREATE TABLE IF NOT EXISTS review_artifacts (
    review_id             TEXT PRIMARY KEY,
    review_kind           TEXT NOT NULL,
    target_ref            TEXT NOT NULL,
    target_kind           TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'scheduled',
    outcome               TEXT,
    findings_summary      TEXT NOT NULL DEFAULT '',
    detailed_findings     JSONB NOT NULL DEFAULT '{}'::jsonb,
    conditions            JSONB NOT NULL DEFAULT '[]'::jsonb,
    reviewer              TEXT NOT NULL,
    is_auto_approval      BOOLEAN NOT NULL DEFAULT FALSE,
    approval_effect       TEXT NOT NULL DEFAULT '',
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_review_artifacts_kind
    ON review_artifacts (review_kind);

CREATE INDEX IF NOT EXISTS idx_review_artifacts_status
    ON review_artifacts (status);

CREATE INDEX IF NOT EXISTS idx_review_artifacts_target
    ON review_artifacts (target_ref);

-- ── Heartbeat review triggers (REV-005) ────────────────────────────────

CREATE TABLE IF NOT EXISTS heartbeat_review_triggers (
    trigger_id            TEXT PRIMARY KEY,
    review_kind           TEXT NOT NULL,
    cycle_interval        INTEGER NOT NULL DEFAULT 5,
    max_elapsed_secs      INTEGER NOT NULL DEFAULT 3600,
    task_count_threshold  INTEGER,
    force_on_no_change    BOOLEAN NOT NULL DEFAULT FALSE,
    last_triggered_at     TIMESTAMPTZ
);

-- ── Auto-approval thresholds (REV-006) ─────────────────────────────────

CREATE TABLE IF NOT EXISTS auto_approval_thresholds (
    threshold_id          TEXT PRIMARY KEY,
    review_kind           TEXT NOT NULL,
    auto_approval_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    max_auto_approvable_changes INTEGER,
    required_minimum_grade TEXT,
    require_all_criteria_satisfied BOOLEAN NOT NULL DEFAULT TRUE,
    forbidden             BOOLEAN NOT NULL DEFAULT FALSE,
    policy_justification  TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_auto_approval_review_kind
    ON auto_approval_thresholds (review_kind);
