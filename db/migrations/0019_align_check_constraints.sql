-- 0019_align_check_constraints.sql
--
-- Fix CHECK constraint drift discovered during end-to-end runtime testing.
-- The original 0018_enum_check_constraints.sql defined CHECK constraints
-- based on the canonical Rust enum definitions, but the actual codebase
-- emits many additional event_kind values and artifact_kind values via
-- raw SQL INSERT statements (not through the typed EventKind enum).
--
-- This migration:
--   1. DROPs each outdated CHECK constraint
--   2. RE-CREATEs it with the complete set of values from ALL Rust code paths
--   3. Adds missing artifact_kind values (git_diff, adapter_output, etc.)
--   4. Refreshes conflicts.conflict_kind to include all runtime values
--
-- Pattern: DROP IF EXISTS + ADD so re-running is safe.
-- Closes: SQL-001 through SQL-019, RMS-003 through RMS-010

------------------------------------------------------------------------
-- 1. EventKind  (event_journal.event_kind)
--
-- The original CHECK had 30 values from the Rust EventKind enum.
-- Runtime code emits 80+ distinct event_kind strings across:
--   - loop-runner/tick.rs (tick_heartbeat, plan_gate_evaluated, etc.)
--   - loop-runner/planning.rs (plan_artifacts_generated, plan_decomposed)
--   - loop-runner/projections.rs (projection_snapshot)
--   - loop-runner/recursive_improvement.rs (comparison_artifact_created, etc.)
--   - worker-dispatch/main.rs (worktree_bound, task_retry_scheduled, etc.)
--   - orchestration-api routes (chat_*, review_*, roadmap_*, etc.)
------------------------------------------------------------------------
ALTER TABLE event_journal DROP CONSTRAINT IF EXISTS chk_event_journal_event_kind;
ALTER TABLE event_journal ADD CONSTRAINT chk_event_journal_event_kind
    CHECK (event_kind IN (
        -- ── state-model EventKind enum (authoritative) ──
        'objective_created','objective_updated',
        'plan_created','plan_updated','plan_gate_changed',
        'loop_created','loop_cycle_advanced',
        'cycle_created','cycle_phase_transitioned','cycle_completed',
        'task_created','task_status_changed','task_attempt_started','task_attempt_finished',
        'worker_registered','worker_heartbeat_received','worker_completed',
        'certification_submitted','certification_returned',
        'conflict_created','conflict_resolved',
        'mainline_integration_attempted','mainline_integration_completed',
        'roadmap_node_created','roadmap_node_absorbed','roadmap_reprioritized',
        'review_artifact_created','review_completed',
        'skill_pack_registered','worker_template_created',
        'user_policy_snapshot_saved',

        -- ── loop-runner/tick.rs runtime events ──
        'tick_heartbeat',
        'plan_gate_evaluated','plan_gate_forced_override',
        'integration_verify_node_created',
        'milestone_bridged',
        'execution_completed',
        'integration_verification_failed',
        'extract_processed',
        'conflict_auto_resolved',
        'adjudication_task_created',
        'drift_detected','objective_drift_detected','dependency_drift_detected',
        'retention_policy_enforced',
        'review_auto_approved',
        'phase_status_recorded',
        'dual_formalization_diverged',
        'certification_completed',

        -- ── loop-runner/planning.rs events ──
        'plan_artifacts_generated','plan_decomposed',

        -- ── loop-runner/projections.rs events ──
        'projection_snapshot',

        -- ── loop-runner/recursive_improvement.rs events ──
        'comparison_artifact_created','loop_score_created',
        'milestone_templates_created','drift_check_completed',
        'self_promotion_blocked','recursive_report_generated',
        'success_pattern_recorded','roadmap_suggestion_recorded',

        -- ── worker-dispatch/main.rs events ──
        'worktree_bound','worktree_released',
        'worker_status_heartbeat',
        'task_retry_scheduled',
        'file_conflict_detected','merge_conflict_detected',
        'dirty_worktree_detected',
        'certification_candidate_created',
        'review_needed',
        'integration_verification_complete',

        -- ── orchestration-api: chat routes ──
        'chat_session_created','chat_session_linked_to_objective',
        'constraints_extracted','chat_message_added',
        'conversation_extracted','backlog_draft_created',
        'plan_updated_from_extract',

        -- ── orchestration-api: task lifecycle routes ──
        'task_completed','task_failed','task_attempt_completed',

        -- ── orchestration-api: review routes ──
        'review_created','review_updated','review_approved',

        -- ── orchestration-api: certification routes ──
        'certification_config_updated',

        -- ── orchestration-api: deployment routes ──
        'deployment_mode_changed',

        -- ── orchestration-api: policy routes ──
        'certification_settings_updated',

        -- ── orchestration-api: roadmap routes ──
        'roadmap_node_deferred','roadmap_node_rejected',
        'roadmap_absorbed','roadmap_reordered','roadmap_track_changed',

        -- ── orchestration-api: peer routes ──
        'peer_message_sent','peer_message_acknowledged',

        -- ── orchestration-api: node routes ──
        'node_edge_created'
    ));

------------------------------------------------------------------------
-- 2. ArtifactKind  (artifact_refs, projection_artifact_timeline)
--
-- Original CHECK: code, test, documentation, configuration, report, mixed
-- Runtime code also inserts:
--   - 'adapter_output'  (worker-dispatch line 674)
--   - 'git_diff'        (worker-dispatch line 966)
--   - 'review_approval' (tick.rs line 2168, reviews.rs line 371)
--   - 'symbol_diff'     (referenced in worker-dispatch comments/queries)
------------------------------------------------------------------------
ALTER TABLE artifact_refs DROP CONSTRAINT IF EXISTS chk_artifact_refs_artifact_kind;
ALTER TABLE artifact_refs ADD CONSTRAINT chk_artifact_refs_artifact_kind
    CHECK (artifact_kind IN (
        'code','test','documentation','configuration','report','mixed',
        'symbol_diff','git_diff','adapter_output','review_approval'
    ));

ALTER TABLE projection_artifact_timeline DROP CONSTRAINT IF EXISTS chk_projection_artifact_timeline_artifact_kind;
ALTER TABLE projection_artifact_timeline ADD CONSTRAINT chk_projection_artifact_timeline_artifact_kind
    CHECK (artifact_kind IN (
        'code','test','documentation','configuration','report','mixed',
        'symbol_diff','git_diff','adapter_output','review_approval'
    ));

------------------------------------------------------------------------
-- 3. SubmissionQueueStatus  (certification_submissions.queue_status)
--
-- Original CHECK: pending, submitted, acknowledged, completed,
--   transport_error, invalidated
-- Runtime code also sets: error, processing, timed_out, diverged, blocked
-- (tick.rs certification processing loop)
------------------------------------------------------------------------
ALTER TABLE certification_submissions DROP CONSTRAINT IF EXISTS chk_certification_submissions_queue_status;
ALTER TABLE certification_submissions ADD CONSTRAINT chk_certification_submissions_queue_status
    CHECK (queue_status IN (
        'pending','submitted','acknowledged','completed',
        'transport_error','invalidated',
        'error','processing','timed_out','diverged','blocked'
    ));

------------------------------------------------------------------------
-- 4. Conflicts table conflict_kind  (conflicts.conflict_kind)
--
-- Refresh to ensure all runtime values are present.
-- tick.rs inserts: 'divergence', 'review_disagreement',
--                  'formalization_divergence'
-- worker-dispatch inserts: 'mainline_integration'
-- Original 0018 also included: 'decomposition', 'evidence',
--   'mainline_integration_conflict'
------------------------------------------------------------------------
ALTER TABLE conflicts DROP CONSTRAINT IF EXISTS chk_conflicts_conflict_kind;
ALTER TABLE conflicts ADD CONSTRAINT chk_conflicts_conflict_kind
    CHECK (conflict_kind IN (
        'divergence','decomposition','evidence','review_disagreement',
        'mainline_integration','mainline_integration_conflict',
        'formalization_divergence'
    ));

------------------------------------------------------------------------
-- 5. Nodes lane  (nodes.lane, projection_node_graph.lane,
--                  projection_branch_mainline.lane)
--
-- Original CHECK: branch, mainline_candidate, mainline, blocked, archived
-- Runtime code inserts:
--   - 'integration'     (tick.rs:805 — integration verification node)
--   - 'implementation'  (tick.rs:963, planning.rs:1104 default)
--   - 'planning'        (decomposition planning nodes)
--   - 'review'          (review nodes)
--   - 'verification'    (verification nodes)
------------------------------------------------------------------------
ALTER TABLE nodes DROP CONSTRAINT IF EXISTS chk_nodes_lane;
ALTER TABLE nodes ADD CONSTRAINT chk_nodes_lane CHECK (lane IN (
    'branch','mainline_candidate','mainline','blocked','archived',
    'integration','implementation','planning','review','verification'
));

ALTER TABLE projection_node_graph DROP CONSTRAINT IF EXISTS chk_projection_node_graph_lane;
ALTER TABLE projection_node_graph ADD CONSTRAINT chk_projection_node_graph_lane CHECK (lane IN (
    'branch','mainline_candidate','mainline','blocked','archived',
    'integration','implementation','planning','review','verification'
));

ALTER TABLE projection_branch_mainline DROP CONSTRAINT IF EXISTS chk_projection_branch_mainline_lane;
ALTER TABLE projection_branch_mainline ADD CONSTRAINT chk_projection_branch_mainline_lane CHECK (lane IN (
    'branch','mainline_candidate','mainline','blocked','archived',
    'integration','implementation','planning','review','verification'
));

------------------------------------------------------------------------
-- Done. All CHECK constraints now align with runtime code paths.
------------------------------------------------------------------------
