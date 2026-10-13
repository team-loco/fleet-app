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
| F1 | ui-kit: status glyph, section label, rule, buttons, ⋯ action menu with descriptions, table on uniform_list, dock, collapsible panel animation, code/diff view | HANDOFF visual rules | claude | f1-ui-kit, f1-ui-kit-2 | done (#3, #12) |
| F2 | fleet-kube: kubeconfig discovery + watch, login-shell env capture, cluster connect + checks, per-cluster watchers → core snapshots | AddCluster, AddClusterCheck | claude | f2-fleet-kube, f2-fleet-kube-live | done (#7, #9) |
| F3 | fleet-flux: CRD types (kopium, Flux 2.9.6), status summarizer, inventory, Helm storage decode, cross-cluster matrix | Kustomization, Flux | Codex | f3-fleet-flux | done |
| F4 | fleet-loco: CRD types from crds/loco, app/build/route/platform models | Loco* | Codex | f4-fleet-loco | done |
| F5 | e2e: kind clusters + Flux + scenario workloads (crashloop, missing secret key, failing HelmRelease, suspended, drift) | Fleet, Workload | codex | f5-e2e-scenarios | done (#1) |
| F6 | folded into screen units: each S-unit owns its fleet-core model, fleet-demo data and fleet-ui screen | | | | dropped |

## Phase 2 — screens (parallel)

| id | unit | spec | owner | branch | status |
|---|---|---|---|---|---|
| S1 | Fleet home: clusters table, problems by cluster, recent GitOps commits | Fleet | claude | s1-fleet-home | done |
| S2 | Command palette ⌘K | Command, v1/Palette | claude | s2-command-palette | done |
| S3 | Workload (Deployment) page + logs / events / terminal dock | Workload | claude | s3-workload | done |
| S4 | Flux object page (Kustomization + HelmRelease + sources) | Kustomization | claude | s4-flux-object | done |
| S5 | Changes: commits, per-cluster outcome, full diff, rendered views | Changes | claude | s5-changes | done |
| S6 | GitOps sync grid across clusters | Flux | claude | s6-gitops-grid | done |
| S7 | Add cluster flow (contexts, checks) | AddCluster, AddClusterCheck | claude | s7-add-cluster, s7-add-cluster-live | done (#18) |
| S8 | Cluster overview, Nodes, Namespaces, Events, generic resource lists | v1/Main, Nav | claude | s8-cluster-pages | done (#17, overview) |
| S9 | Pods: list, details, logs, shell | v1/Pods | claude | s9-pods | done |
| S10 | Compare across clusters, cluster settings (Catalog) | v1/Compare, v1/Catalog | claude | s10-compare-settings | done |
| S11 | Loco: Platform, Applications, Builds, Routes | Loco* | claude | s11-loco | done (#25, Platform + Routes PR) |

## Phase 3 — integration

| id | unit | owner | branch | status |
|---|---|---|---|---|
| I1 | live backend wired into every screen (I1a: home, cluster pages, pods; I1b: live Flux object pages, GitOps grid, Changes + home commits from git mirrors, reconcile progress; I1d: live Loco Platform, Applications, Builds, Routes, restart, cancel build) | claude | i1a-live-cluster-pages, i1b-live-flux, i1d-live-loco | done (#30, #32, #33, #36) |
| I1c | live follow-ups: startup presets on live, YAML refresh, connection commands, one Flux summary, terminal resize, StatefulSet/DaemonSet rollback, live Compare | claude | i1c-presets-yaml, i1c-connection, i1c-flux-summary, i1c-terminal-rollback, i1c-compare-live | done (#81, #88, #92, #101, compare PR) |
| I2 | actions: reconcile, suspend/resume, scale, restart, delete, port-forward, exec, with confirmation | claude | i2-live-actions | done (#29, #31) |
| I3 | e2e run against kind scenarios | claude | i3-e2e-suite | done |
| I4 | Fleet.app bundle, icon (squircle 22%), version stamping | claude | i4-app-bundle | done |
| I5 | animation polish + parity pass, dark and light, every screen | | | todo |
| I6 | headless screenshots (`mise run shot`) and UI interaction test helper (`fleet_shots::Ui`), no on-screen window | claude | headless-shots | done |

| CPERF | scale scenarios, snapshot benchmarks and performance fixes | codex | cperf-scale | doing |

## Font repair

| id | unit | owner | branch | status |
|---|---|---|---|---|
| GLYPH | restore upstream Plex and ship Fleet Status glyph font (#95) | Codex | status-glyph-font | done |
| GLYPH2 | bundle the all-clusters bullseye in Fleet Status | Codex | all-clusters-glyph | done (#100) |

## Follow-ups

| id | unit | owner | branch | status |
|---|---|---|---|---|
| C77 | Palette open in new window; split follow-up in #77 | codex | c77-windows-split | done |
| SEC1 | redact Secret data in YAML views and Helm diffs; strip credentials from URLs, git errors, auth and kubeconfig errors | claude | secret-redaction | done |
| SHELL2 | single sidebar, cluster switcher in the status bar, Loco nav only where installed, animated inspectors (#110, #111, #121) | claude | shell-single-sidebar | done |
