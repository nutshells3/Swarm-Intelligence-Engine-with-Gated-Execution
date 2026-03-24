-- 0015_cnf_rev_strengthen.sql
--
-- Widen event_journal.event_kind CHECK to include new event kinds emitted by
-- the strengthened conflict detection (CNF-004 through CNF-011) and review
-- processing (REV-011 through REV-020) logic in loop-runner.
--
-- Also widen certification_submissions.queue_status to allow 'diverged' and
-- 'processing' statuses written by dual-formalization paths.
--
-- Pattern: DROP + re-ADD (idempotent via DO/EXCEPTION).

------------------------------------------------------------------------
-- 1. Widen event_journal.event_kind
------------------------------------------------------------------------
ALTER TABLE event_journal DROP CONSTRAINT IF EXISTS chk_event_journal_event_kind;

DO $$ BEGIN
    ALTER TABLE event_journal ADD CONSTRAINT chk_event_journal_event_kind
        CHECK (event_kind IN (
            -- existing
            'objective_created','objective_updated',
            'plan_created','plan_updated','plan_gate_changed',
            'loop_created','loop_cycle_advanced',
            'cycle_created','cycle_phase_transitioned','cycle_completed',
            'task_created','task_status_changed','task_attempt_started','task_attempt_finished',
            'worker_registered','worker_heartbeat_received','worker_completed',
            'certification_submitted','certification_returned','certification_completed',
            'conflict_created','conflict_resolved',
            'mainline_integration_attempted','mainline_integration_completed',
            'roadmap_node_created','roadmap_node_absorbed','roadmap_reprioritized',
            'review_artifact_created','review_completed',
            'skill_pack_registered','worker_template_created',
            'user_policy_snapshot_saved',
            -- new: conflict system (CNF-004 through CNF-011)
            'conflict_auto_resolved',
            'adjudication_task_created',
            'dual_formalization_diverged',
            -- new: review governance (REV-011 through REV-020)
            'review_auto_approved',
            'review_approved',
            'review_updated',
            -- new: drift detection
            'drift_detected',
            'objective_drift_detected',
            'dependency_drift_detected',
            -- new: retention
            'retention_policy_enforced'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 2. Widen certification_submissions.queue_status
------------------------------------------------------------------------
ALTER TABLE certification_submissions DROP CONSTRAINT IF EXISTS chk_certification_submissions_queue_status;

DO $$ BEGIN
    ALTER TABLE certification_submissions ADD CONSTRAINT chk_certification_submissions_queue_status
        CHECK (queue_status IN (
            'pending','submitted','acknowledged','completed',
            'transport_error','invalidated',
            -- new statuses from dual-formalization and timeout paths
            'processing','failed','timed_out','error','diverged'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 3. Widen artifact_refs.artifact_kind to include 'review_approval'
------------------------------------------------------------------------
ALTER TABLE artifact_refs DROP CONSTRAINT IF EXISTS chk_artifact_refs_artifact_kind;

DO $$ BEGIN
    ALTER TABLE artifact_refs ADD CONSTRAINT chk_artifact_refs_artifact_kind
        CHECK (artifact_kind IN (
            'code','test','documentation','configuration','report','mixed',
            'review_approval'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 4. Add review_satisfied column to plan_gates if not present (REV-020)
------------------------------------------------------------------------
ALTER TABLE plan_gates ADD COLUMN IF NOT EXISTS review_satisfied BOOLEAN NOT NULL DEFAULT FALSE;
