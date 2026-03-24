-- M8: Formal Validation Readiness
--
-- This migration adds tables for:
-- - Formal predicates (FRM-001 to FRM-007)
-- - Predicate evaluations (FRM-001 to FRM-007)
-- - Formal exports (FRM-008)
-- - Projection consistency checks (FRM-009)
-- - Readiness contracts (FRM-010)
--
-- Design constraints:
-- - NO Lean/Isabelle dependencies -- readiness only.
-- - All predicates must be replayable from durable state alone.
-- - Export format must be backend-neutral.

-- ── Formal predicates (FRM-001 to FRM-007) ──────────────────────────────

CREATE TABLE IF NOT EXISTS formal_predicates (
    predicate_id          TEXT PRIMARY KEY,
    predicate_category    TEXT NOT NULL,
    name                  TEXT NOT NULL,
    description           TEXT NOT NULL,
    inputs                JSONB NOT NULL DEFAULT '[]'::jsonb,
    extra                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    version               TEXT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_formal_predicates_category
    ON formal_predicates (predicate_category);

CREATE INDEX IF NOT EXISTS idx_formal_predicates_version
    ON formal_predicates (version);

-- ── Predicate evaluations (FRM-001 to FRM-007) ──────────────────────────

CREATE TABLE IF NOT EXISTS predicate_evaluations (
    evaluation_id         TEXT PRIMARY KEY,
    predicate_id          TEXT NOT NULL REFERENCES formal_predicates(predicate_id),
    outcome               TEXT NOT NULL,
    reason                TEXT NOT NULL,
    predicate_version     TEXT NOT NULL,
    input_snapshot        JSONB NOT NULL DEFAULT '{}'::jsonb,
    evaluated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_predicate_evaluations_predicate
    ON predicate_evaluations (predicate_id);

CREATE INDEX IF NOT EXISTS idx_predicate_evaluations_outcome
    ON predicate_evaluations (outcome);

-- ── Formal exports (FRM-008) ────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS formal_exports (
    export_id             TEXT PRIMARY KEY,
    export_version        TEXT NOT NULL,
    source_schema_hash    TEXT NOT NULL,
    predicates            JSONB NOT NULL DEFAULT '[]'::jsonb,
    graph_facts           JSONB NOT NULL DEFAULT '[]'::jsonb,
    lifecycle_states      JSONB NOT NULL DEFAULT '[]'::jsonb,
    approval_effects      JSONB NOT NULL DEFAULT '[]'::jsonb,
    certification_facts   JSONB NOT NULL DEFAULT '[]'::jsonb,
    exported_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_formal_exports_version
    ON formal_exports (export_version);

CREATE INDEX IF NOT EXISTS idx_formal_exports_hash
    ON formal_exports (source_schema_hash);

-- ── Projection consistency checks (FRM-009) ─────────────────────────────

CREATE TABLE IF NOT EXISTS projection_consistency_checks (
    check_id              TEXT PRIMARY KEY,
    domain                TEXT NOT NULL,
    status                TEXT NOT NULL,
    mismatches            JSONB NOT NULL DEFAULT '[]'::jsonb,
    rebuild_action        TEXT NOT NULL DEFAULT 'none',
    authoritative_schema_version TEXT NOT NULL,
    projection_schema_version    TEXT NOT NULL,
    checked_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_proj_consistency_domain
    ON projection_consistency_checks (domain);

CREATE INDEX IF NOT EXISTS idx_proj_consistency_status
    ON projection_consistency_checks (status);

-- ── Readiness contracts (FRM-010) ────────────────────────────────────────

CREATE TABLE IF NOT EXISTS readiness_contracts (
    contract_id                TEXT PRIMARY KEY,
    contract_version           TEXT NOT NULL,
    naming_conventions         JSONB NOT NULL DEFAULT '[]'::jsonb,
    proof_candidates           JSONB NOT NULL DEFAULT '[]'::jsonb,
    integration_boundary_notes JSONB NOT NULL DEFAULT '[]'::jsonb,
    runtime_dependency_assertion BOOLEAN NOT NULL DEFAULT false,
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                 TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_readiness_contracts_version
    ON readiness_contracts (contract_version);

-- Constraint: runtime_dependency_assertion must be false.
-- This is a database-level enforcement of FRM-010's core invariant.
ALTER TABLE readiness_contracts
    ADD CONSTRAINT chk_no_runtime_dependency
    CHECK (runtime_dependency_assertion = false);
