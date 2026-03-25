# Dashboard, API, and CLI

SIEGE is not only a library or backend. It exposes the engine through multiple operator-facing surfaces.

That matters because orchestration should be inspectable, not just executable.

## Dashboard

The web dashboard is the main cockpit for watching a loop move.

The intended panel set covers:

- dashboard,
- chat,
- plan,
- tasks,
- graph,
- branches,
- conflicts,
- certification,
- reviews,
- settings,
- skills,
- loop history.

Together, those views are meant to answer four operator questions:

1. what is the system trying to do,
2. why is it blocked or allowed,
3. what is running right now,
4. what happened that changed state.

## What the dashboard should make obvious

A good orchestration UI is not just a prettier log stream.

The dashboard should help operators see:

- gate status,
- objective and plan structure,
- task ordering and dependencies,
- branch or lane progression,
- review and conflict pressure,
- certification queues,
- policy choices,
- historical loop behavior.

## API

The HTTP API is the machine-facing surface for SIEGE.

Typical route families include:

- objectives,
- chat / extraction,
- plans and gate status,
- tasks and nodes,
- branches and conflicts,
- reviews,
- certification,
- settings / policy,
- event streams.

The API is the right place to build external automations, control panels, or higher-level integrations.

## Swagger / OpenAPI

SIEGE exposes documented API surfaces and can generate TypeScript-facing shapes from those contracts.

That matters because orchestration systems become fragile quickly when the UI, backend, and automation scripts all invent separate DTOs by hand.

## CLI REPL

The CLI REPL is the fastest way to operate SIEGE without the dashboard.

A typical workflow looks like:

```text
siege> create objective
siege> status
siege> gate
siege> tasks
siege> tail
```

That gives contributors and operators a lightweight way to:

- create or inspect objectives,
- watch cycle progress,
- inspect gate conditions,
- check task status,
- tail event activity.

## Events and live operation

SIEGE’s operator surfaces are built around live state inspection.

Depending on the surface, that can mean event streams, polling, or both. The important point is that operators should not need to ssh into a worker and grep random files just to understand why the engine is blocked.

## Recommended usage

If you are integrating with code, prefer the API.
If you are operating interactively, use the dashboard or CLI.
If you are debugging orchestration behavior, use all three together:

- dashboard for system shape,
- API for exact contract output,
- CLI for fast loop inspection and tailing.

## Why this matters

A multi-agent engine without good operator surfaces becomes superstition very quickly.

You can watch tokens burn, but you cannot tell whether the engine made a principled decision.

The dashboard, API, and CLI exist to keep the orchestration loop legible.
