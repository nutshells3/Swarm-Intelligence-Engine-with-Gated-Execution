# Formal assurance

SIEGE includes an assurance path for cases where normal agent execution is not enough.

The important thing is that this path is not only about validating final outputs. SIEGE can route **planning artifacts** and **execution artifacts** through formal-assurance machinery.

## Two assurance surfaces

### 1. Planning assurance

Planning artifacts can be exported through the `formal-readiness` layer and submitted for formal-claim style validation before or during promotion into execution.

This lets the engine ask questions such as:

- is the dependency structure coherent enough to trust,
- are plan invariants explicit enough to verify,
- is the gate state strong enough to justify execution,
- should a plan be blocked or escalated before code is written.

### 2. Result assurance

Execution outputs can also be routed through certification flows after implementation work succeeds.

This is the more familiar path: validate claims, audit results, compare formalizers, project the outcome back into the orchestration system, and block or promote accordingly.

## Main building blocks

### `formal-readiness`

This package handles the “are we even ready to formalize this?” question.

It provides:

- predicates for what should be formalized,
- export surfaces for backend-neutral formal artifacts,
- readiness checks for planning exports,
- consistency helpers for review / certification / export coherence.

### `integration`

This package is the gateway layer.

It models:

- claim submissions,
- certification queue state,
- gateway responses,
- local gate effects,
- lane transitions,
- divergence and stale-result handling.

### `deployment`

This package decides how formal work is routed.

Depending on deployment mode, assurance can be:

- local only,
- local with remote fallback,
- remote-preferred,
- disabled for dev/test scenarios.

## Local and remote execution

SIEGE is not locked to one assurance transport.

The formal-claim path can be wired through:

- a local CLI gateway,
- an HTTP gateway,
- routing rules that distinguish transport failure from certification failure.

That last point matters. A remote timeout is not the same thing as “the claim failed certification.”

## Dual formalization

For sensitive cases, SIEGE can route work through two formalizers.

The point is not just redundancy. The point is to detect divergence.

If two formalization paths disagree materially, that disagreement becomes visible state. It can block promotion, create conflict artifacts, or require human review rather than quietly pretending everything passed.

## Certification grades and downstream effects

SIEGE does not need assurance to be binary.

The robustness and certification policy layers can model graded outcomes and map them to downstream permissions. Different actions can require different assurance strength.

That is important for real systems because “useful but not perfect” and “safe to promote to mainline” are not always the same threshold.

## Stale invalidation

One of the easiest mistakes in assurance-heavy systems is to treat an old certification as permanently valid.

SIEGE avoids that by modeling stale invalidation explicitly.

If upstream artifacts change, the system can mark prior certifications as stale instead of silently carrying them forward as if nothing happened.

## How planning validation fits

The newer planning-validation path changes the assurance story in an important way.

It means SIEGE can now do more than certify outputs after the fact. It can also ask whether the plan itself deserves confidence.

That unlocks workflows like:

```text
objective
  -> planning artifacts
  -> readiness check
  -> plan export
  -> formal-claim validation
  -> allow / block / escalate
  -> only then decompose and dispatch
```

For domains with safety, compliance, or formal correctness pressure, that is materially stronger than “implement first, certify later.”

## Candidate selection

Not every artifact should go through formal assurance.

Selection can be driven by things such as:

- explicit `certification_required` flags,
- policy routing,
- claim criticality,
- plan-gate demands,
- contract / invariant / proof / safety semantics.

This lets SIEGE reserve expensive assurance for the places where it matters.

## When to use this path

Formal assurance makes the most sense when one or more of these are true:

- the domain is safety-critical,
- policy or compliance requires stronger evidence,
- the plan contains non-negotiable invariants,
- review disagreement is too costly to resolve informally,
- the cost of silent failure is higher than the cost of additional gating.

## Practical note

SIEGE is still useful without any formal-assurance backend enabled.

The orchestration engine, planning gate, execution flow, reviews, and conflicts all stand on their own.

The formal path exists so the same runtime can scale from ordinary engineering automation to stronger assurance regimes without a separate orchestration stack.
