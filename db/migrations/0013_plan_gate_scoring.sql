-- Migration 0013: PLAN-018/PLAN-019 -- add completeness_score and failure_reasons
-- to plan_gates so the full CompletenessScore and ValidationFailure vector are
-- persisted alongside the gate evaluation.

ALTER TABLE plan_gates
    ADD COLUMN IF NOT EXISTS completeness_score jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS failure_reasons    jsonb NOT NULL DEFAULT '[]'::jsonb;
