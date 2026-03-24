-- 0011_seal_review_artifacts.sql
-- Backfill columns that migration 0006 intended to add to review_artifacts.
-- Migration 0002 created the table with 8 columns (thin schema).
-- Migration 0006 used CREATE TABLE IF NOT EXISTS with 13 columns (rich schema),
-- which was silently skipped because the table already existed.
-- This migration adds the missing columns so the table matches the 0006 intent.

ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS target_kind TEXT;
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS outcome TEXT;
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS findings_summary TEXT NOT NULL DEFAULT '';
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS detailed_findings JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS conditions JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS reviewer TEXT;
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS is_auto_approval BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE review_artifacts ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
