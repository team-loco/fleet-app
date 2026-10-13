---
name: code-quality
description: Self-check or review a Fleet change before calling it done — definition of done, correctness, concurrency, gpui and kube-rs pitfalls, design fidelity. Load when finishing a unit, reviewing a diff, or refactoring.
---

# Code quality

`AGENTS.md` principles apply to every change; this skill turns them into checks.

## Definition of done

Verified by running, not by reading:

1. `mise run check` passes (fmt, clippy `-D warnings` with and without `--features loco`, nextest, typos, deny)
2. A new or changed test fails without the change and passes with it
3. UI changes: the affected screen rendered headlessly in dark and light (`mise run shot -- <screen> dark`
   and `light`) and compared with the matching `design/` file; interactions covered by a `fleet_shots::Ui`
   test in `apps/fleet-shots/tests/ui.rs`. Never launch the app or drive it with synthetic input
4. `docs/TASKS.md` row updated (status, PR)
5. Anything skipped or failing is stated in the report with output

## Correctness

- Encode invariants in types: enums over boolean pairs, newtypes for ids (`ClusterId`, `ResourceRef`)
- Validate external input at the boundary: kubeconfig, API objects, git output, Helm storage blobs
- Unknown Kubernetes fields and versions degrade to "unknown" in the UI; they never panic
- No `unwrap`/`expect`/indexing on data that came from outside the process

## Concurrency

- Tokio lives in `fleet-kube`; gpui code never blocks on it. Cross with async channels only
- No locks held across `.await`; keep critical sections short
- Every watcher, stream and port-forward is owned by a handle whose drop cancels it
- Reconnects back off; a failing cluster never stalls the others

## gpui

- Views render from state; side effects live in event handlers and `cx.spawn`
- Every table uses `uniform_list`; no per-row allocations of large strings during render
- Colours, sizes and radii come from the theme; nothing hard-coded in a screen
- Light and dark both checked

## Maintainability

- Small functions with one job; modules by feature
- Three similar call sites are fine; a shared helper with flags is not
- Refactor when a fix needs another special case; keep refactors out of feature PRs

## Review feedback

- Correctness first, style last; cite `file:line` with a failing scenario
- Separate must-fix from suggestion; confirm suspected bugs with a test
