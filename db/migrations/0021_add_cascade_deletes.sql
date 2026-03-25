-- Add ON DELETE CASCADE to critical FK chains to prevent orphaned rows
-- when parent records are deleted.

-- objectives -> plans
ALTER TABLE plans DROP CONSTRAINT plans_objective_id_fkey;
ALTER TABLE plans ADD CONSTRAINT plans_objective_id_fkey
    FOREIGN KEY (objective_id) REFERENCES objectives(objective_id) ON DELETE CASCADE;

-- objectives -> loops
ALTER TABLE loops DROP CONSTRAINT loops_objective_id_fkey;
ALTER TABLE loops ADD CONSTRAINT loops_objective_id_fkey
    FOREIGN KEY (objective_id) REFERENCES objectives(objective_id) ON DELETE CASCADE;

-- loops -> cycles
ALTER TABLE cycles DROP CONSTRAINT cycles_loop_id_fkey;
ALTER TABLE cycles ADD CONSTRAINT cycles_loop_id_fkey
    FOREIGN KEY (loop_id) REFERENCES loops(loop_id) ON DELETE CASCADE;

-- nodes -> tasks
ALTER TABLE tasks DROP CONSTRAINT tasks_node_id_fkey;
ALTER TABLE tasks ADD CONSTRAINT tasks_node_id_fkey
    FOREIGN KEY (node_id) REFERENCES nodes(node_id) ON DELETE CASCADE;

-- nodes -> node_edges (from_node_id)
ALTER TABLE node_edges DROP CONSTRAINT node_edges_from_node_id_fkey;
ALTER TABLE node_edges ADD CONSTRAINT node_edges_from_node_id_fkey
    FOREIGN KEY (from_node_id) REFERENCES nodes(node_id) ON DELETE CASCADE;

-- nodes -> node_edges (to_node_id)
ALTER TABLE node_edges DROP CONSTRAINT node_edges_to_node_id_fkey;
ALTER TABLE node_edges ADD CONSTRAINT node_edges_to_node_id_fkey
    FOREIGN KEY (to_node_id) REFERENCES nodes(node_id) ON DELETE CASCADE;

-- tasks -> task_attempts
ALTER TABLE task_attempts DROP CONSTRAINT task_attempts_task_id_fkey;
ALTER TABLE task_attempts ADD CONSTRAINT task_attempts_task_id_fkey
    FOREIGN KEY (task_id) REFERENCES tasks(task_id) ON DELETE CASCADE;

-- tasks -> artifact_refs
ALTER TABLE artifact_refs DROP CONSTRAINT artifact_refs_task_id_fkey;
ALTER TABLE artifact_refs ADD CONSTRAINT artifact_refs_task_id_fkey
    FOREIGN KEY (task_id) REFERENCES tasks(task_id) ON DELETE CASCADE;

-- certification_candidates -> certification_submissions
ALTER TABLE certification_submissions DROP CONSTRAINT certification_submissions_candidate_id_fkey;
ALTER TABLE certification_submissions ADD CONSTRAINT certification_submissions_candidate_id_fkey
    FOREIGN KEY (candidate_id) REFERENCES certification_candidates(candidate_id) ON DELETE CASCADE;

-- certification_submissions -> certification_result_projections
ALTER TABLE certification_result_projections DROP CONSTRAINT certification_result_projections_submission_id_fkey;
ALTER TABLE certification_result_projections ADD CONSTRAINT certification_result_projections_submission_id_fkey
    FOREIGN KEY (submission_id) REFERENCES certification_submissions(submission_id) ON DELETE CASCADE;

-- certification_submissions -> stale_invalidation_records
ALTER TABLE stale_invalidation_records DROP CONSTRAINT stale_invalidation_records_submission_id_fkey;
ALTER TABLE stale_invalidation_records ADD CONSTRAINT stale_invalidation_records_submission_id_fkey
    FOREIGN KEY (submission_id) REFERENCES certification_submissions(submission_id) ON DELETE CASCADE;

-- chat_sessions -> chat_messages
ALTER TABLE chat_messages DROP CONSTRAINT chat_messages_session_id_fkey;
ALTER TABLE chat_messages ADD CONSTRAINT chat_messages_session_id_fkey
    FOREIGN KEY (session_id) REFERENCES chat_sessions(session_id) ON DELETE CASCADE;

-- chat_sessions -> conversation_extracts
ALTER TABLE conversation_extracts DROP CONSTRAINT conversation_extracts_session_id_fkey;
ALTER TABLE conversation_extracts ADD CONSTRAINT conversation_extracts_session_id_fkey
    FOREIGN KEY (session_id) REFERENCES chat_sessions(session_id) ON DELETE CASCADE;

-- conflict_records -> conflict_history
ALTER TABLE conflict_history DROP CONSTRAINT conflict_history_conflict_id_fkey;
ALTER TABLE conflict_history ADD CONSTRAINT conflict_history_conflict_id_fkey
    FOREIGN KEY (conflict_id) REFERENCES conflict_records(conflict_id) ON DELETE CASCADE;

-- conflict_records -> adjudication_tasks
ALTER TABLE adjudication_tasks DROP CONSTRAINT adjudication_tasks_conflict_id_fkey;
ALTER TABLE adjudication_tasks ADD CONSTRAINT adjudication_tasks_conflict_id_fkey
    FOREIGN KEY (conflict_id) REFERENCES conflict_records(conflict_id) ON DELETE CASCADE;

-- conflict_records -> conflict_resolutions
ALTER TABLE conflict_resolutions DROP CONSTRAINT conflict_resolutions_conflict_id_fkey;
ALTER TABLE conflict_resolutions ADD CONSTRAINT conflict_resolutions_conflict_id_fkey
    FOREIGN KEY (conflict_id) REFERENCES conflict_records(conflict_id) ON DELETE CASCADE;
