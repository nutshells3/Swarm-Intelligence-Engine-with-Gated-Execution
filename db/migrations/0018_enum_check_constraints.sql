-- 0018_enum_check_constraints.sql (renamed from 0012 to resolve numbering conflict)
-- Add CHECK constraints for all persisted enum-like TEXT columns identified
-- by the canonical enum registry (BND-004).
--
-- Pattern: idempotent DO/EXCEPTION block so re-running is safe.
-- Variant lists are taken from the authoritative Rust enum definitions
-- with #[serde(rename_all = "snake_case")].

------------------------------------------------------------------------
-- 1. EventKind  (state-model)
--    Table: event_journal.event_kind
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE event_journal ADD CONSTRAINT chk_event_journal_event_kind
        CHECK (event_kind IN (
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
            'user_policy_snapshot_saved'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 2. PlanGate  (state-model)
--    Tables: objectives.plan_gate, plans.plan_gate
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE objectives ADD CONSTRAINT chk_objectives_plan_gate
        CHECK (plan_gate IN ('draft','needs_clarification','ready_for_execution'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE plans ADD CONSTRAINT chk_plans_plan_gate
        CHECK (plan_gate IN ('draft','needs_clarification','ready_for_execution'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 3. ObjectiveStage  (planning-engine)
--    Table: objectives.planning_status
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE objectives ADD CONSTRAINT chk_objectives_planning_status
        CHECK (planning_status IN ('draft','elaborating','validated','executing','completed','abandoned'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 4. NodeLane  (state-model)
--    Tables: nodes.lane, projection_node_graph.lane, projection_branch_mainline.lane
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE nodes ADD CONSTRAINT chk_nodes_lane
        CHECK (lane IN ('branch','mainline_candidate','mainline','blocked','archived'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_node_graph ADD CONSTRAINT chk_projection_node_graph_lane
        CHECK (lane IN ('branch','mainline_candidate','mainline','blocked','archived'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_branch_mainline ADD CONSTRAINT chk_projection_branch_mainline_lane
        CHECK (lane IN ('branch','mainline_candidate','mainline','blocked','archived'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 5. NodeLifecycle  (state-model)
--    Tables: nodes.lifecycle, projection_node_graph.lifecycle,
--            projection_branch_mainline.lifecycle
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE nodes ADD CONSTRAINT chk_nodes_lifecycle
        CHECK (lifecycle IN ('proposed','queued','running','review_needed','certification_needed','admitted','blocked','superseded'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_node_graph ADD CONSTRAINT chk_projection_node_graph_lifecycle
        CHECK (lifecycle IN ('proposed','queued','running','review_needed','certification_needed','admitted','blocked','superseded'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_branch_mainline ADD CONSTRAINT chk_projection_branch_mainline_lifecycle
        CHECK (lifecycle IN ('proposed','queued','running','review_needed','certification_needed','admitted','blocked','superseded'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 6. TaskStatus  (state-model)
--    Tables: tasks.status, task_attempts.status, projection_task_board.status
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE tasks ADD CONSTRAINT chk_tasks_status
        CHECK (status IN ('queued','running','succeeded','failed','review_needed','cancelled','timed_out','archived'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE task_attempts ADD CONSTRAINT chk_task_attempts_status
        CHECK (status IN ('queued','running','succeeded','failed','review_needed','cancelled','timed_out','archived'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_task_board ADD CONSTRAINT chk_projection_task_board_status
        CHECK (status IN ('queued','running','succeeded','failed','review_needed','cancelled','timed_out','archived'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 7. CyclePhase  (state-model)
--    Tables: cycles.phase, projection_loop_history.phase
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE cycles ADD CONSTRAINT chk_cycles_phase
        CHECK (phase IN (
            'intake','conversation_extraction','plan_elaboration','plan_validation',
            'review','decomposition','dispatch','execution','integration',
            'certification_selection','certification','state_update','next_cycle_ready'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_loop_history ADD CONSTRAINT chk_projection_loop_history_phase
        CHECK (phase IN (
            'intake','conversation_extraction','plan_elaboration','plan_validation',
            'review','decomposition','dispatch','execution','integration',
            'certification_selection','certification','state_update','next_cycle_ready'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 8. ProviderMode  (worker-protocol / user-policy)
--    Tables: worker_registrations.provider_mode,
--            worker_templates.provider_mode,
--            tasks.provider_mode (nullable)
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE worker_registrations ADD CONSTRAINT chk_worker_registrations_provider_mode
        CHECK (provider_mode IN ('api','session','local'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE worker_templates ADD CONSTRAINT chk_worker_templates_provider_mode
        CHECK (provider_mode IN ('api','session','local'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE tasks ADD CONSTRAINT chk_tasks_provider_mode
        CHECK (provider_mode IS NULL OR provider_mode IN ('api','session','local'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 9. ArtifactKind  (worker-protocol)
--    Tables: artifact_refs.artifact_kind,
--            projection_artifact_timeline.artifact_kind
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE artifact_refs ADD CONSTRAINT chk_artifact_refs_artifact_kind
        CHECK (artifact_kind IN ('code','test','documentation','configuration','report','mixed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE projection_artifact_timeline ADD CONSTRAINT chk_projection_artifact_timeline_artifact_kind
        CHECK (artifact_kind IN ('code','test','documentation','configuration','report','mixed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 10. ErrorCategory  (worker-protocol)
--     Table: task_metrics.failure_category (nullable)
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE task_metrics ADD CONSTRAINT chk_task_metrics_failure_category
        CHECK (failure_category IS NULL OR failure_category IN ('transient','permanent','internal','policy_violation','timeout'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 11. PeerMessageKind  (worker-protocol)
--     Table: peer_messages.kind
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE peer_messages ADD CONSTRAINT chk_peer_messages_kind
        CHECK (kind IN (
            'request_help','share_finding','compare_result','report_blocker',
            'coordinate_resource','request_review','review_response',
            'dependency_completed','conflict_warning','agent_chat'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 12. AgentKind  (agent-adapters)
--     Table: adapter_invocations.agent_kind
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE adapter_invocations ADD CONSTRAINT chk_adapter_invocations_agent_kind
        CHECK (agent_kind IN ('codex','claude','generic_cli','http_api','local'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 13. InvocationOutcome  (agent-adapters)
--     Table: adapter_invocations.outcome
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE adapter_invocations ADD CONSTRAINT chk_adapter_invocations_outcome
        CHECK (outcome IN ('success','empty_output','timeout','failed','retried_after_empty','cancelled'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 14. AdapterHealth  (agent-adapters)
--     Table: remote_endpoints.health
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE remote_endpoints ADD CONSTRAINT chk_remote_endpoints_health
        CHECK (health IN ('healthy','degraded','unhealthy','unknown'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 15. ConflictClass  (conflict-system)
--     Table: conflict_records.conflict_class
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE conflict_records ADD CONSTRAINT chk_conflict_records_conflict_class
        CHECK (conflict_class IN ('divergence','decomposition','evidence','review_disagreement','mainline_integration'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 16. ConflictTrigger  (conflict-system)
--     Table: conflict_records.trigger
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE conflict_records ADD CONSTRAINT chk_conflict_records_trigger
        CHECK (trigger IN (
            'symbol_overlap_detected','semantic_conflict_detected',
            'duplicate_task_completion','mainline_pre_check_failed',
            'review_outcome_disagreement','decomposition_mismatch',
            'evidence_contradiction','manual_report'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 17. ConflictStatus  (conflict-system)
--     Tables: conflict_records.status, conflict_history.status_at_snapshot
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE conflict_records ADD CONSTRAINT chk_conflict_records_status
        CHECK (status IN ('open','under_adjudication','resolved','superseded','dismissed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE conflict_history ADD CONSTRAINT chk_conflict_history_status_at_snapshot
        CHECK (status_at_snapshot IN ('open','under_adjudication','resolved','superseded','dismissed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 18. AdjudicationUrgency  (conflict-system)
--     Table: adjudication_tasks.urgency
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE adjudication_tasks ADD CONSTRAINT chk_adjudication_tasks_urgency
        CHECK (urgency IN ('normal','elevated','critical'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 19. ResolutionStrategy  (conflict-system)
--     Table: conflict_resolutions.strategy
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE conflict_resolutions ADD CONSTRAINT chk_conflict_resolutions_strategy
        CHECK (strategy IN ('pick_winner','manual_merge','supersede','dismiss'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 20. CertificationEligibility  (integration)
--     Table: certification_candidates.eligibility_reason
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE certification_candidates ADD CONSTRAINT chk_certification_candidates_eligibility_reason
        CHECK (eligibility_reason IN ('downstream_dependency','contract_or_invariant','promotion_requested','conflict_adjudication'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 21. SubmissionQueueStatus  (integration)
--     Table: certification_submissions.queue_status
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE certification_submissions ADD CONSTRAINT chk_certification_submissions_queue_status
        CHECK (queue_status IN ('pending','submitted','acknowledged','completed','transport_error','invalidated'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 22. GateEffect  (integration)
--     Table: certification_result_projections.local_gate_effect
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE certification_result_projections ADD CONSTRAINT chk_cert_result_proj_local_gate_effect
        CHECK (local_gate_effect IN ('admit','block','hold','partial_admit'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 23. LaneTransition  (integration)
--     Table: certification_result_projections.lane_transition (nullable)
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE certification_result_projections ADD CONSTRAINT chk_cert_result_proj_lane_transition
        CHECK (lane_transition IS NULL OR lane_transition IN ('branch_to_mainline_candidate','mainline_candidate_to_mainline','to_blocked','no_change'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 24. ReviewKind  (review-governance)
--     Tables: review_artifacts.review_kind,
--             review_scheduling_policies.review_kind,
--             heartbeat_review_triggers.review_kind,
--             auto_approval_thresholds.review_kind
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE review_artifacts ADD CONSTRAINT chk_review_artifacts_review_kind
        CHECK (review_kind IN ('planning','architecture','direction','milestone','implementation'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE review_scheduling_policies ADD CONSTRAINT chk_review_scheduling_policies_review_kind
        CHECK (review_kind IN ('planning','architecture','direction','milestone','implementation'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE heartbeat_review_triggers ADD CONSTRAINT chk_heartbeat_review_triggers_review_kind
        CHECK (review_kind IN ('planning','architecture','direction','milestone','implementation'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE auto_approval_thresholds ADD CONSTRAINT chk_auto_approval_thresholds_review_kind
        CHECK (review_kind IN ('planning','architecture','direction','milestone','implementation'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 25. ReviewStatus  (review-governance)
--     Table: review_artifacts.status
--     Fix: migration 0002 set DEFAULT 'pending' which is not a valid
--     ReviewStatus variant.  Backfill any 'pending' rows to 'scheduled'
--     and fix the column default before adding the CHECK.
------------------------------------------------------------------------
UPDATE review_artifacts SET status = 'scheduled' WHERE status = 'pending';
ALTER TABLE review_artifacts ALTER COLUMN status SET DEFAULT 'scheduled';

DO $$ BEGIN
    ALTER TABLE review_artifacts ADD CONSTRAINT chk_review_artifacts_status
        CHECK (status IN ('scheduled','in_progress','submitted','integrated','approved','changes_requested','superseded','cancelled'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 26. ReviewOutcome  (review-governance)
--     Table: review_artifacts.outcome (nullable)
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE review_artifacts ADD CONSTRAINT chk_review_artifacts_outcome
        CHECK (outcome IS NULL OR outcome IN ('approved','approved_with_conditions','rejected','inconclusive'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 27. ReviewTriggerKind  (review-governance)
--     Table: review_scheduling_policies.trigger_kind
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE review_scheduling_policies ADD CONSTRAINT chk_review_scheduling_policies_trigger_kind
        CHECK (trigger_kind IN ('phase_transition','periodic','event_driven','manual'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 28. DeploymentMode  (deployment)
--     Table: deployment_policies.deployment_mode
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE deployment_policies ADD CONSTRAINT chk_deployment_policies_deployment_mode
        CHECK (deployment_mode IN ('local_only','local_plus_remote','remote_certification_preferred','certification_disabled'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 29. WorkerState  (control-plane)
--     Table: worker_registrations.state
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE worker_registrations ADD CONSTRAINT chk_worker_registrations_state
        CHECK (state IN ('registered','idle','running','draining','lease_expired','stuck','cancelled','killed','deregistered'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 30. RoadmapNode status  (roadmap-model -- no typed enum, uses string)
--     Table: roadmap_nodes.status
--     Values observed in code: 'open' (default), 'deferred', 'rejected',
--     plus 'absorbed' and 'completed' as logical final states.
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE roadmap_nodes ADD CONSTRAINT chk_roadmap_nodes_status
        CHECK (status IN ('open','deferred','rejected','absorbed','completed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- 31. Legacy conflicts table (0001) -- conflict_kind and status
--     Table: conflicts.conflict_kind
--     Values observed: 'divergence', 'mainline_integration_conflict',
--     'formalization_divergence' (written by loop-runner / worker-dispatch).
--     Also include the canonical ConflictClass values for forward-compat.
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE conflicts ADD CONSTRAINT chk_conflicts_conflict_kind
        CHECK (conflict_kind IN (
            'divergence','decomposition','evidence','review_disagreement',
            'mainline_integration','mainline_integration_conflict','formalization_divergence'
        ));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    ALTER TABLE conflicts ADD CONSTRAINT chk_conflicts_status
        CHECK (status IN ('open','under_adjudication','resolved','superseded','dismissed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- Done. All 25+ persisted enum columns now have CHECK constraints.
------------------------------------------------------------------------
