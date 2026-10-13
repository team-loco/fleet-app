# Fleet — plan

Fleet is a native macOS desktop app for watching and operating many Kubernetes clusters at once, with
Flux built in. It is a free, self-hosted alternative to Lens. The design spec is `design/` (exported
from the "Shoal" canvas; the product is now called Fleet everywhere).

## Audience

1. Primary: the Loco team. A handful of platform engineers running Loco on several clusters
   (local kind, staging, prod; each environment owns its clusters) that are all managed by Flux.
   They need "what is broken, on which cluster, which commit did it" in seconds, plus deep Loco
   internals when built with `--features loco`.
2. Secondary: any engineer who installs `Fleet.app` and points it at a kubeconfig. Nothing in the
   default build assumes Loco.

## Market (summary of `docs/research.md`)

Lens gates GitOps behind a paid tier; Freelens and k9s have no Flux support; Headlamp's Flux plugin
is thin; Capacitor and the Flux Operator UI are per-cluster and browser based. No native client
combines Lens-style multi-cluster browsing with Flux depth. Fleet's differentiators: native speed,
a cross-cluster Flux matrix, HelmRelease history decoded from Helm storage, and commit-to-cluster
outcome tracing.

## Scope

Flux first: HelmRelease, HelmRepository, GitRepository, OCIRepository, and Kustomization (the
design has a Kustomization page; it shares the generic Flux object page, so it costs little).
The Flux layer is written against a `GitOpsObject` abstraction so other engines can follow.

Parity target: every screen in `design/screens` (v2 flows, the source of truth for look and
interaction) plus the v1 screens in `design/v1` (Overview, Pods with logs and shell, Catalog, Compare,
palette) redrawn under the v2 visual rules. The design's sample data ships as `--demo` mode.

Out of scope for v1: Loco friendly names (raw `resource-<uuid>` / `ws-<id>`), the "Ask" AI button,
extensions, Windows and Linux packaging (the code stays portable).

## Architecture

Cargo workspace, Rust 1.99 (edition 2024), toolchain pinned in `mise.toml`.

```
crates/
  fleet-core   plain data: ids, Status (ok/warn/err/progress/suspended/offline), view models,
               Command enum, Backend trait. No async runtime, no gpui. Everything else depends on it.
  fleet-kube   tokio runtime on its own threads; kubeconfig discovery + file watch, login-shell
               env capture for exec plugins, connection checks, kube-rs watchers per cluster,
               logs / exec / port-forward / patch actions, metrics-server + Prometheus.
  fleet-flux   Flux CRD types (kopium from the pinned Flux release), status-to-sentence summarizer,
               inventory, Helm storage secret decode, git commits + diffs (system git, user creds),
               cross-cluster matrix.
  fleet-loco   feature-gated: Loco CRD types from crds/loco, platform/app/build/route models.
  fleet-demo   Backend impl serving the design's sample data (parity screenshots, `--demo`).
  fleet-ui     gpui views: theme tokens, primitives, panels, tables, palette, every screen.
apps/fleet     binary: window, menus, keybindings, backend selection, `loco` feature passthrough.
```

Data flow: the backend (live or demo) runs off the UI thread and pushes `fleet-core` snapshots
over an async channel; a gpui `Store` entity applies them and notifies views. User intents go back as
`Command`s. The UI only ever depends on `fleet-core`, which is what lets UI and backend work proceed
in parallel and lets demo mode render pixel-identical screens.

UI stack: `gpui-component` 0.7.x (latest, built on the `gpui-pre` snapshot of Zed's gpui) for
inputs, menus, scroll areas and virtual lists, restyled to the handoff tokens. Tables use
`uniform_list`. Animations: panel collapse, palette open, row status transitions, live log tail.

## Tools

mise (rust, cargo-nextest, kopium, cargo-bundle, cargo-deny, lefthook, typos, kind, kubectl, flux2,
helm, all pinned), lefthook pre-commit (fmt, clippy, typos), GitHub Actions on macOS, kind + Flux
scenario clusters for e2e (`mise run e2e:up`), Codex and Claude subagents.

## Code quality

- `.claude/skills/code-quality/SKILL.md` is the definition of done; every agent loads it.
- `mise run check` = fmt check, clippy `-D warnings` (default and `--features loco`), nextest,
  typos, cargo-deny. Must pass locally before a PR is opened.
- Every logic change has a test (summarizers, decoders, kubeconfig parsing, status rollups);
  UI is verified by running `--demo` and screenshotting the window against the design.
- Rules from `AGENTS.md`: no code comments, no magic numbers, one source per value, no compat paths.

## Not repeating work

- `docs/TASKS.md` is the ledger: every unit has an id, owner, branch, status. Agents claim a row
  before starting and read the ledger plus `git log` first.
- Decisions live in `docs/decisions.md`; nobody re-litigates them.
- Shared types land first (phase 0) so parallel units never invent their own versions.

## Coordination

The orchestrator (main Claude session) plans, writes prompts, reviews diffs, merges. Minions
(Claude subagents and Codex) each work in their own git worktree on their own branch, own a
disjoint set of files, run `mise run check`, open one small PR, and the orchestrator merges it
immediately. Hotspot files (workspace `Cargo.toml`, module registries) are touched only by the
unit that owns them or merged by the orchestrator.

## Phases

0. Scaffold: workspace, mise tasks, lefthook, CI, `fleet-core` contract, theme tokens, window
   shell with the three-pane layout + status bar running on demo data.
1. Foundations in parallel: ui-kit primitives; kube connection layer; Flux types + summarizer +
   Helm decode; Loco types; kind + Flux e2e scenarios; demo dataset.
2. Screens in parallel, one unit per screen group (see `docs/TASKS.md`).
3. Integration: live backend wired to every screen, actions with confirmation, e2e on kind,
   `.app` bundle with the icon, animation polish, a parity pass per screen in light and dark.
