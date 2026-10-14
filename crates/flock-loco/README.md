# flock-loco

Pure, synchronous adapters for the Loco Applications, Builds, Routes and Platform screens. The
application opts into this crate through its `loco` feature; this crate has no feature gate or
runtime. Live watchers belong to `flock-kube`, and UI snapshots must cross the `flock-core`
contract rather than importing this crate from `flock-ui`.

`generated` exports kube-rs Application and Build resources. Unknown phase strings deserialize to
`Unknown`. `applications` exposes workspace grouping, placement revision comparison, application
summaries and related-object discovery. `builds` exposes phase, timestamps, duration, digests and
UID-verified Job/Pod references. `routes` decodes the Gateway/HTTPRoute fields used by the screens,
combines routes with complete Service/EndpointSlice snapshots, and sorts problems first.
`platform` exports the installed component catalog, workload health adapters, component health
rollups and agent discovery observations. Cluster input errors use the crate's `Error` enum.

Regenerate CRDs and Rust types with `mise run gen:loco`. `LOCO_CORE` defaults to
`../core` (a loco checkout next to this repo); the task only reads that checkout and copies
`controller/config/crd/bases` into Flock. It runs pinned kopium and rustfmt. The two phase enums
are maintained in `generated/mod.rs` so new server phases remain readable. Do not edit generated
`applications.rs` or `builds.rs` manually.

## Observable in Kubernetes

- Application spec, image, deployment/environment IDs, routing, replica counts, phase, status
  message and the controller's exact `Degraded` condition message.
- Desired placement revision from `loco.io/placement-revision` on the Application and the
  controller's `status.observedPlacementRevision`. The latter records controller observation,
  not a separate agent acknowledgment. Unapplied revisions rejected before CR creation cannot
  be discovered from the Application alone.
- Application workloads are in `ws-<spec.workspaceId>`; Application CRs are in the agent's
  namespace, normally `loco-system`. Workload names are `resource-<spec.resourceId>`.
  Cross-namespace resources carry `loco.io/application-name` / `application-namespace`
  annotations and `app` / `app.kubernetes.io/managed-by` labels. Staging env Secrets use same
  namespace Application UID owner references. Workspace NetworkPolicies carry workspace and
  managed-by labels and are shown as shared, not owned exclusively by an Application. Secret
  values are never read by these adapters.
- Build phase, start/finish timestamps, image/cache digests, requested cache reference and
  message. Jobs are in `loco-builds`, named exactly like the Build CR, with Build UID ownership;
  Pods have Job ownership and `loco.io/build-id` / managed-by labels. The controller creates
  `fetch` and `build` init containers plus a `push` container. Names are taken from live objects;
  expired Jobs and Pods remain absent.
- Gateway `eg` listeners, certificate references, attached-route counts and current conditions;
  HTTPRoute conditions for that gateway; backend Service existence and EndpointSlice readiness.
  Provide complete snapshots from all referenced namespaces. A missing normal Service's
  EndpointSlices means zero endpoints; ExternalName services remain unknown. Pod UID deduplication
  avoids counting IPv4/IPv6 slices twice. Zero-weight backends are excluded.
- Workload readiness, current generation, rollout state and container images. Catalog defaults
  follow core's `helmfile.yaml.gotmpl`, chart templates and values, including `loco-core-agent`,
  `loco-controller`, `loco-build-controller`, Envoy Gateway/proxy in `loco-system`, cert-manager,
  Cilium, both OTel collectors in `observability`, ClickHouse pod labels and `loco-obs-obs-proxy`.
  Envoy proxy names and ClickHouse pod names are generated, so the catalog selects their labels.
  Callers may adjust catalog namespaces and selectors for customized Helm installations.
- Build support is discovered by the served `builds` API resource under `infra.loco.io/v1alpha1`,
  exactly as `agent/pkg/cluster/cluster.go` does. A failed/unavailable discovery is unknown.

## Not persisted in Kubernetes

The agent sends heartbeat health/capacity and `builds_enabled` to the control plane over its Sync
stream (`agent/main.go`). It does not create a heartbeat Lease or status CR. Connected state,
last heartbeat, uptime, reconnect count/history, reported Kubernetes/controller health, agent
queue length, retry deadline and rejected placement details cannot be reconstructed from resource
readiness. `AgentSync` explicitly returns `Observation::Unavailable` for those fields; its build
support value describes current API discovery rather than the agent's last report.

Build cache digest or cache request does not reveal cache-hit percentage. `CacheHit::Unavailable`
is explicit. Friendly workspace/application names, source commits, retention/renewal dates and
agent/build log contents are not supplied by these CRs. Logs require separate Pod log access;
certificate expiry requires certificate resources. No placeholder telemetry is fabricated.
