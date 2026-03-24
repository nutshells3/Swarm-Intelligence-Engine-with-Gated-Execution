create table if not exists objectives (
    objective_id text primary key,
    summary text not null,
    planning_status text not null,
    plan_gate text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists plans (
    plan_id text primary key,
    objective_id text not null references objectives(objective_id),
    architecture_summary text not null,
    milestone_tree_ref text not null,
    unresolved_questions integer not null default 0,
    plan_gate text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists loops (
    loop_id text primary key,
    objective_id text not null references objectives(objective_id),
    cycle_index integer not null default 0,
    active_track text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists cycles (
    cycle_id text primary key,
    loop_id text not null references loops(loop_id),
    phase text not null,
    policy_snapshot jsonb not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists nodes (
    node_id text primary key,
    objective_id text not null references objectives(objective_id),
    title text not null,
    statement text not null,
    lane text not null,
    lifecycle text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists node_edges (
    edge_id text primary key,
    from_node_id text not null references nodes(node_id),
    to_node_id text not null references nodes(node_id),
    edge_kind text not null
);

create table if not exists tasks (
    task_id text primary key,
    node_id text not null references nodes(node_id),
    worker_role text not null,
    skill_pack_id text not null,
    status text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists task_attempts (
    task_attempt_id text primary key,
    task_id text not null references tasks(task_id),
    attempt_index integer not null,
    lease_owner text,
    status text not null,
    started_at timestamptz,
    finished_at timestamptz,
    unique (task_id, attempt_index)
);

create table if not exists conflicts (
    conflict_id text primary key,
    node_id text not null references nodes(node_id),
    conflict_kind text not null,
    status text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists conflict_artifacts (
    conflict_artifact_id text primary key,
    conflict_id text not null references conflicts(conflict_id),
    artifact_ref text not null,
    artifact_role text not null
);

create table if not exists claim_refs (
    claim_ref_id text primary key,
    node_id text not null references nodes(node_id),
    local_summary text not null,
    source_anchors jsonb not null default '[]'::jsonb,
    canonical_external_ref text,
    last_certification_status text
);

create table if not exists argument_refs (
    argument_ref_id text primary key,
    node_id text not null references nodes(node_id),
    supporting_refs jsonb not null default '[]'::jsonb,
    opposing_refs jsonb not null default '[]'::jsonb,
    dependency_edges jsonb not null default '[]'::jsonb,
    last_review_status text
);

create table if not exists artifact_refs (
    artifact_ref_id text primary key,
    task_id text references tasks(task_id),
    artifact_kind text not null,
    artifact_uri text not null,
    metadata jsonb not null default '{}'::jsonb
);

create table if not exists certification_refs (
    certification_ref_id text primary key,
    node_id text not null references nodes(node_id),
    external_system text not null,
    external_ref text not null,
    gate text not null,
    status text not null,
    metadata jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

create table if not exists user_policies (
    policy_id text primary key,
    policy_payload jsonb not null,
    created_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists worker_templates (
    template_id text primary key,
    role text not null,
    skill_pack_id text not null,
    provider_mode text not null,
    model_binding text not null,
    allowed_task_kinds jsonb not null default '[]'::jsonb,
    created_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists skill_packs (
    skill_pack_id text primary key,
    worker_role text not null,
    description text not null,
    accepted_task_kinds jsonb not null default '[]'::jsonb,
    "references" jsonb not null default '[]'::jsonb,
    scripts jsonb not null default '[]'::jsonb,
    created_at timestamptz not null default now(),
    revision integer not null default 1
);

create table if not exists event_journal (
    event_id text primary key,
    aggregate_kind text not null,
    aggregate_id text not null,
    event_kind text not null,
    idempotency_key text not null,
    payload jsonb not null,
    created_at timestamptz not null default now(),
    unique (aggregate_kind, aggregate_id, idempotency_key)
);
