-- Migration 0002: M1 completion
-- Adds roadmap, chat/conversation, review tables; missing columns; FK indexes.

------------------------------------------------------------------------
-- Section 1: ALTER existing tables — add missing columns
------------------------------------------------------------------------

-- 1a. objectives: planning-level metadata
alter table objectives
    add column if not exists success_metric          text,
    add column if not exists architecture_summary     text,
    add column if not exists acceptance_criteria      jsonb default '[]'::jsonb,
    add column if not exists invariant_set            jsonb default '[]'::jsonb,
    add column if not exists milestone_tree_ref       text;

-- 1b. nodes: link back to the plan that spawned this node
alter table nodes
    add column if not exists plan_id text references plans(plan_id);

-- 1c. tasks: worker dispatch & safety knobs
alter table tasks
    add column if not exists provider_mode          text,
    add column if not exists model_binding          text,
    add column if not exists timeout_seconds        integer,
    add column if not exists retry_budget           integer,
    add column if not exists cautions               jsonb not null default '[]'::jsonb,
    add column if not exists auto_approval_policy   text,
    add column if not exists human_review_policy    text;

------------------------------------------------------------------------
-- Section 2: New tables — Roadmap (RMS-001 … RMS-010)
------------------------------------------------------------------------

create table if not exists roadmap_nodes (
    roadmap_node_id  text primary key,
    objective_id     text not null references objectives(objective_id),
    title            text not null,
    description      text not null default '',
    track            text not null,
    status           text not null default 'open',
    priority         integer not null default 0,
    created_at       timestamptz not null default now(),
    updated_at       timestamptz not null default now(),
    revision         integer not null default 1
);

create table if not exists roadmap_ordering (
    ordering_id    text primary key,
    objective_id   text not null references objectives(objective_id),
    node_sequence  jsonb not null default '[]'::jsonb,
    created_at     timestamptz not null default now(),
    updated_at     timestamptz not null default now()
);

create table if not exists roadmap_absorption_records (
    absorption_id    text primary key,
    roadmap_node_id  text not null references roadmap_nodes(roadmap_node_id),
    action_kind      text not null,
    source_ref       text not null,
    target_ref       text not null,
    rationale        text not null default '',
    created_at       timestamptz not null default now()
);

------------------------------------------------------------------------
-- Section 3: New tables — Chat / Conversation (CHAT-001 … CONV-010)
------------------------------------------------------------------------

create table if not exists chat_sessions (
    session_id    text primary key,
    objective_id  text references objectives(objective_id),   -- nullable
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now()
);

create table if not exists chat_messages (
    message_id  text primary key,
    session_id  text not null references chat_sessions(session_id),
    role        text not null,
    content     text not null,
    created_at  timestamptz not null default now()
);

create table if not exists conversation_extracts (
    extract_id              text primary key,
    session_id              text not null references chat_sessions(session_id),
    summarized_intent       text not null,
    extracted_constraints   jsonb not null default '[]'::jsonb,
    extracted_decisions     jsonb not null default '[]'::jsonb,
    extracted_open_questions jsonb not null default '[]'::jsonb,
    created_at              timestamptz not null default now()
);

------------------------------------------------------------------------
-- Section 4: New table — Review artifacts
------------------------------------------------------------------------

create table if not exists review_artifacts (
    review_id             text primary key,
    review_kind           text not null,
    target_ref            text not null,
    reviewer_template_id  text,
    status                text not null default 'pending',
    score_or_verdict      text,
    approval_effect       text,
    recorded_at           timestamptz not null default now()
);

------------------------------------------------------------------------
-- Section 5: Indexes on every foreign-key column (SQL-020)
--   Naming convention: idx_<table>_<column>
------------------------------------------------------------------------

-- 5a. Indexes for tables created in 0001_initial.sql

create index if not exists idx_plans_objective_id
    on plans(objective_id);

create index if not exists idx_loops_objective_id
    on loops(objective_id);

create index if not exists idx_cycles_loop_id
    on cycles(loop_id);

create index if not exists idx_nodes_objective_id
    on nodes(objective_id);

create index if not exists idx_nodes_plan_id
    on nodes(plan_id);

create index if not exists idx_node_edges_from_node_id
    on node_edges(from_node_id);

create index if not exists idx_node_edges_to_node_id
    on node_edges(to_node_id);

create index if not exists idx_tasks_node_id
    on tasks(node_id);

create index if not exists idx_task_attempts_task_id
    on task_attempts(task_id);

create index if not exists idx_conflicts_node_id
    on conflicts(node_id);

create index if not exists idx_conflict_artifacts_conflict_id
    on conflict_artifacts(conflict_id);

create index if not exists idx_claim_refs_node_id
    on claim_refs(node_id);

create index if not exists idx_argument_refs_node_id
    on argument_refs(node_id);

create index if not exists idx_artifact_refs_task_id
    on artifact_refs(task_id);

create index if not exists idx_certification_refs_node_id
    on certification_refs(node_id);

-- 5b. Indexes for tables created in this migration

create index if not exists idx_roadmap_nodes_objective_id
    on roadmap_nodes(objective_id);

create index if not exists idx_roadmap_ordering_objective_id
    on roadmap_ordering(objective_id);

create index if not exists idx_roadmap_absorption_records_roadmap_node_id
    on roadmap_absorption_records(roadmap_node_id);

create index if not exists idx_chat_sessions_objective_id
    on chat_sessions(objective_id);

create index if not exists idx_chat_messages_session_id
    on chat_messages(session_id);

create index if not exists idx_conversation_extracts_session_id
    on conversation_extracts(session_id);

-- 5c. Supplementary indexes for event_journal lookups

create index if not exists idx_event_journal_aggregate
    on event_journal(aggregate_kind, aggregate_id);
