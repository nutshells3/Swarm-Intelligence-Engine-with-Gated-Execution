-- Migration 0003: M2 planning schemas (PLAN-001 through PLAN-009)
--
-- Tables mirror the Rust structs in planning-engine/src/schemas.rs.
-- Enum-like columns use CHECK constraints so the database enforces
-- the same value sets as the Rust types.

------------------------------------------------------------------------
-- PLAN-003: milestone_trees / milestone_nodes
------------------------------------------------------------------------

create table if not exists milestone_trees (
    tree_id       text primary key,
    objective_id  text not null references objectives(objective_id),
    draft_id      text,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now()
);

create table if not exists milestone_nodes (
    milestone_id            text primary key,
    tree_id                 text not null references milestone_trees(tree_id),
    title                   text not null,
    description             text not null default '',
    parent_id               text references milestone_nodes(milestone_id),
    ordering                integer not null default 0,
    status                  text not null default 'pending'
        check (status in ('pending','in_progress','complete','blocked','cancelled')),
    acceptance_criteria_ids jsonb not null default '[]'::jsonb,
    dependency_edge_ids     jsonb not null default '[]'::jsonb,
    component_id            text
);

------------------------------------------------------------------------
-- PLAN-004: dependency_edges_planning
------------------------------------------------------------------------

create table if not exists dependency_edges_planning (
    edge_id    text primary key,
    graph_id   text not null,
    from_id    text not null,
    from_kind  text not null
        check (from_kind in ('milestone','node','roadmap_node')),
    to_id      text not null,
    to_kind    text not null
        check (to_kind in ('milestone','node','roadmap_node')),
    edge_kind  text not null
        check (edge_kind in ('blocks','should_precede','data_flow','shared_resource','roadmap_link')),
    rationale  text
);

------------------------------------------------------------------------
-- PLAN-005: acceptance_criteria
------------------------------------------------------------------------

create table if not exists acceptance_criteria (
    criterion_id          text primary key,
    owner_id              text not null,
    owner_kind            text not null
        check (owner_kind in ('plan','milestone','node')),
    description           text not null,
    verification_method   text not null
        check (verification_method in ('automated','manual_review','formal_verification','metric_threshold')),
    predicate_expression  text,
    status                text not null default 'pending'
        check (status in ('pending','evaluating','satisfied','failed','waived')),
    ordering              integer not null default 0,
    created_at            timestamptz not null default now(),
    updated_at            timestamptz not null default now()
);

------------------------------------------------------------------------
-- PLAN-006: unresolved_questions
------------------------------------------------------------------------

create table if not exists unresolved_questions (
    question_id        text primary key,
    objective_id       text not null references objectives(objective_id),
    question           text not null,
    context            text not null default '',
    severity           text not null default 'important'
        check (severity in ('blocking','important','informational')),
    resolution_status  text not null default 'open'
        check (resolution_status in ('open','tentative','resolved','dismissed')),
    resolution_answer  text,
    blocking_ids       jsonb not null default '[]'::jsonb,
    source_ref         text,
    created_at         timestamptz not null default now(),
    updated_at         timestamptz not null default now()
);

------------------------------------------------------------------------
-- PLAN-007: risk_register
------------------------------------------------------------------------

create table if not exists risk_register (
    risk_id                 text primary key,
    objective_id            text not null references objectives(objective_id),
    title                   text not null,
    description             text not null default '',
    severity                text not null default 'medium'
        check (severity in ('low','medium','high','critical')),
    likelihood              text not null default 'possible'
        check (likelihood in ('unlikely','possible','likely','almost_certain')),
    status                  text not null default 'identified'
        check (status in ('identified','mitigating','realized','closed','accepted')),
    mitigation_plan         text not null default '',
    affected_milestone_ids  jsonb not null default '[]'::jsonb,
    trigger_conditions      jsonb not null default '[]'::jsonb,
    created_at              timestamptz not null default now(),
    updated_at              timestamptz not null default now()
);

------------------------------------------------------------------------
-- PLAN-008: plan_invariants
------------------------------------------------------------------------

create table if not exists plan_invariants (
    invariant_id  text primary key,
    objective_id  text not null references objectives(objective_id),
    description   text not null,
    predicate     text not null,
    scope         text not null default 'global'
        check (scope in ('global','component','milestone','runtime')),
    enforcement   text not null default 'plan_validation'
        check (enforcement in ('plan_validation','cycle_gate','continuous','integration')),
    status        text not null default 'unchecked'
        check (status in ('unchecked','holding','violated','suspended')),
    target_id     text,
    created_at    timestamptz not null default now(),
    updated_at    timestamptz not null default now()
);

------------------------------------------------------------------------
-- PLAN-009: plan_gates
------------------------------------------------------------------------

create table if not exists plan_gates (
    gate_id                     text primary key,
    plan_id                     text not null references plans(plan_id),
    condition_entries           jsonb not null default '[]'::jsonb,
    current_status              text not null default 'open'
        check (current_status in ('open','satisfied','overridden')),
    unresolved_question_budget  integer not null default 0,
    unresolved_question_count   integer not null default 0,
    override_reason             text,
    evaluated_at                timestamptz not null default now()
);

------------------------------------------------------------------------
-- Indexes on every foreign-key column
------------------------------------------------------------------------

create index if not exists idx_milestone_trees_objective_id
    on milestone_trees(objective_id);

create index if not exists idx_milestone_nodes_tree_id
    on milestone_nodes(tree_id);

create index if not exists idx_milestone_nodes_parent_id
    on milestone_nodes(parent_id);

create index if not exists idx_dependency_edges_planning_graph_id
    on dependency_edges_planning(graph_id);

create index if not exists idx_dependency_edges_planning_from_id
    on dependency_edges_planning(from_id);

create index if not exists idx_dependency_edges_planning_to_id
    on dependency_edges_planning(to_id);

create index if not exists idx_acceptance_criteria_owner_id
    on acceptance_criteria(owner_id);

create index if not exists idx_unresolved_questions_objective_id
    on unresolved_questions(objective_id);

create index if not exists idx_risk_register_objective_id
    on risk_register(objective_id);

create index if not exists idx_plan_invariants_objective_id
    on plan_invariants(objective_id);

create index if not exists idx_plan_gates_plan_id
    on plan_gates(plan_id);
