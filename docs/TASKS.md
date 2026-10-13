# Task ledger

Claim a row (owner + branch, status `doing`) before starting. Status: `todo` · `doing` · `pr` · `done`.
Spec column points at `design/`.

## Phase 0 — scaffold

| id | unit | spec | owner | branch | status |
|---|---|---|---|---|---|
| P0 | workspace, mise tasks, lefthook, CI, fleet-core contract, theme tokens, window shell (strip, two panels, page header, status bar) on demo data | HANDOFF.md, Nav, StatusBar | claude | p0-scaffold | done (#2) |

## Phase 1 — foundations (parallel)

| id | unit | spec | owner | branch | status |
|---|---|---|---|---|---|
| F1 | ui-kit: status glyph, section label, rule, buttons, ⋯ action menu with descriptions, table on uniform_list, dock, collapsible panel animation, code/diff view | HANDOFF visual rules | | | todo |
| F2 | fleet-kube: kubeconfig discovery + watch, login-shell env capture, cluster connect + checks, per-cluster watchers → core snapshots | AddCluster, AddClusterCheck | | | todo |
| F3 | fleet-flux: CRD types (kopium, Flux 2.9.6), status summarizer, inventory, Helm storage decode, cross-cluster matrix | Kustomization, Flux | | | todo |
| F4 | fleet-loco: CRD types from crds/loco, app/build/route/platform models | Loco* | | | todo |
| F5 | e2e: kind clusters + Flux + scenario workloads (crashloop, missing secret key, failing HelmRelease, suspended, drift) | Fleet, Workload | | | todo |
| F6 | fleet-demo: full design sample dataset behind the Backend trait | all screens' scripts | | | todo |

## Phase 2 — screens (parallel)

| id | unit | spec | owner | branch | status |
|---|---|---|---|---|---|
| S1 | Fleet home: clusters table, problems by cluster, recent GitOps commits | Fleet | | | todo |
| S2 | Command palette ⌘K | Command, v1/Palette | | | todo |
| S3 | Workload (Deployment) page + logs / events / terminal dock | Workload | | | todo |
| S4 | Flux object page (Kustomization + HelmRelease + sources) | Kustomization | | | todo |
| S5 | Changes: commits, per-cluster outcome, full diff, rendered views | Changes | | | todo |
| S6 | GitOps sync grid across clusters | Flux | | | todo |
| S7 | Add cluster flow (contexts, checks) | AddCluster, AddClusterCheck | | | todo |
| S8 | Cluster overview, Nodes, Namespaces, Events, generic resource lists | v1/Main, Nav | | | todo |
| S9 | Pods: list, details, logs, shell | v1/Pods | | | todo |
| S10 | Compare across clusters, cluster settings (Catalog) | v1/Compare, v1/Catalog | | | todo |
| S11 | Loco: Platform, Applications, Builds, Routes | Loco* | | | todo |

## Phase 3 — integration

| id | unit | owner | branch | status |
|---|---|---|---|---|
| I1 | live backend wired into every screen | | | todo |
| I2 | actions: reconcile, suspend/resume, scale, restart, delete, port-forward, exec, with confirmation | | | todo |
| I3 | e2e run against kind scenarios | | | todo |
| I4 | Fleet.app bundle, icon (squircle 22%), version stamping | | | todo |
| I5 | animation polish + parity pass, dark and light, every screen | | | todo |
