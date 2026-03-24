-- FCG-004 through FCG-015: Runtime strengthening for certification pipeline.
--
-- Adds:
-- - nodes.certification_required flag (FCG-004)
-- - certification_claim_links table (FCG-014)
-- - submission_id column on certification_refs for direct linkage (FCG-010)
-- - Index improvements for FIFO queue processing (FCG-008)

-- ── FCG-004: certification_required flag on nodes ────────────────────────

ALTER TABLE nodes ADD COLUMN IF NOT EXISTS certification_required BOOLEAN NOT NULL DEFAULT FALSE;

-- ── FCG-010: Add submission_id to certification_refs for direct linkage ──

ALTER TABLE certification_refs ADD COLUMN IF NOT EXISTS submission_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_certification_refs_submission
    ON certification_refs (submission_id) WHERE submission_id IS NOT NULL;

-- ── FCG-014: Claim/ref linkage table ─────────────────────────────────────

CREATE TABLE IF NOT EXISTS certification_claim_links (
    link_id               TEXT PRIMARY KEY,
    submission_id         TEXT NOT NULL REFERENCES certification_submissions(submission_id),
    local_ref_kind        TEXT NOT NULL,
    local_ref_id          TEXT NOT NULL,
    linkage_description   TEXT NOT NULL DEFAULT '',
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_cert_claim_links_submission
    ON certification_claim_links (submission_id);

CREATE INDEX IF NOT EXISTS idx_cert_claim_links_ref
    ON certification_claim_links (local_ref_kind, local_ref_id);

-- ── FCG-008: Index for FIFO queue ordering ───────────────────────────────

CREATE INDEX IF NOT EXISTS idx_cert_submissions_fifo
    ON certification_submissions (submitted_at ASC)
    WHERE queue_status = 'pending';
