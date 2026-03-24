-- M3: Worker Governance Core
--
-- This migration adds tables for:
-- - Worker registrations (WRK-001, WRK-002)
-- - Worker leases (WRK-003, WRK-004)
-- - Worker heartbeats (WRK-005, WRK-006)
-- - Adapter invocations / provenance (ADT-008)
-- - Policy overrides (POL-011)
-- - Policy versions (POL-001)
-- - Policy cycle snapshots (POL-002)

-- ── Worker registrations ─────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS worker_registrations (
    worker_id         TEXT PRIMARY KEY,
    worker_role       TEXT NOT NULL,
    skill_pack_id     TEXT NOT NULL,
    state             TEXT NOT NULL DEFAULT 'registered',
    accepted_task_kinds JSONB NOT NULL DEFAULT '[]'::jsonb,
    max_concurrency   INTEGER NOT NULL DEFAULT 1,
    provider_mode     TEXT NOT NULL,
    model_binding     TEXT NOT NULL,
    registered_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    state_changed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_worker_registrations_state
    ON worker_registrations (state);

CREATE INDEX IF NOT EXISTS idx_worker_registrations_role
    ON worker_registrations (worker_role);

-- ── Worker leases ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS worker_leases (
    lease_id       TEXT PRIMARY KEY,
    worker_id      TEXT NOT NULL REFERENCES worker_registrations(worker_id),
    task_id        TEXT,
    granted_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL,
    active         BOOLEAN NOT NULL DEFAULT TRUE,
    renewal_count  INTEGER NOT NULL DEFAULT 0,
    max_renewals   INTEGER NOT NULL DEFAULT 10
);

CREATE INDEX IF NOT EXISTS idx_worker_leases_worker
    ON worker_leases (worker_id);

CREATE INDEX IF NOT EXISTS idx_worker_leases_active
    ON worker_leases (active) WHERE active = TRUE;

CREATE INDEX IF NOT EXISTS idx_worker_leases_expires
    ON worker_leases (expires_at) WHERE active = TRUE;

-- ── Worker heartbeats ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS worker_heartbeats (
    heartbeat_id     TEXT PRIMARY KEY,
    worker_id        TEXT NOT NULL REFERENCES worker_registrations(worker_id),
    task_id          TEXT,
    status           TEXT NOT NULL,
    progress_percent SMALLINT NOT NULL DEFAULT 0,
    phase            TEXT NOT NULL DEFAULT '',
    resource_usage   JSONB,
    received_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_worker_heartbeats_worker
    ON worker_heartbeats (worker_id, received_at DESC);

-- ── Adapter invocations (provenance) ─────────────────────────────────────

CREATE TABLE IF NOT EXISTS adapter_invocations (
    invocation_id   TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    worker_id       TEXT NOT NULL,
    agent_kind      TEXT NOT NULL,
    input_summary   TEXT NOT NULL,
    input_hash      TEXT NOT NULL,
    output_summary  TEXT,
    output_hash     TEXT,
    exit_code       INTEGER,
    stderr_capture  TEXT,
    outcome         TEXT NOT NULL,
    duration_ms     BIGINT NOT NULL DEFAULT 0,
    retry_attempt   INTEGER NOT NULL DEFAULT 0,
    utf8_valid      BOOLEAN NOT NULL DEFAULT TRUE,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_adapter_invocations_task
    ON adapter_invocations (task_id);

CREATE INDEX IF NOT EXISTS idx_adapter_invocations_worker
    ON adapter_invocations (worker_id);

CREATE INDEX IF NOT EXISTS idx_adapter_invocations_outcome
    ON adapter_invocations (outcome);

-- ── Policy overrides ─────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS policy_overrides (
    override_id     TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    field           TEXT NOT NULL,
    override_value  JSONB NOT NULL,
    justification   TEXT NOT NULL,
    approved_by     TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    active          BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_policy_overrides_task
    ON policy_overrides (task_id) WHERE active = TRUE;

-- ── Policy versions ──────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS policy_versions (
    version         INTEGER PRIMARY KEY,
    created_by      TEXT NOT NULL,
    change_reason   TEXT NOT NULL,
    content_hash    TEXT NOT NULL,
    policy_content  JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Policy cycle snapshots ───────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS policy_cycle_snapshots (
    snapshot_id     TEXT PRIMARY KEY,
    cycle_id        TEXT NOT NULL,
    policy_version  INTEGER NOT NULL REFERENCES policy_versions(version),
    policy_content  JSONB NOT NULL,
    snapshotted_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_policy_cycle_snapshots_cycle
    ON policy_cycle_snapshots (cycle_id);
