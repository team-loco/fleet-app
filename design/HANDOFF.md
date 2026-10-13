# Shoal design handoff (GPUI)

Native desktop app for debugging Kubernetes clusters, with first-class Loco support behind a build flag.
Each screen is in `screens/`: the markup holds the layout and inline styles, and the `<script>` holds the sample data and interactions. These files need the design canvas's runtime, so they won't render on their own in a browser. Read them as a spec and rebuild in GPUI; don't port the HTML.

## Layout (every window)

Superseded in Fleet by decision 18 (`docs/decisions.md`): one sidebar, clusters switch from the status bar.

```
┌───────────────────┬────────────────────────────────────┐
│ ● ● ●           « │ page header (44px): name · status  │
├───────────────────┤   one primary action + ⋯ menu       │
│ Overview          │                                    │
│ Nodes      ▲1 14  │ content (scrolls)                  │
│ Namespaces    23  │                                    │
│ Events        ▲7  │                                    │
│ LOCO (installed)  │                                    │
│ WORKLOADS / FLUX  │                                    │
│ 196px             │                                    │
├───────────────────┴────────────────────────────────────┤
│ status bar 26px: ✕ prod-us-east ^ · ✕3 ▲6 · EKS · us-east-1 · v1.31.2 · ● loco agent synced 4s ago │
└────────────────────────────────────────────────────────┘
```

- One sidebar under the 28px strip (window controls and the collapse chevron, no title bar). ⌘B collapses it to 76px: the strip and an expand chevron, no cluster name or state.
- On Fleet home the sidebar holds Overview, Problems, GitOps and Changes; in a cluster it holds that cluster's pages. The LOCO section appears only on clusters running Loco.
- The status bar starts with the cluster switcher: the current cluster's status glyph and name (◎ All clusters on Fleet home). A click or ⌘⇧C opens a filterable, keyboard-driven list of every cluster with its state and problem counts, plus All clusters and Add cluster; each cluster's ⋯ holds Settings, Connect or Disconnect and Remove from Fleet. Switching never disconnects a cluster.
- Plain cluster names with a status glyph. No icons or avatars.
- No tabs, no breadcrumbs, no panel-layout buttons. ⌘K opens the command palette from the keyboard; there is no visible button for it.
- One visible primary action per page (for example Reconcile). Everything else goes in a ⋯ / Actions menu, each item with a one-line description.
- Inspectors slide and fade in from the right and out again when closed.

## Visual rules

- Font: IBM Plex Sans for UI at 13px, IBM Plex Mono for data at 12px. Uppercase 11px section labels.
- Corners: 3px radius maximum, 4px for menus and dialogs. No pills, no cards. Sections are separated by 1px rules.
- Status uses both shape and colour: ● ok, ▲ warning, ✕ error, ◐ in progress, ‖ suspended, ○ offline. ○ in mute also marks a Loco build that ended without finishing (canceled or superseded); it is neither an error nor a success.
- Pages read like documents. Put one plain sentence about what's wrong first, then data. Diffs are always shown in full.
- Light and dark follow the OS. Every colour is a token; the values are below.

| token | dark | light | use |
|---|---|---|---|
| editor | #111317 | #FFFFFF | content background |
| panel | #15181D | #F6F7F8 | side panels, inspectors |
| chrome | #1A1D22 | #EEEFF1 | status bar, strip, popover footers |
| hover | #1D2127 | #F1F2F4 | row hover |
| line / line2 | #262A30 / #323740 | #E1E3E7 / #CFD3D9 | rules / control borders |
| fg / fg2 / mute | #DCDFE4 / #B3B9C3 / #8E96A3 | #1D2027 / #3B414B / #5F6774 | text levels |
| sel / selfg / selline | #1F2C40 / #F2F6FC / #3A5A8C | #DCE7FA / #0D2850 / #8DB0EA | selection |
| acc / onacc | #74ADFF / #0C1220 | #1F5FD1 / #FFFFFF | links, primary button |
| ok / warn / err / sus | #48C78E / #E8B04B / #F26D6D / #9AA2AE | #167A4C / #935800 / #C53030 / #6B7280 | status |
| okbg / warnbg / errbg | ok/warn/err at 10–13% | 7–10% | tinted rows, diff +/- |

Row heights: 24–28px for tables, 26px for nav rows. Use virtualized lists (`uniform_list`) for every table.

## Screens (`screens/`)

**Loco (shown only when built with the loco flag)**
- `LocoPlatform`: agent ↔ control-plane Sync stream, last heartbeat, the health it reports (kubernetes, controller), reconnects, builds enabled. Placements out of sync (apply error/retrying, env secret pending). Component table: agent, loco-controller, loco-build-controller, envoy-gateway and proxy, cert-manager, cilium, otel, clickhouse, observability-proxy. Agent log.
- `LocoApps`: Application CRs (infra.loco.io/v1alpha1) grouped by workspace (`ws-<id>` namespace). The detail panel shows phase, the controller's own Degraded message, image and deployment, desired vs applied placement revision, and the objects the controller created (Deployment, Service, HTTPRoute, env Secret, SA, NetworkPolicies), plus the route path.
- `LocoBuilds`: Build CRs run as Jobs in `loco-builds` (fetch init container + buildkit), with phase, duration, digest and cache hit, and a log pane.
- `LocoRoutes`: listeners on Gateway `eg`, then HTTPRoutes with problems first (no ready endpoints, BackendNotFound).
- Open question: the cluster only knows `resource-<uuid>`. The design assumes friendly names come from the user's `loco login` session against the Loco API.

**Triage flow**: `Fleet` → `Command` (⌘K) → `Workload` → `Kustomization` → `Changes`
- `Fleet`: clusters table, problems grouped by cluster, recent GitOps commits with per-cluster outcome.
- `Workload`: deployment page with pods, rollout history linked to commits, resolved env vars (flags a missing secret key), and a logs/events/terminal dock.
- `Kustomization`: Flux object page with one-sentence status, managed objects, history, the same Kustomization on other clusters, and Spec at the bottom.
- `Changes`: commits with per-cluster outcome, plus a full diff with source and per-cluster rendered views.

**Other**: `Flux` (sync grid across clusters), `AddCluster` and `AddClusterCheck` (pick kubeconfig contexts, then connection checks), `AppIcon`.
Light variants: `*Light.dc.html`.
Shared parts: `Nav` (the sidebar; its clusters panel is superseded by the status bar switcher), `StatusBar`.

`assets/shoal-logo.png` is the app icon; on macOS use a squircle with a ~22% corner radius.
