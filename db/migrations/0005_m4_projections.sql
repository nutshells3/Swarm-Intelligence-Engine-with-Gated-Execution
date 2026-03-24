-- M4: Live Projections and Core UI
--
-- Projection cache tables for pre-computed read models.
-- These tables are *derived* from the authoritative event journal
-- and command outcomes.  They may be rebuilt from events at any time.
--
-- Projections are optional caches -- the system can compute them
-- on-the-fly from the event journal if these tables are empty.

-- ── Projection metadata ───────────────────────────────────────────────────
-- Tracks when each projection was last computed for staleness detection.

CREATE TABLE IF NOT EXISTS projection_metadata (
    projection_name TEXT PRIMARY KEY,
    last_computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_event_id   TEXT,
    item_count       INTEGER NOT NULL DEFAULT 0
);

-- ── Task board projection cache ───────────────────────────────────────────
-- RDM-001: Flat view of tasks for the task board panel.

CREATE TABLE IF NOT EXISTS projection_task_board (
    task_id           TEXT PRIMARY KEY,
    node_id           TEXT NOT NULL,
    title             TEXT NOT NULL DEFAULT '',
    worker_role       TEXT NOT NULL,
    status            TEXT NOT NULL,
    assigned_worker_id TEXT,
    attempt_number    INTEGER NOT NULL DEFAULT 1,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Node graph projection cache ──────────────────────────────────────────
-- RDM-002: Nodes and edges for the graph visualization.

CREATE TABLE IF NOT EXISTS projection_node_graph (
    node_id           TEXT PRIMARY KEY,
    objective_id      TEXT NOT NULL,
    title             TEXT NOT NULL,
    lane              TEXT NOT NULL,
    lifecycle         TEXT NOT NULL,
    task_count        INTEGER NOT NULL DEFAULT 0,
    completed_task_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS projection_node_graph_edge (
    edge_id           TEXT PRIMARY KEY,
    from_node_id      TEXT NOT NULL,
    to_node_id        TEXT NOT NULL,
    edge_kind         TEXT NOT NULL
);

-- ── Branch/mainline projection cache ─────────────────────────────────────
-- RDM-003: Nodes grouped by lane.

CREATE TABLE IF NOT EXISTS projection_branch_mainline (
    node_id           TEXT PRIMARY KEY,
    title             TEXT NOT NULL,
    lane              TEXT NOT NULL,
    lifecycle         TEXT NOT NULL,
    promotion_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    review_status     TEXT
);

-- ── Review queue projection cache ────────────────────────────────────────
-- RDM-004: Items awaiting review.

CREATE TABLE IF NOT EXISTS projection_review_queue (
    review_id         TEXT PRIMARY KEY,
    review_kind       TEXT NOT NULL,
    target_ref        TEXT NOT NULL,
    target_title      TEXT NOT NULL DEFAULT '',
    status            TEXT NOT NULL,
    submitted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Certification queue projection cache ─────────────────────────────────
-- RDM-005: Items awaiting certification.

CREATE TABLE IF NOT EXISTS projection_certification_queue (
    certification_id  TEXT PRIMARY KEY,
    node_id           TEXT NOT NULL,
    node_title        TEXT NOT NULL DEFAULT '',
    certification_kind TEXT NOT NULL,
    status            TEXT NOT NULL,
    submitted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Drift projection cache ──────────────────────────────────────────────
-- RDM-006: Nodes with detected drift.

CREATE TABLE IF NOT EXISTS projection_drift (
    drift_id          TEXT PRIMARY KEY,
    node_id           TEXT NOT NULL,
    node_title        TEXT NOT NULL DEFAULT '',
    drift_source      TEXT NOT NULL,
    drift_description TEXT NOT NULL DEFAULT '',
    detected_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved          BOOLEAN NOT NULL DEFAULT FALSE
);

-- ── Conflict queue projection cache ──────────────────────────────────────
-- RDM-007: Active conflicts.

CREATE TABLE IF NOT EXISTS projection_conflict_queue (
    conflict_id       TEXT PRIMARY KEY,
    description       TEXT NOT NULL DEFAULT '',
    affected_node_ids TEXT[] NOT NULL DEFAULT '{}',
    status            TEXT NOT NULL,
    detected_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Objective progress projection cache ──────────────────────────────────
-- RDM-008: Progress per objective.

CREATE TABLE IF NOT EXISTS projection_objective_progress (
    objective_id      TEXT PRIMARY KEY,
    summary           TEXT NOT NULL DEFAULT '',
    total_nodes       INTEGER NOT NULL DEFAULT 0,
    completed_nodes   INTEGER NOT NULL DEFAULT 0,
    running_nodes     INTEGER NOT NULL DEFAULT 0,
    blocked_nodes     INTEGER NOT NULL DEFAULT 0,
    total_tasks       INTEGER NOT NULL DEFAULT 0,
    completed_tasks   INTEGER NOT NULL DEFAULT 0,
    progress_percent  SMALLINT NOT NULL DEFAULT 0
);

-- ── Loop history projection cache ────────────────────────────────────────
-- RDM-009: Cycle summaries within a loop.

CREATE TABLE IF NOT EXISTS projection_loop_history (
    cycle_id          TEXT PRIMARY KEY,
    loop_id           TEXT NOT NULL,
    objective_id      TEXT NOT NULL,
    cycle_index       INTEGER NOT NULL,
    phase             TEXT NOT NULL,
    tasks_dispatched  INTEGER NOT NULL DEFAULT 0,
    tasks_completed   INTEGER NOT NULL DEFAULT 0,
    tasks_failed      INTEGER NOT NULL DEFAULT 0,
    nodes_promoted    INTEGER NOT NULL DEFAULT 0,
    started_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at      TIMESTAMPTZ
);

-- ── Artifact timeline projection cache ───────────────────────────────────
-- RDM-010: Chronological artifact list.

CREATE TABLE IF NOT EXISTS projection_artifact_timeline (
    artifact_id       TEXT PRIMARY KEY,
    artifact_kind     TEXT NOT NULL,
    title             TEXT NOT NULL DEFAULT '',
    source_task_id    TEXT,
    source_node_id    TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Git control tables ───────────────────────────────────────────────────
-- GIT-002, GIT-003, GIT-009, GIT-010: Persistent rules and assignments.

CREATE TABLE IF NOT EXISTS git_branch_ownership_rules (
    rule_id           TEXT PRIMARY KEY,
    branch_pattern    TEXT NOT NULL,
    scope             TEXT NOT NULL,
    owning_role       TEXT,
    owning_worker_id  TEXT,
    active            BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS git_worktree_assignments (
    assignment_id     TEXT PRIMARY KEY,
    worker_id         TEXT NOT NULL,
    task_id           TEXT NOT NULL,
    worktree_path     TEXT NOT NULL,
    branch_name       TEXT NOT NULL,
    assigned_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    released_at       TIMESTAMPTZ,
    active            BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS git_review_before_merge_rules (
    rule_id               TEXT PRIMARY KEY,
    branch_pattern        TEXT NOT NULL,
    require_review        BOOLEAN NOT NULL DEFAULT TRUE,
    require_certification BOOLEAN NOT NULL DEFAULT FALSE,
    require_all_tasks_complete BOOLEAN NOT NULL DEFAULT TRUE,
    require_no_conflicts  BOOLEAN NOT NULL DEFAULT TRUE,
    min_approvals         INTEGER NOT NULL DEFAULT 1,
    active                BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS git_safe_cleanup_rules (
    rule_id                   TEXT PRIMARY KEY,
    delete_merged_branches    BOOLEAN NOT NULL DEFAULT TRUE,
    remove_completed_worktrees BOOLEAN NOT NULL DEFAULT TRUE,
    cleanup_grace_seconds     BIGINT NOT NULL DEFAULT 300,
    archive_abandoned         BOOLEAN NOT NULL DEFAULT TRUE,
    max_archived_branches     INTEGER NOT NULL DEFAULT 50,
    require_clean_before_delete BOOLEAN NOT NULL DEFAULT TRUE
);

-- ── Indexes ──────────────────────────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_proj_task_board_status ON projection_task_board(status);
CREATE INDEX IF NOT EXISTS idx_proj_task_board_node ON projection_task_board(node_id);
CREATE INDEX IF NOT EXISTS idx_proj_node_graph_objective ON projection_node_graph(objective_id);
CREATE INDEX IF NOT EXISTS idx_proj_branch_lane ON projection_branch_mainline(lane);
CREATE INDEX IF NOT EXISTS idx_proj_review_status ON projection_review_queue(status);
CREATE INDEX IF NOT EXISTS idx_proj_cert_status ON projection_certification_queue(status);
CREATE INDEX IF NOT EXISTS idx_proj_drift_resolved ON projection_drift(resolved);
CREATE INDEX IF NOT EXISTS idx_proj_conflict_status ON projection_conflict_queue(status);
CREATE INDEX IF NOT EXISTS idx_proj_loop_history_loop ON projection_loop_history(loop_id);
CREATE INDEX IF NOT EXISTS idx_proj_artifact_kind ON projection_artifact_timeline(artifact_kind);
CREATE INDEX IF NOT EXISTS idx_git_worktree_active ON git_worktree_assignments(active);
