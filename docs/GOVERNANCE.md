# Governance: reviews, conflicts, promotion, and failure discipline

SIEGE treats governance as part of runtime execution, not as paperwork added after the interesting work is done.

That is one of the biggest differences between SIEGE and lighter agent orchestration stacks.

## Why governance is a runtime concern

Parallel execution is powerful, but it introduces new failure modes:

- two workers produce incompatible implementations,
- decomposition itself forks in contradictory ways,
- evidence conflicts with evidence,
- reviews disagree,
- integration with mainline becomes unsafe,
- a self-improving loop starts rewarding itself for the wrong reasons.

If those issues are only visible in logs, the system becomes fast but untrustworthy.

SIEGE makes them explicit.

## Reviews are first-class artifacts

The review subsystem is built around explicit review kinds and templates.

Canonical review kinds include:

- planning,
- architecture,
- direction,
- milestone,
- implementation.

Reviews are meant to produce durable records, not temporary comments that vanish when the terminal scrolls.

### Why this matters

A review artifact can:

- justify a gate decision,
- block or allow promotion,
- feed human digest summaries,
- become input for later cycles,
- explain why the engine took a branch rather than just showing that it did.

## Scheduling and auto-approval

Reviews in SIEGE are not assumed to happen ad hoc.

They can be scheduled and evaluated against explicit policies, including auto-approval thresholds. That gives the engine a way to balance throughput against oversight without hiding the decision.

## Conflicts are also first-class

SIEGE models conflicts as typed records with full history retention.

The canonical conflict classes are:

1. **divergence** — two branches or workers produced incompatible outputs for the same target,
2. **decomposition** — two workers decomposed the same unit of work in contradictory ways,
3. **evidence** — the system has contradictory evidence about the same claim,
4. **review disagreement** — reviewers reached incompatible conclusions,
5. **mainline integration** — the branch result conflicts with current mainline state.

This is a major design choice. SIEGE does not treat conflict as “something git will yell about later.”

## Conflict history is durable

Conflict records are not meant to disappear just because one path eventually wins.

Keeping conflict history matters for:

- auditability,
- adjudication,
- training future policies,
- understanding where decomposition or routing is repeatedly going wrong.

## Promotion discipline

Promotion in SIEGE is supposed to be earned.

A successful attempt does not automatically mean “ship it.”
Results may still have to survive:

- review,
- conflict checks,
- integration checks,
- certification requirements,
- stale invalidation rules.

That is what makes the engine closer to a governed control plane than a raw swarm runner.

## Event journal and auditability

Governance only works if the system remembers what happened.

SIEGE’s control-plane and runtime services are designed around durable event recording and idempotent mutation discipline. That gives downstream projections and operators a real history to inspect.

## Recursive improvement without silent self-deception

SIEGE includes recursive-improvement machinery because later cycles should learn from earlier failures.

But it also includes safety-oriented constraints, because a self-improving engine that can quietly rewrite its own scoring logic is not trustworthy.

The intended stance is:

- learn from execution,
- do not silently self-promote,
- do not erase review debt,
- do not treat unresolved conflict as success.

## Why this document exists

A lot of readers will understand planning and execution quickly.
Governance is the part that often looks “heavy” unless the motivation is explicit.

The shortest explanation is this:

SIEGE was built for cases where orchestration should remain inspectable even under parallelism, retries, and policy-driven routing.

If a system is going to coordinate many agents, it needs a memory of why it trusted or blocked them.
