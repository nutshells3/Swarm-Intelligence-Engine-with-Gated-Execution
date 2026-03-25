# Core concepts

SIEGE has a lot of moving parts. The easiest way to read the repo is to keep the core nouns straight.

## Objective

An **objective** is the user-facing top-level goal.

Examples:

- “Build a REST API for user authentication.”
- “Refactor the caching layer and add integration tests.”
- “Decompose this domain document into implementation milestones.”

The objective is not yet executable work. It is the seed for conversation extraction, planning, gating, decomposition, and later cycles.

## Conversation extraction

SIEGE can derive structure from chat before planning begins.

That extraction is meant to pull out things such as:

- constraints,
- decisions,
- unresolved questions,
- desired outcomes,
- acceptance signals.

This matters because “the prompt” is not treated as a flat blob. It becomes durable planning input.

## Plan

A **plan** is the structured artifact created before implementation starts.

It can include:

- architecture drafts,
- milestone trees,
- acceptance criteria,
- dependency edges,
- invariants,
- risk records,
- gate condition entries.

In SIEGE, the plan is not just a nice explanation for humans. It is the thing the engine evaluates before work is allowed to fan out.

## Gate

A **gate** is a typed readiness checkpoint.

The main planning gate is a **10-condition gate**. If the gate is open, SIEGE can keep elaborating the plan, collecting questions, or routing for review — but it should not pretend implementation is ready.

## Milestone tree

A **milestone tree** is the plan’s structural decomposition of the objective.

Milestones are not yet dispatched work. They are the planning-level skeleton that later becomes nodes and tasks.

## Dependency graph

The **dependency graph** captures precedence and structural relationships.

This is what separates SIEGE from flat queue systems. The engine is designed to know that some work blocks other work, some work can run in parallel, and some work should be held until upstream assumptions settle.

## Node

A **node** is a graph element in the decomposed execution model.

A node is conceptually “a unit of planned work in the objective graph.” It lives at the orchestration layer and is richer than a raw task queue item.

## Task

A **task** is the dispatchable unit created from a node.

A node expresses where work sits in the graph.
A task expresses how the engine will ask a worker role or provider to attempt that work.

A node can generate tasks across cycles or retries. A task can have multiple attempts.

## Task attempt

A **task attempt** is one concrete execution run for a task.

This is where adapters, retries, outputs, failures, and integration consequences show up.

## Loop and cycle

A **loop** ties an objective to repeated orchestration passes.
A **cycle** is one iteration of that loop.

The point is not just “run agents once.” The point is “plan, execute, inspect, learn, and continue with memory.”

## Worktree

A **worktree** is the isolated git workspace assigned to a task or worker run.

This keeps concurrent edits from piling into the same branch blindly. It gives the engine a safer way to execute in parallel and integrate later.

## Policy snapshot

A **policy snapshot** records runtime execution settings such as:

- provider mode,
- model binding,
- concurrency ceilings,
- retry budgets,
- certification routing,
- formalizer configuration.

In SIEGE, these are policy concerns, not prompt accidents.

## Review

A **review** is a first-class artifact.

Reviews are not just comments buried in logs. They have kinds, templates, outcomes, scheduling rules, and downstream effects.

## Conflict

A **conflict** is also first-class.

SIEGE models conflicts explicitly instead of silently overwriting one branch with another. Conflict history matters for routing, adjudication, and auditability.

## Certification / formal assurance

A **certification** is a stronger assurance lane for selected artifacts or claims.

SIEGE’s formal-assurance surfaces are not limited to end-result validation. Planning artifacts themselves can also be exported and checked through formal-readiness and formal-claim pathways.

## Projection

A **projection** is a read model derived from authoritative state and events.

The dashboard, API summaries, and operator views should consume projections rather than ad hoc joins scattered everywhere.

## Skill pack

A **skill pack** is a reusable capability bundle resolved by the skill registry.

This gives worker roles more structure than “one giant system prompt.”

## Quick mental model

If you want the shortest possible summary:

- the **objective** says what you want,
- the **plan** says how it should be structured,
- the **gate** decides whether execution is allowed,
- the **graph** says what depends on what,
- the **node** is the orchestration unit,
- the **task** is the dispatch unit,
- the **attempt** is the concrete run,
- the **review / conflict / certification** artifacts decide whether results can safely advance.
