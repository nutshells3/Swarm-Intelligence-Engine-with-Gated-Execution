# Gated execution and planning

Gated execution is the central design choice in SIEGE.

Most multi-agent systems ask for a plan and then start running anyway.
SIEGE treats that as a category error.

A weak plan in a parallel system is not a small problem. It is a force multiplier for failure: overlapping edits, hidden dependency cycles, vague acceptance criteria, integration churn, retry storms, and wasted model spend.

So SIEGE turns planning into a hard runtime contract.

## Planning flow

The planning path is designed to move through these stages:

```text
objective
  -> conversation extraction
  -> plan elaboration
  -> plan validation
  -> 10-condition gate evaluation
  -> optional planning review / formal-assurance path
  -> decomposition into node graph
  -> execution
```

The gate sits in the middle. If the plan is not ready, the engine should keep improving the plan instead of pretending implementation can safely begin.

## What the planning layer produces

A mature plan in SIEGE is more than a paragraph of prose.

The planning layer is expected to surface artifacts such as:

- an objective summary,
- architecture drafts,
- a milestone tree,
- acceptance criteria,
- dependency edges,
- plan invariants,
- risk records,
- unresolved questions,
- machine-readable gate condition entries.

These artifacts are important because later phases depend on them directly.

## The 10 gate conditions

The planning gate evaluates ten conditions:

1. **objective summarized** — the objective has a usable summary, desired outcome, and success framing.
2. **architecture drafted** — at least one acceptable architecture draft exists.
3. **milestone tree created** — the objective has been decomposed into milestones.
4. **acceptance criteria defined** — milestones are not vague promises; they have completion conditions.
5. **dependencies acyclic** — blocking dependencies do not form a cycle.
6. **dependencies resolved** — dependency edges point to real entities.
7. **invariants extracted** — the plan states what must remain true.
8. **invariants holding** — invariants with planning-time enforcement are currently satisfied.
9. **risks identified** — the plan acknowledges meaningful execution risks.
10. **unresolved questions below budget** — blocking uncertainty is within the allowed budget.

That last condition is important. SIEGE is allowed to proceed with some uncertainty, but not with unbounded ambiguity.

## What happens when the gate is open

An open gate is not a crash.

It means the engine should keep doing planning work such as:

- elaborating architecture,
- refining milestones,
- clarifying open questions,
- extracting or validating invariants,
- tightening dependency structure,
- routing for review,
- routing for formal planning validation where appropriate.

The point is that “not ready” becomes a visible state, not a hidden failure.

## What happens when the gate is satisfied

Once the gate is satisfied, the plan is considered strong enough to decompose.

That means SIEGE can:

- convert planned structure into node graphs,
- create dispatchable tasks,
- assign roles and providers,
- fan work out into isolated worktrees,
- treat later failures as execution or integration issues rather than basic planning negligence.

## Planning invariants matter

A lot of systems talk about requirements, but SIEGE also cares about invariants.

That matters because invariants anchor the execution model. They describe what must continue to hold while work is decomposed, parallelized, and merged.

Examples:

- a public API contract must remain backward compatible,
- a migration plan must remain reversible,
- a safety-critical rule must not be weakened by refactoring,
- a claim decomposition must preserve source mapping.

## Planning can now enter formal-assurance lanes

SIEGE’s formal-assurance story is not limited to validating final outputs.

Planning artifacts themselves can be exported and checked.

This is a meaningful step up from “write code, then certify the result.” It enables a stronger workflow:

```text
objective
  -> plan elaboration
  -> 10-condition gate
  -> formal-readiness export for the plan
  -> formal-claim validation of planning artifacts
  -> decomposition / execution
```

In practice this means the engine can validate more than just implementation results. It can also validate whether the structure of the plan is coherent enough to deserve promotion into execution.

## Plan-to-formal export

The `formal-readiness` layer includes a plan-to-formal export bridge.

That bridge is designed to export planning artifacts in a backend-neutral shape, including:

- plan invariants as predicates,
- dependency graphs as graph facts,
- gate status as lifecycle facts,
- deterministic checksums for reproducibility.

This is intentionally structural. It is not a Lean-specific or Isabelle-specific document. It is a neutral export surface that later formal tooling can consume.

## Readiness before formal export

SIEGE also distinguishes “plan exists” from “plan is ready for formal export.”

A lightweight readiness check can verify things such as:

- at least one invariant exists,
- dependency structure exists,
- gate conditions are defined.

That prevents formal tooling from being called on empty planning shells.

## Why all this matters

The easiest way to understand gated execution is this:

SIEGE is trying to move failure left.

Instead of waiting for parallel execution to expose a bad plan in the most expensive possible way, it tries to surface structural weakness while the engine still has the option to repair the plan.

That is the difference between “planning as narration” and “planning as an executable contract.”
