-- 0015_cnf_rev_strengthen.sql
--
-- Widen event_journal.event_kind CHECK to include ALL event kinds emitted by
-- the loop-runner (tick.rs, planning.rs, projections.rs, recursive_improvement.rs),
-- control-plane executor, and worker-dispatch.
--
-- Also widen certification_submissions.queue_status to allow 'diverged',
-- 'processing', and 'blocked' statuses written by dual-formalization paths.
--
-- Pattern: DROP + re-ADD (idempotent via DO/EXCEPTION).

------------------------------------------------------------------------
-- 1. Widen event_journal.event_kind
------------------------------------------------------------------------
ALTER TABLE event_journal DROP CONSTRAINT IF EXISTS chk_event_journal_event_kind;

DO $$ BEGIN
    ALTER TABLE event_journal ADD CONSTRAINT chk_event_journal_event_kind
        CHECK (event_kind IN (
            -- core objective/plan lifecycle
            'objective_created','objective_updated',
            'plan_created','plan_updated','plan_gate_changed',
            'plan_artifacts_generated','plan_decomposed',
            -- loop / cycle lifecycle
            'loop_created','loop_cycle_advanced',
            'cycle_created','cycle_phase_transitioned','cycle_completed',
            -- task lifecycle
            'task_created','task_status_changed','task_attempt_started','task_attempt_finished',
            'task_completed','task_failed','task_timed_out','task_retry_scheduled',
            -- worker lifecycle
            'worker_registered','worker_heartbeat_received','worker_completed',
            -- certification pipeline
            'certification_submitted','certification_returned','certification_completed',
            'certification_candidate_created',
            -- conflict system (CNF-004 through CNF-011)
            'conflict_created','conflict_resolved',
            'conflict_auto_resolved',
            'adjudication_task_created',
            'dual_formalization_diverged',
            -- integration
            'mainline_integration_attempted','mainline_integration_completed',
            -- roadmap
            'roadmap_node_created','roadmap_node_absorbed','roadmap_reprioritized',
            -- review governance (REV-011 through REV-020)
            'review_artifact_created','review_completed',
            'review_auto_approved',
            'review_approved',
            'review_updated',
            -- skill / worker templates
            'skill_pack_registered','worker_template_created',
            -- policy
            'user_policy_snapshot_saved',
            -- planning pipeline (loop-runner tick)
            'plan_gate_evaluated',
            'plan_gate_forced_override',
            -- dispatch / execution
            'execution_completed',
            'dispatch_round_completed',
            -- decomposition / milestone bridge
            'integration_verify_node_created',
            'milestone_bridged',
            -- conversation extraction
            'extract_processed',
            -- drift detection
            'drift_detected',
            'objective_drift_detected',
            'dependency_drift_detected',
            -- integration verification
            'integration_verification_failed',
            -- observability: heartbeat / phase sidecar / projection
            'tick_heartbeat',
            'phase_status_recorded',
            'projection_snapshot',
            -- retention
            'retention_policy_enforced',
            -- recursive improvement
            'comparison_artifact_created',
            'loop_score_created',
            'milestone_templates_created',
            'drift_check_completed',
            'self_promotion_blocked',
            'recursive_report_generated',
            'success_pattern_recorded',
            'roadmap_suggestion_recorded'
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
            'processing','failed','timed_out','error','diverged','blocked'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 3. Widen artifact_refs.artifact_kind to include 'review_approval'
--    and other kinds used by the codebase
------------------------------------------------------------------------
ALTER TABLE artifact_refs DROP CONSTRAINT IF EXISTS chk_artifact_refs_artifact_kind;

DO $$ BEGIN
    ALTER TABLE artifact_refs ADD CONSTRAINT chk_artifact_refs_artifact_kind
        CHECK (artifact_kind IN (
            'code','test','documentation','configuration','report','mixed',
            'review_approval','adapter_output','source_file','source_anchor','output_file'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 4. Add review_satisfied column to plan_gates if not present (REV-020)
------------------------------------------------------------------------
ALTER TABLE plan_gates ADD COLUMN IF NOT EXISTS review_satisfied BOOLEAN NOT NULL DEFAULT FALSE;
