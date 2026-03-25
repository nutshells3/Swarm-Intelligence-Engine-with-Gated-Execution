-- 0020_bundle03_check_constraint_fixes.sql
--
-- Fix CHECK constraint violations discovered during bundle-03 execution/
-- integration audit. The codebase writes status values and artifact kinds
-- that the CHECK constraints in 0018/0019 do not permit.
--
-- Fixes:
--   WRK-014: tasks.status and task_attempts.status must allow
--            'failed_permanent' and 'failed_retryable'.
--   WRK-015: tasks.status must NOT allow re-dispatch of terminal states;
--            the new constraint makes these states explicit.
--   WRK-011: artifact_refs.artifact_kind must allow 'stdout_capture'
--            and 'stderr_capture'.
--   WRK-012: artifact_refs.artifact_kind must allow 'integration_verify_result'.
--   FCG-*:   artifact_refs.artifact_kind must allow 'trust_surface',
--            'audit_probe', 'verification_result'.
--   OBS-009: task_metrics.failure_category must allow 'retryable_backend'.
--   Nodes:   nodes.lifecycle must allow 'completed' and 'done'
--            (written by worker-dispatch and queried in dep-check).
--
-- Pattern: DROP IF EXISTS + ADD so re-running is safe.

------------------------------------------------------------------------
-- 1. tasks.status -- add failed_permanent and failed_retryable
------------------------------------------------------------------------
ALTER TABLE tasks DROP CONSTRAINT IF EXISTS chk_tasks_status;
ALTER TABLE tasks ADD CONSTRAINT chk_tasks_status
    CHECK (status IN (
        'queued','running','succeeded','failed',
        'failed_permanent','failed_retryable',
        'review_needed','cancelled','timed_out','archived'
    ));

------------------------------------------------------------------------
-- 2. task_attempts.status -- same as tasks
------------------------------------------------------------------------
ALTER TABLE task_attempts DROP CONSTRAINT IF EXISTS chk_task_attempts_status;
ALTER TABLE task_attempts ADD CONSTRAINT chk_task_attempts_status
    CHECK (status IN (
        'queued','running','succeeded','failed',
        'failed_permanent','failed_retryable',
        'review_needed','cancelled','timed_out','archived'
    ));

------------------------------------------------------------------------
-- 3. projection_task_board.status -- same as tasks
------------------------------------------------------------------------
ALTER TABLE projection_task_board DROP CONSTRAINT IF EXISTS chk_projection_task_board_status;
ALTER TABLE projection_task_board ADD CONSTRAINT chk_projection_task_board_status
    CHECK (status IN (
        'queued','running','succeeded','failed',
        'failed_permanent','failed_retryable',
        'review_needed','cancelled','timed_out','archived'
    ));

------------------------------------------------------------------------
-- 4. nodes.lifecycle -- add 'completed' and 'done'
--    worker-dispatch writes 'completed'; dependency queries check 'done'.
------------------------------------------------------------------------
ALTER TABLE nodes DROP CONSTRAINT IF EXISTS chk_nodes_lifecycle;
ALTER TABLE nodes ADD CONSTRAINT chk_nodes_lifecycle
    CHECK (lifecycle IN (
        'proposed','queued','running',
        'review_needed','certification_needed',
        'admitted','completed','done',
        'blocked','superseded'
    ));

------------------------------------------------------------------------
-- 5. projection_node_graph.lifecycle -- same as nodes
------------------------------------------------------------------------
ALTER TABLE projection_node_graph DROP CONSTRAINT IF EXISTS chk_projection_node_graph_lifecycle;
ALTER TABLE projection_node_graph ADD CONSTRAINT chk_projection_node_graph_lifecycle
    CHECK (lifecycle IN (
        'proposed','queued','running',
        'review_needed','certification_needed',
        'admitted','completed','done',
        'blocked','superseded'
    ));

------------------------------------------------------------------------
-- 6. projection_branch_mainline.lifecycle -- same as nodes
------------------------------------------------------------------------
ALTER TABLE projection_branch_mainline DROP CONSTRAINT IF EXISTS chk_projection_branch_mainline_lifecycle;
ALTER TABLE projection_branch_mainline ADD CONSTRAINT chk_projection_branch_mainline_lifecycle
    CHECK (lifecycle IN (
        'proposed','queued','running',
        'review_needed','certification_needed',
        'admitted','completed','done',
        'blocked','superseded'
    ));

------------------------------------------------------------------------
-- 7. artifact_refs.artifact_kind -- add all runtime artifact kinds
------------------------------------------------------------------------
ALTER TABLE artifact_refs DROP CONSTRAINT IF EXISTS chk_artifact_refs_artifact_kind;
ALTER TABLE artifact_refs ADD CONSTRAINT chk_artifact_refs_artifact_kind
    CHECK (artifact_kind IN (
        -- original typed enum values
        'code','test','documentation','configuration','report','mixed',
        -- worker-dispatch adapter output and I/O captures (WRK-011)
        'adapter_output','stdout_capture','stderr_capture',
        -- git diffs and symbol diffs (GIT-005, ROB-016)
        'git_diff','symbol_diff',
        -- certification pipeline artifacts (FCG-*)
        'trust_surface','audit_probe','verification_result',
        -- integration verification (GIT-007/008)
        'integration_verify_result',
        -- review artifacts
        'review_approval',
        -- source anchors / files
        'source_file','source_anchor','output_file'
    ));

------------------------------------------------------------------------
-- 8. projection_artifact_timeline.artifact_kind -- same as artifact_refs
------------------------------------------------------------------------
ALTER TABLE projection_artifact_timeline DROP CONSTRAINT IF EXISTS chk_projection_artifact_timeline_artifact_kind;
ALTER TABLE projection_artifact_timeline ADD CONSTRAINT chk_projection_artifact_timeline_artifact_kind
    CHECK (artifact_kind IN (
        'code','test','documentation','configuration','report','mixed',
        'adapter_output','stdout_capture','stderr_capture',
        'git_diff','symbol_diff',
        'trust_surface','audit_probe','verification_result',
        'integration_verify_result',
        'review_approval',
        'source_file','source_anchor','output_file'
    ));

------------------------------------------------------------------------
-- 9. task_metrics.failure_category -- add 'retryable_backend' (OBS-009)
------------------------------------------------------------------------
ALTER TABLE task_metrics DROP CONSTRAINT IF EXISTS chk_task_metrics_failure_category;
ALTER TABLE task_metrics ADD CONSTRAINT chk_task_metrics_failure_category
    CHECK (failure_category IS NULL OR failure_category IN (
        'transient','permanent','internal','policy_violation','timeout',
        'retryable_backend'
    ));

------------------------------------------------------------------------
-- Done. All CHECK constraints now align with bundle-03 runtime code.
------------------------------------------------------------------------
