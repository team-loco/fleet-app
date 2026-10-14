# Flock: market and feature research (2026-10-12)

Scope: native macOS (Rust + GPUI) multi-cluster viewer with first-class Flux CD, as a free/self-hosted alternative to Lens Desktop.
Confidence tags: [verified] = read on the cited page today; [unverified] = from memory or search snippets only, check before relying on it.

## 1. Competitors

| Tool | Native? | Multi-cluster at once? | Flux support depth | License / price | Notable |
|---|---|---|---|---|---|
| Lens Desktop (Mirantis/Lens) | No, Electron | Yes, catalog of clusters, one window per cluster view | Docs list GitOps (Flux CD, Argo CD) under the paid tier; depth unknown [verified for plan table, unverified for depth] | Closed source. Personal free (students, hobbyists, orgs under $10M revenue/funding; larger orgs must upgrade after 30 days). Plus $25/mo annual ($30 monthly) adds AI, GitOps, EKS/AKS/GKE integrations. Pro $25/seat/mo, Enterprise $50/seat/mo (SSO/SCIM, air-gapped, audit). [pricing](https://lenshq.io/pricing) | Catalog, Helm, logs, extension marketplace, "Ask AI", Trivy scanning, cost tools [docs](https://docs.lenshq.io/) |
| Freelens | No, Electron 41 | Yes (Lens-style catalog) | None built in; Lens-era extensions ported by community (no Flux mention on the repo page) | MIT, free. [repo](https://github.com/freelensapp/freelens) | Active: v1.10.3 on 2026-07-07, Helm v4, kubectl 1.36, metrics time-range selector, air-gapped kubectl mirror, shell-env work done in v2.0.0 milestone [releases](https://github.com/freelensapp/freelens/releases) |
| Headlamp (CNCF/Kubernetes SIG UI) | Web app plus Electron desktop wrapper | Yes (multiple clusters from kubeconfig) | Plugin "flux" ("Visualize Flux in Headlamp"), maintained by one person; depth not documented [plugins repo](https://github.com/headlamp-k8s/plugins) | Apache-2.0, free | 25+ plugins (prometheus, app-catalog for Helm, cert-manager, kyverno, karpenter, AI assistant) |
| k9s | Terminal UI | One context at a time (`:ctx` switch) | None built in; Flux CRDs browsable as generic CRDs, plugins can add shortcuts | Apache-2.0, free, ~34.8k stars [repo](https://github.com/derailed/k9s) | Fast, keyboard-driven, plugins.yaml, port-forward, logs, YAML edit |
| Aptakube | Yes (Tauri-based, per marketing; [unverified]) macOS/Win/Linux | Yes, multi-cluster views, aggregated logs | None called out | Closed source. Free on clusters up to 5 nodes; $79/yr personal; $59/seat/yr team; 15-day trial for larger clusters. [pricing](https://aptakube.com/pricing) | Aggregated multi-cluster logs, metrics, YAML edit, port-forward, no in-cluster install |
| Seabird | Yes (native, cross-platform) | Single cluster focus [unverified] | None | MPL-2.0, free [search](https://korben.info/en/seabird-native-kubernetes-client.html) | Simple, "bloat-free" resource browser; scale/start/stop; low feature depth |
| Capacitor (Gimlet) | Local binary serving a browser UI (`capacitor next --port 3333`) | Yes | Flux-only: resource tree, Helm history and manifest diff, Kustomization diff cluster vs git, RBAC-aware discovery. Closest functional match to what Flock's Flux tab should do | Apache-2.0, free; team hosting in beta [repo](https://github.com/gimlet-io/capacitor) | Keyboard navigation, local-first, ~1.2k stars |
| Flux Operator Web UI ("Flux Status Page", ControlPlane) | Web, embedded in the operator in-cluster | Cross-cluster view claimed; per-cluster install, SSO required for sharing [unverified for true multi-cluster] | Deep: HelmRelease dashboards, dependency graph, events, reconciliation history, log viewer, actions (reconcile, suspend, resume, rollout restart) under RBAC | AGPL-3.0 (per product page) [web-ui](https://fluxoperator.dev/web-ui/) | Built by Flux core maintainers; the reference for Flux UX. Needs ingress, OIDC, an in-cluster component |
| Weave GitOps | Web, in-cluster | Limited | Deep Flux, but Weaveworks shut down in 2024; treat as unmaintained [unverified for 2026 status] | Apache-2.0 | Do not benchmark against it |
| Other desktop clients (Aptakube-likes: Kubernetes Dashboard, OpenLens, Kubenav, Headlamp) | mixed | mixed | none | mixed | OpenLens is superseded by Freelens |

Gap Flock can fill: no native (non-Electron) free client combines Lens-style multi-cluster browsing with Capacitor/Flux-Operator-grade Flux views. Flux depth is currently only in browser or in-cluster tools.

## 2. Lens feature inventory (Flock parity list)

Source basis: [Lens docs](https://docs.lenshq.io/), [Lens FAQ](https://docs.lenshq.io/faq/technical-faq/), Freelens repo. Feature-by-feature details below come largely from known Lens behavior [unverified on the 2026 build]; the docs index only confirmed catalog, Helm, extensions, AI, Trivy, cost, and GitOps.

| Feature | Lens behavior | Flock priority |
|---|---|---|
| Catalog and cluster list | Entities from kubeconfig, grouped by source/labels; click to open a cluster | Must |
| Hotbar | Pinned quick-launch of clusters/entities | Nice (a pinned/favorites sidebar covers it) |
| Cluster overview | Node/pod/CPU/memory/capacity gauges, workload status counts, recent events | Must (basic), metrics-backed gauges nice |
| Workloads browser | Pods, Deployments, DaemonSets, StatefulSets, ReplicaSets, Jobs, CronJobs; sortable/filterable tables, namespace filter | Must |
| Config | ConfigMaps, Secrets (reveal/decode), ResourceQuotas, LimitRanges, HPAs, PodDisruptionBudgets, PriorityClasses | Must for ConfigMap/Secret; rest nice |
| Network | Services, Endpoints, Ingresses, IngressClasses, NetworkPolicies, Gateway API (newer) | Must for Service/Ingress; rest nice |
| Storage | PVCs, PVs, StorageClasses | Nice |
| Nodes and namespaces | Node list/detail (cordon, drain), namespaces | Must (view), cordon/drain nice |
| Access control | ServiceAccounts, Roles, RoleBindings, ClusterRoles | Nice |
| CRD browser | Lists CRDs, then instances of each; generic table with printer columns | Must (Flux is all CRDs; also need generic fallback) |
| Pod logs | Streaming, per-container, previous, timestamps, search, download | Must |
| Aggregated multi-pod logs | Not in Lens; Aptakube sells it | Nice, differentiator |
| Exec shell | Pod terminal (xterm) plus per-cluster terminal tab with kubectl pre-configured | Must (pod exec); cluster terminal nice |
| Port-forward | Per-service/pod, list of active forwards, auto-open in browser | Must |
| Edit YAML | In-app editor, apply on save, create-resource templates | Must (edit and apply with server-side apply or patch); templates nice |
| Delete, scale, restart | Row actions | Must |
| Helm releases | List via Helm secrets (all namespaces), values, notes, rollback/upgrade; chart browser (Artifact Hub) | Must view-only (history, values, manifest); install/upgrade UI skip because Flux owns releases |
| Events | Cluster events view plus per-resource events | Must |
| Metrics source | Auto-detects Prometheus via service discovery (Lens "Prometheus Operator", "Helm", "Lens bundled" presets, or user-set service path) and proxies PromQL through the API server | Nice for v1; see detection note below |
| Extensions / marketplace | Electron extension API, npm-based | Skip (no compat path); consider a small plugin story later |
| Kubeconfig sync folders | Preferences > Kubernetes > "Sync kubeconfig folders" watches dirs, auto-adds contexts; default includes `~/.kube/config` | Must (watch `~/.kube/config` and extra dirs; multi-file KUBECONFIG) |
| Command palette, search | Cmd+Shift+P | Nice |
| Themes, dark mode | Yes | Must (follow system) |
| AI assistant, Trivy, cost, SSO/team features | Paid tiers | Skip |

Prometheus detection approaches (for later metrics work): (1) try well-known service paths (`monitoring/prometheus-operated:9090`, `monitoring/kube-prometheus-stack-prometheus`, `prometheus/prometheus-server`); (2) discover by label (`app.kubernetes.io/name=prometheus`, `operated-prometheus=true`); (3) per-cluster user override; reach it through the API-server service proxy `/api/v1/namespaces/{ns}/services/{svc}:{port}/proxy/`. Fall back to metrics-server (`metrics.k8s.io`) for node/pod CPU and memory, which needs no Prometheus.

Freelens evidence that metrics and kubeconfig watching remain active areas: kubeconfig watching and Prometheus compatibility fixes listed in recent releases ([releases](https://github.com/freelensapp/freelens/releases)).

## 3. Flux UX specifics

### API versions (as of Flux 2.9.x)
- Latest release read: Flux v2.9.6, 2026-10-01; v2.9.0 on 2026-06-30 removed `image.toolkit.fluxcd.io/v1beta2` and `notification.toolkit.fluxcd.io/v1beta2` [releases](https://github.com/fluxcd/flux2/releases).
- `helm.toolkit.fluxcd.io/v2` HelmRelease [verified](https://fluxcd.io/flux/components/helm/api/v2/). (Older v2beta1/v2beta2 are gone from current releases [unverified].)
- `source.toolkit.fluxcd.io/v1` now contains Bucket, GitRepository, HelmChart, HelmRepository, OCIRepository, ExternalArtifact [verified](https://fluxcd.io/flux/components/source/api/v1/).
- `kustomize.toolkit.fluxcd.io/v1` Kustomization [unverified; stable since 2.0].
- New since 2.7: ExternalArtifact and ArtifactGenerator (source composition/decomposition) [search](https://v2-7.docs.fluxcd.io/blog/2025/09/flux-v2.7.0/). 2.8: Helm v4, server-side apply and kstatus health for HelmRelease, CEL readiness for Helm-managed objects, and `status.inventory` on HelmRelease [search result, 2.8 notes].
- Flock should discover versions through API discovery per cluster and prefer the highest served stable version, because clusters will run different Flux versions.

### Triggering actions (annotations)
- Reconcile now: set `reconcile.fluxcd.io/requestedAt` to a new value (the CLI uses an RFC3339 timestamp); controller copies it to `status.lastHandledReconcileAt` [verified in [fluxcd/pkg annotations](https://github.com/fluxcd/pkg/blob/main/apis/meta/annotations.go) and the helm-controller source].
- Force one-off install/upgrade: `reconcile.fluxcd.io/forceAt` (same value-change semantics; CLI `flux reconcile hr --force`). Verified string for forceAt; the `meta.ForceRequestStatus` field tracks it.
- Reset failure count: `reconcile.fluxcd.io/resetAt` (CLI `--reset`). The status field `lastHandledResetAt` is verified in [helmrelease_types.go](https://raw.githubusercontent.com/fluxcd/helm-controller/main/api/v2/helmrelease_types.go); the annotation string itself is [unverified].
- `--with-source` in the CLI reconciles the source first, then the HelmRelease; Flock should do the same (annotate source, wait for new artifact revision, annotate release) [CLI doc](https://fluxcd.io/flux/cmd/flux_reconcile_helmrelease/).
- Suspend / resume: patch `spec.suspend: true/false` on any Flux kind. Setting requestedAt on a suspended object is ignored.
- Use the same annotation value across kinds for "reconcile with source": use a timestamp, patch with merge patch; do not use `kubectl apply` (field-ownership conflicts with Flux server-side apply).

### What operators need (feature list for the Flux view)
1. List of all Flux objects per cluster with Ready/Suspended/Stalled state, last revision, last reconcile time, message. Grouped across clusters in one table.
2. HelmRelease detail: chart + version, source ref, values (merged `valuesFrom` resolved), conditions (Ready, Released, TestSuccess, Remediated, Drifted), last attempted revision, failure counters, `history` snapshots (chart version, app version, status, digest, first/last deployed), `inventory` of applied objects (2.8+).
3. Condition reasons to surface prominently, from [condition_types.go](https://raw.githubusercontent.com/fluxcd/helm-controller/main/api/v2/condition_types.go): InstallSucceeded/InstallFailed, UpgradeSucceeded/UpgradeFailed, TestSucceeded/TestFailed, RollbackSucceeded/RollbackFailed, UninstallSucceeded/UninstallFailed, ArtifactFailed, DependencyNotReady, DriftDetected/NoDriftDetected. Show retry state vs remediation (`RemediateOnFailure` default does rollback/uninstall between retries; `RetryOnFailure` just retries) [HelmRelease doc](https://fluxcd.io/flux/components/helm/helmreleases/).
4. Dependency chains: `spec.dependsOn` (HelmRelease and Kustomization), including `readyExpr` CEL. Render as graph; mark blocked-by upstream ("DependencyNotReady").
5. Source to deployed mapping: HelmRelease -> `chart`/`chartRef` -> HelmChart/OCIRepository/HelmRepository -> Artifact; show `status.artifact.revision` (Git: `branch@sha1:<commit>`, OCI: tag@sha256 digest). Resolve to commit via Git provider link only if URL known; plain SHA display is the baseline.
6. Inventory: Kustomization `status.inventory.entries` (object IDs) and HelmRelease `status.inventory`; click through to live resources and their health.
7. Events: Kubernetes events filtered by `involvedObject` for the Flux kind plus those of the controllers' namespaces (flux-system); event stream from notification-controller sources are not needed.
8. Drift: HelmRelease `spec.driftDetection` mode (warn/enabled) and `Drifted` condition; Kustomization diff of cluster vs git (Capacitor does this; Flux Operator does not) [Capacitor](https://github.com/gimlet-io/capacitor).
9. Helm storage secrets: `sh.helm.release.v1.<name>.v<N>` Secrets (type `helm.sh/release.v1`) in `storageNamespace` (default = release namespace) hold gzip+base64(+base64) JSON release data: chart metadata, values, manifest, status, revision. Decoding them gives full history and manifests for diff between revisions. Flock must decode these client-side (Flux `history` only has summaries).
10. Actions: reconcile (optionally with source), force, reset, suspend, resume; Flux Operator UI also offers rollout restart. All must respect RBAC and show a clear error on 403 [Flux Web UI](https://fluxoperator.dev/web-ui/).
11. Controller health: pods and versions in `flux-system` per cluster; CRD API version mismatch warnings.
12. Cross-cluster compare: same HelmRelease name across kind/EKS/DOKS/GKE with chart version and status side by side. No competitor does this well; it is Flock's differentiator.

Notes on remote clusters: `spec.kubeConfig` on a HelmRelease (Flux managing another cluster) is a separate topology from "each cluster runs its own Flux"; model clusters as kubeconfig contexts, not as Flux targets.

## 4. Auth realities for desktop clients

- Exec credential plugins (client.authentication.k8s.io v1; v1beta1 still seen) are run per kubeconfig `users[].user.exec` by client-go semantics: `command`, `args`, `env`, `interactiveMode`, `provideClusterInfo`. Output is an `ExecCredential` with `status.token` and `expirationTimestamp`; clients must cache until expiry and re-run on 401. [k8s docs](https://kubernetes.io/docs/reference/access-authn-authz/authentication/#client-go-credential-plugins)
- Providers:
  - EKS: `aws eks get-token --cluster-name X [--region R] [--profile P]` or `aws-iam-authenticator`; needs the `aws` CLI on PATH and AWS env (profile, SSO cache, `AWS_*`). SSO sessions expire and the plugin errors until `aws sso login`; surface that error text.
  - GKE: `gke-gcloud-auth-plugin` on PATH, installed via `gcloud components install gke-gcloud-auth-plugin` [verified](https://cloud.google.com/blog/products/containers-kubernetes/kubectl-auth-changes-in-gke). Needs gcloud ADC / logged-in account.
  - DOKS: `doctl kubernetes cluster kubeconfig exec-credential` (kubeconfig written by `doctl kubernetes cluster kubeconfig save`); needs `doctl` on PATH with a configured token. The exec-credential subpage returned 404, so exact output format is [unverified]; the parent command page is [here](https://docs.digitalocean.com/reference/doctl/reference/kubernetes/cluster/kubeconfig/).
  - Azure/OIDC: `kubelogin get-token ...` (interactive browser, device code, or cached). `interactiveMode: IfAvailable/Always` plugins need a TTY or browser; Flock must handle prompts (spawn with a pty or open the browser; surface failures).
  - kind: client certificates in kubeconfig, no exec; works offline-ish and is the simplest baseline.
- macOS PATH problem: apps launched from Finder/Dock get the launchd minimal environment (`/usr/bin:/bin:/usr/sbin:/sbin`), not Homebrew (`/opt/homebrew/bin`), `~/google-cloud-sdk/bin`, mise/asdf shims, or anything in `.zshrc`. Result: "executable aws not found" the first time a user opens the .app. Lens users hit this and the documented workarounds are absolute paths in the kubeconfig command or checking Preferences > Terminal > shell path [FAQ search](https://docs.k8slens.dev/faq/technical-faq/) [unverified details].
- How Lens/Freelens solve it: Both run the user's login shell once to capture the environment and merge it into the app/child-process env (the "shell sync" step). Freelens has tracked this as issues #2378 (compute the shell environment once per app run instead of respawning a login shell per terminal tab), #2377 (make the sync child less likely to be flagged by EDR) and #2704 (expose host kubectl/helm and terminal env to extensions), closed in the v2.0.0 milestone [issues](https://github.com/freelensapp/freelens/issues?q=PATH+shell+env). Lens also bundles its own `kubectl`; Freelens downloads kubectl and supports a mirror URL for air-gapped use [releases](https://github.com/freelensapp/freelens/releases).
- Recommended design for Flock: capture env once at startup (and on explicit "refresh") via `$SHELL -l -i -c 'env -0'` with a short timeout, fall back to a static path list (`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `~/google-cloud-sdk/bin`), apply it to every exec-plugin spawn, and show the resolved PATH and plugin binary location in a diagnostics panel. Interactive-shell startup can hang or print junk (powerlevel10k prompts, `read` in rc files); use markers around the env dump and a timeout.

## 5. Implications for Flock

1. Position Flock as "the native, free Lens for Flux teams": native GPUI speed plus Flux depth that Lens gates behind the Plus tier ($25/mo) [pricing](https://lenshq.io/pricing) and that Freelens/Headlamp/k9s lack.
2. Differentiate with a cross-cluster Flux matrix (same HelmRelease across all clusters, versions, status). Capacitor and Flux Operator UI are per-cluster; Lens has nothing equivalent.
3. Build the generic engine on API discovery plus a CRD browser first (Flux is all CRDs). Pick API versions at runtime (helm v2, source v1, kustomize v1) and degrade gracefully on older clusters.
4. Flux actions are annotation patches (`requestedAt`, `forceAt`, `resetAt`) and `spec.suspend`; use merge patches, never full-object apply; implement "reconcile with source" as source-then-release.
5. Decode Helm storage secrets client-side for history, values and manifest diffs; this is what the Flux status alone cannot give and what Capacitor's Helm history/diff proves is valued.
6. Fix the exec-plugin PATH problem on day 1 (login-shell env capture once, timeout, static fallback, diagnostics panel). It will be the first support issue for EKS/GKE/DOKS users launching from Finder.
7. Handle credential lifecycle explicitly: cache ExecCredential until expiry, re-run on 401, and show actionable errors (expired AWS SSO, missing `gke-gcloud-auth-plugin`, doctl token) instead of generic connection failures.
8. Must-have core: kubeconfig discovery and watch, cluster list, workloads/config/network browsers, logs, pod exec, port-forward, YAML edit, events. Defer extensions, hotbar, AI, Trivy, cost, team features.
9. Metrics: ship metrics-server based CPU/memory first; add Prometheus auto-detection (label discovery plus API-server service proxy) later.
10. Licensing: choose a permissive license (Apache-2.0 or MIT like Freelens and Capacitor); avoid AGPL entanglement and do not copy from Flux Operator UI (AGPL-3.0) code.

## Open items / unverified
- Lens Plus GitOps feature depth; whether the free tier shows Flux resources at all (only the plan table was verified).
- Aptakube/Seabird implementation stack and Seabird multi-cluster behavior.
- Exact string `reconcile.fluxcd.io/resetAt`; doctl exec-credential output.
- Flux Operator UI: true multi-cluster aggregation vs per-cluster install.
- Freelens Flux extension availability (repo page silent).
