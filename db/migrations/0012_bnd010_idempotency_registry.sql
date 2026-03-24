-- BND-010: Scoped idempotency registry
--
-- Problem: idempotency checks used (aggregate_kind, idempotency_key) but the
-- DB unique constraint was (aggregate_kind, aggregate_id, idempotency_key).
-- This mismatch meant the same idempotency_key across different aggregates of
-- the same kind could return the wrong existing record.
--
-- Fix part 1: Add a unique index on (aggregate_kind, idempotency_key) so the
-- lookup invariant is enforced at the DB level.  This prevents two different
-- aggregates of the same kind from sharing an idempotency_key.
--
-- Fix part 2: Create a dedicated idempotency_registry table for richer
-- operation-scoped idempotency (future use).

-- Enforce that idempotency_key is unique per aggregate_kind.
-- The existing 3-column unique constraint stays for backward compat.
CREATE UNIQUE INDEX IF NOT EXISTS uq_event_journal_kind_idem
    ON event_journal (aggregate_kind, idempotency_key);

-- Dedicated registry for operation-scoped idempotency.
CREATE TABLE IF NOT EXISTS idempotency_registry (
    operation    TEXT        NOT NULL,
    idempotency_key TEXT    NOT NULL,
    request_hash TEXT       NOT NULL,
    aggregate_id TEXT,
    outcome      TEXT        NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL DEFAULT now() + interval '24 hours',
    PRIMARY KEY (operation, idempotency_key)
);

-- Index for expiry-based cleanup
CREATE INDEX IF NOT EXISTS idx_idempotency_registry_expires
    ON idempotency_registry (expires_at);
