# Agent dev loop — design notes

Module of [`AGENTS.md`](../AGENTS.md). Roster, runbook, and prohibitions live there. This file holds the _why_. For HMR/CDP mechanics see [`dev-loop.md`](dev-loop.md).

## Why four agents, four verbs

The verbs are mutually exclusive: **answer** (`wt-investigate`, read-only on source), **mutate** (`wt-edit`, the only writer of project files and — only in `mode: finalise` — git history), **install/test on device** (`wt-runner`), **observe the swarm** (`wt-monitor`, read-only on artefacts). A task maps to exactly one verb; selection is not a judgement call.

Earlier rosters split each verb further (`scout`/`triage`/`researcher` all answered; `coder`/`docs-updater` both mutated; ship lived in a separate agent). Per Anthropic's writing-tools-for-agents guidance, overlapping responsibilities create selection ambiguity and degrade outcomes. Consolidation traded fine-grained personas for an unambiguous routing table.

## Why the answer agent never edits

Diagnosis context is wider than fix context — investigating an OOM means tailing logcat, running CDP probes, walking `git log`. Letting the same agent edit produces drive-by patches that "fix" the wrong thing. `wt-investigate` observes; `wt-edit` applies the spec.

## Why edit owns finalise, but not by default

`wt-edit mode: edit` never touches git; `mode: finalise` is a separate explicit invocation that stages only the named paths and lets the pre-commit hook gate the change. Same agent, two modes, single writer to history — `--no-verify` stays forbidden, and the orchestrator can always re-dispatch finalise from a fresh context with the named path set repeated verbatim.

## Why monitor is read-only and never manages agents

`wt-monitor` reports on the orchestration itself: stale artefacts, missing handoffs, parallelism left on the table, repeated contract misses, verification gaps. If it could restart, interrupt, or replace workers it would become a second orchestrator with no audit trail and no replayable artefact. It writes one file (`tmp/monitor-<slug>.json`), surfaces insights plus a single recommended next action, and stops; the main thread decides and dispatches.

## Why filesystem, not events

Async subagents cannot wake the parent on completion (anthropics/claude-code#20921, openai/codex#15723). Workers signal through append-only files under `tmp/`: `investigate-<slug>.md`, `edit-report.json`, `last-commit.json`, `install-report.json`, `test-report.json`, `monitor-<slug>.json`. Main thread polls between turns. Never pipe one agent's stdout into another — the file artefact is the contract and the only replay surface.

## One file, one owner per parallel run

If two `wt-edit` invocations could touch the same file, serialise them or use git worktrees (`worktree: true` in parallel subagent calls). Every invocation must leave an artefact — replay is otherwise impossible. `wt-monitor` may run alongside any worker batch because it only reads existing artefacts and writes its own slug-scoped file.
