# flock-flux

Pure Flux interpretation for backends. No Kubernetes client calls or async runtime are used.

`generated` exports the six stable Flux resource types. Run `mise run gen:flux` to regenerate
from `flux install --export` using the CLI and kopium pinned in `mise.toml`. No live cluster
or YAML parser dependency is needed.

`GitOpsObject::from_json` validates stable API versions and resource shapes before normalizing
identity, conditions, suspension, references, dependencies (including CEL expressions),
generations, interval, handled reconcile token, inventory and release history. `TryFrom<&T>`
accepts each generated resource type. Unknown fields are ignored; malformed fields and
unsupported API versions return `ObjectError` for the caller to display.

`revision()` returns the applied Kustomization revision, latest deployed Helm chart version,
or source artifact revision. Attempted revisions remain separate so a failed upgrade does
not appear deployed. Older HelmRelease objects without inventory return an empty inventory;
history snapshots provide release metadata, not managed object lists.

`summarize(object, optional_source)` returns a local `Status`, a plain sentence and controller
detail. It handles suspension, stale conditions, stalled reconciliation, source and dependency
failures, Helm remediation and retry budgets, health checks, builds and drift. Source state is
used only when its identity matches the object's reference. Missing readiness is pending;
explicit unknown readiness is offline. Controller messages remain intact in `detail`.

`helm_storage::decode_secret` accepts a Kubernetes Secret whose outer base64 layer is already
decoded by k8s-openapi. `decode_secret_json` accepts serialized Kubernetes JSON with both
base64 layers. Both decode Helm's inner base64, gzip and release JSON, validate secret name
against release identity and revision, and bound decompressed data. The release namespace
may differ from the storage namespace. `revision_history` validates and orders one release's
secrets; `manifest_diff` returns a complete unified manifest diff.

`actions::{reconcile, force, reset, suspend, resume}` build JSON merge patches. The caller
supplies a fresh timestamp for every request. Force and reset use the same token as requestedAt;
they apply to HelmRelease. The backend handles RBAC, confirmation and source-first ordering.

`matrix::rows` groups exact kind/namespace/name identities and preserves input cluster order.
Cells distinguish absent objects from offline clusters and retain applied and attempted
revisions. `differs` marks unequal known revisions among online clusters; it does not infer
which revision is newer. Duplicate cluster names or objects return `MatrixError`.

Reason and reset annotation verification uses the controller tags exported by the pinned CLI:
[Helm conditions](https://github.com/fluxcd/helm-controller/blob/v1.6.5/api/v2/condition_types.go),
[Helm reset and force](https://github.com/fluxcd/helm-controller/blob/v1.6.5/api/v2/annotations.go),
[Helm retries](https://github.com/fluxcd/helm-controller/blob/v1.6.5/internal/reconcile/atomic_release.go),
and [Kustomization failures](https://github.com/fluxcd/kustomize-controller/blob/v1.9.6/internal/controller/kustomization_controller.go).
These links document verification provenance; generation always reads the tool pin.

JSON fixtures cover controller state and self-generated Helm storage secrets. Run
`mise exec -- cargo nextest run -p flock-flux` for tests without a live cluster.
