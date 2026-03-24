-- M6: Observability, Deployment, and Hardening
-- Items: OBS-001..OBS-010, DEP-001..DEP-012, ROB-019, ROB-020

-- ── Observability tables ────────────────────────────────────────────────

-- OBS-001: Cycle metrics
CREATE TABLE IF NOT EXISTS cycle_metrics (
    id              TEXT PRIMARY KEY,
    cycle_id        TEXT NOT NULL,
    duration_ms     BIGINT NOT NULL,
    queue_time_ms   BIGINT NOT NULL,
    execution_time_ms BIGINT NOT NULL,
    review_time_ms  BIGINT NOT NULL,
    certification_time_ms BIGINT NOT NULL,
    tasks_completed INTEGER NOT NULL DEFAULT 0,
    tasks_failed    INTEGER NOT NULL DEFAULT 0,
    blocking_causes JSONB NOT NULL DEFAULT '[]',
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OBS-002: Task metrics
CREATE TABLE IF NOT EXISTS task_metrics (
    id               TEXT PRIMARY KEY,
    task_id          TEXT NOT NULL,
    cycle_id         TEXT NOT NULL,
    worker_role      TEXT NOT NULL,
    duration_ms      BIGINT NOT NULL,
    retry_count      INTEGER NOT NULL DEFAULT 0,
    succeeded        BOOLEAN NOT NULL,
    failure_category TEXT,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OBS-003: Cost records
CREATE TABLE IF NOT EXISTS cost_records (
    record_id        TEXT PRIMARY KEY,
    cycle_id         TEXT NOT NULL,
    task_id          TEXT,
    provider         TEXT NOT NULL,
    cost_amount      DOUBLE PRECISION NOT NULL,
    cost_currency    TEXT NOT NULL DEFAULT 'USD',
    source_provenance TEXT NOT NULL,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OBS-004: Token records
CREATE TABLE IF NOT EXISTS token_records (
    record_id        TEXT PRIMARY KEY,
    cycle_id         TEXT NOT NULL,
    task_id          TEXT,
    provider         TEXT NOT NULL,
    model            TEXT NOT NULL,
    input_tokens     BIGINT NOT NULL,
    input_provenance TEXT NOT NULL,
    output_tokens    BIGINT NOT NULL,
    output_provenance TEXT NOT NULL,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OBS-005: Worker success rates
CREATE TABLE IF NOT EXISTS worker_success_rates (
    id            TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    worker_role   TEXT NOT NULL,
    window_start  TIMESTAMPTZ NOT NULL,
    window_end    TIMESTAMPTZ NOT NULL,
    total_attempts INTEGER NOT NULL,
    successes     INTEGER NOT NULL,
    failures      INTEGER NOT NULL,
    success_rate  DOUBLE PRECISION NOT NULL
);

-- OBS-006: Saturation snapshots
CREATE TABLE IF NOT EXISTS saturation_snapshots (
    id                    TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    ready_queue_depth     INTEGER NOT NULL,
    running_tasks         INTEGER NOT NULL,
    blocked_tasks         INTEGER NOT NULL,
    review_backlog        INTEGER NOT NULL,
    certification_backlog INTEGER NOT NULL,
    pressure_level        TEXT NOT NULL,
    recorded_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Deployment tables ───────────────────────────────────────────────────

-- DEP-001, DEP-006: Deployment policies
CREATE TABLE IF NOT EXISTS deployment_policies (
    policy_id       TEXT PRIMARY KEY,
    revision        INTEGER NOT NULL DEFAULT 1,
    scope           TEXT NOT NULL DEFAULT 'global',
    deployment_mode TEXT NOT NULL,
    update_channel  JSONB NOT NULL,
    migration_compatibility JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- DEP-002, DEP-003: Remote endpoints
CREATE TABLE IF NOT EXISTS remote_endpoints (
    endpoint_id     TEXT PRIMARY KEY,
    endpoint_type   TEXT NOT NULL,
    label           TEXT NOT NULL,
    base_url        TEXT NOT NULL,
    auth_method     TEXT NOT NULL,
    timeout_ms      BIGINT NOT NULL,
    max_concurrent  INTEGER,
    lean_version    TEXT,
    health          TEXT NOT NULL DEFAULT 'unknown',
    last_health_check TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- DEP-004: Update channels
CREATE TABLE IF NOT EXISTS update_channels (
    id              TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    channel         TEXT NOT NULL,
    pinned_version  TEXT,
    auto_apply      BOOLEAN NOT NULL DEFAULT false,
    notify_on_available BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- DEP-005: Migration compatibility records
CREATE TABLE IF NOT EXISTS migration_compatibility_records (
    record_id       TEXT PRIMARY KEY,
    source_from     TEXT NOT NULL,
    source_to       TEXT NOT NULL,
    target_from     TEXT NOT NULL,
    target_to       TEXT NOT NULL,
    status          TEXT NOT NULL,
    notes           TEXT,
    assessed_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Retention and archive tables (ROB-019, ROB-020) ─────────────────────

-- ROB-019: Retention policies
CREATE TABLE IF NOT EXISTS retention_policies (
    id                TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    scope             TEXT NOT NULL,
    min_retention_days INTEGER NOT NULL,
    max_retention_days INTEGER NOT NULL DEFAULT 0,
    exceptions        JSONB NOT NULL DEFAULT '[]',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ROB-020: Archive manifests
CREATE TABLE IF NOT EXISTS archive_manifests (
    id                    TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    artifact_kind         TEXT NOT NULL,
    artifact_id           TEXT NOT NULL,
    compressed            BOOLEAN NOT NULL DEFAULT false,
    rebuild_rule          TEXT NOT NULL DEFAULT 'full_restore',
    original_size_bytes   BIGINT,
    archived_size_bytes   BIGINT,
    archived_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
