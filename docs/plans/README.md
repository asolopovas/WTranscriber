# Execution plans

Plans record work that spans multiple turns or where decisions must survive context resets.

## Directories

A plan's directory must match its `Status:` line (enforced by `scripts/lint-docs.ts`):

```text
docs/plans/active/     Status: active     — in-progress
docs/plans/completed/  Status: completed  — finished, kept for design history
docs/plans/abandoned/  Status: abandoned  — superseded, with a short reason
```

Keep one-turn tasks out unless the plan records important design history.

## When to create a plan

Create `docs/plans/active/<slug>.md` when work has any of these:

- Multiple phases or expected handoffs.
- User-visible behaviour with non-trivial acceptance criteria.
- Architecture, release, Android, or ASR pipeline risk.
- A bug needing reproduction evidence and a verification log.
- A refactor whose decisions should outlive the branch.

## Template

```md
# <Plan title>

Status: active
Owner: agent
Started: YYYY-MM-DD
Related docs: links

## Goal

## Acceptance criteria

## Current context

## Steps

- [ ] ...

## Decisions

- YYYY-MM-DD: ...

## Verification log

- YYYY-MM-DD: command/result

## Handoff notes
```

## Lifecycle

- Update decisions and checkboxes as work proceeds; do not leave stale checkboxes.
- On completion, set `Status: completed`, add final verification notes, and move to `completed/`.
- On supersession, set `Status: abandoned`, record why, and move to `abandoned/`.
- If a plan reveals a durable rule, update `AGENTS.md`, a relevant doc, or tooling before closing it.
