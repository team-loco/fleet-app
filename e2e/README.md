# Flock live scenarios

These fixtures create two local kind clusters, `fleet-a` and `fleet-b`, with
contexts `kind-fleet-a` and `kind-fleet-b` in the default kubeconfig. Each has
Flux and metrics-server, with `--kubelet-insecure-tls` for kind's kubelet
certificates. Docker must be running. Tool versions come from `mise.toml`.

```sh
mise trust && mise install
mise run e2e:up
mise run e2e:status
```

`up` reuses existing clusters, exports their contexts, and reapplies the
fixtures. Every operation selects its context explicitly. Cluster creation or
kubeconfig export can change the current context; Flock can select both named
contexts independently. Keep the clusters running for desktop testing.

Flux reconciles asynchronously. `up` runs `e2e/verify` after applying both
clusters. You can also run `./e2e/verify` independently. It waits for healthy
objects, expected failures, the suspended spec, metrics availability, and
pod/event evidence; it exits unsuccessfully if the expected states do not
appear. `status` prints
`flux get all -A` for each cluster. Allow several minutes for image downloads,
Helm installation timeouts, and crash backoff on the first run.

| Scenario | fleet-a | fleet-b | Flock screens |
| --- | --- | --- | --- |
| `HelmRelease/apps/podinfo` | Ready | Ready | Flock home, HelmRelease page, GitOps grid |
| `HelmRelease/payments/checkout-api` | Not Ready; pod crashes on `--flock-invalid-flag` | Ready; no extra arguments | Flock home, Workload logs/events, HelmRelease page, GitOps grid |
| `Deployment/payments/stripe-webhook` | CreateContainerConfigError | CreateContainerConfigError | Flock home, Workload environment/events |
| `HelmRelease/search/search-indexer` | Suspended | Suspended | HelmRelease page, GitOps grid |
| `Kustomization/flux-system/apps` | Ready; podinfo deployed into `web` | Ready; podinfo deployed into `web` | Kustomization page and inventory, Changes, GitOps grid |
| `Kustomization/flux-system/broken-kustomization` | ArtifactFailed; nonexistent path | ArtifactFailed; nonexistent path | Flock home, Kustomization page, GitOps grid |
| `Pod/apps/unschedulable` | Pending; FailedScheduling | Pending; FailedScheduling | Flock home, Workload/pod events |

The checkout release's only cluster-specific change is the extra invalid
argument in `scenarios/fleet-a/checkout-api.yaml`. The chart's `extraArgs` value
is appended to podinfo's command, so the process exits and Kubernetes restarts
it. Flux keeps the failed installation for inspection and retries without
uninstalling it. Both releases explicitly use the name `checkout-api`.

The dummy `stripe` secret contains `webhook-secret`, while `stripe-webhook`
requests `webhook_secret` through `STRIPE_WEBHOOK_SECRET` with `optional: false`.
This deliberately reproduces a configuration error at container creation;
Kubernetes cannot start the process, so this workload has events but no process
logs. The separate checkout scenario provides crash logs.

The public HelmRepository is `https://stefanprodan.github.io/podinfo`. The public
GitRepository tracks `master` at `https://github.com/stefanprodan/podinfo`.
The healthy Kustomization builds `./kustomize`; the broken one builds
`./flock-nonexistent-path`. Sources and generated HelmCharts should be Ready on
both clusters. The suspended release has no deployed workload or Ready
condition until resumed.

Changes can show the public Git source revision and upstream commit history.
Local scenario overlays are applied with kubectl and are not commits in that
source: this setup does not reproduce the design's fictional checkout commit
or its Git diff. GitOps grid comparison shows the differing Helm outcomes for
the same checkout chart and the matching healthy Git revision.

Upstream charts, Git branch, container tags, and metrics-server's release URL
follow their published defaults; an upstream change can alter the fixtures.
The verification script catches changes that break the expected states.

`mise run e2e:test` is the one-command proof. It runs `e2e:up` (reusing existing
clusters), `e2e/verify`, then every `FLOCK_E2E` test with and without
`--features flock/loco`, against a temporary kubeconfig that holds only
`kind-fleet-a` and `kind-fleet-b`. On macOS it also launches the real `flock`
binary with a seeded data directory and waits for both clusters and the
scenario problems to appear in a live snapshot (the app logs these when
`FLOCK_LOG_SNAPSHOTS` is set). `--skip-up` requires existing clusters;
`--no-teardown` keeps them (the default, except when `CI` is set).

The live UI journeys in `apps/flock-shots/tests/journeys.rs` are part of that run. They open the
headless Flock shell on a `LiveBackend` with both clusters added to a temporary data directory,
then visit every page, tab, inspector, row menu and shortcut and check that counts, badges and
pages stay loaded. Two of them change the clusters and put them back: one resumes and suspends
`HelmRelease/search/search-indexer` on fleet-a, then uninstalls what Flux installed on resume and
recreates the HelmRelease from its saved spec, so it is back to never-deployed (also when the test
fails); the
other creates and deletes the ConfigMap `default/flock-qa-live` on fleet-b. Render every live page
for review with
`FLOCK_E2E=1 mise exec -- cargo nextest run -p flock-shots --test journeys --run-ignored all live_pages_render_for_review`;
the PNGs land in `target/qa/`.

To remove both clusters when desktop testing is finished:

```sh
mise run e2e:down
```

This deletes only `fleet-a` and `fleet-b` and their kind kubeconfig entries.

## Scale and snapshot performance

`mise run e2e:scale` adds a disposable `flock-scale` namespace to **kind-fleet-b only**.
The task obtains a temporary kubeconfig directly from kind, uses an explicit context on every
kubectl call, and never loads the user's kubeconfig. It creates 3,000 ConfigMaps with 1 KiB payloads,
1,500 headless Services, 400 zero-replica Deployments (and their controller-created ReplicaSets),
2,000 Events and 200 pause Pods. Pods above the node's pod capacity can remain Pending; they still
exercise the real pod watcher. Eight bounded apply workers keep setup practical. Re-running is
idempotent. The image is pinned in `mise.toml`.

Run `mise run e2e:scale:bench` after setup. The ignored release tests measure selected-cluster page
rebuilds, live heap freed when watcher stores and tasks are released, and pod label patches through
the real LiveBackend channel into the UI's `AppState`. Twelve isolated probes, one second apart, must
reach `AppState` with a median under 300 ms including the API request; their maximum after the API
acknowledged the patch must stay under 1 s, so a stalled API server on a loaded host does not fail
the run. A ten-second storm of eight concurrent pod patchers then checks that snapshots stay at most
one per snapshot interval while probes still get through. Each line prints the load average. A separate
heap test starts the LiveBackend on kind-fleet-b and reports the time to the first Pods and
Deployments counts after connect, then the process heap and the API server's watch count (the sum of
`apiserver_longrunning_requests` with `verb="WATCH"`, minus the count before Flock connected) after
connect and on the ConfigMaps page. This
measures state availability, not a rendered gpui frame. The API server drops Events after its one-hour
event TTL, so re-run `mise run e2e:scale` before benchmarking an older scenario. The heap test uses [dhat](https://docs.rs/dhat/latest/dhat/) in its own profiling scope;
timing measurements run without profiling. RSS is also reported, but includes clients and runtime
memory and is not used as a store heap measurement. Run these tests alone to avoid CPU contention.

`mise run bench:snapshot` needs no cluster. It builds 10,000 realistic pod view models and their
overview index, plus 5,000 metadata-only ConfigMap rows. It reports medians and maxima over 40 samples
and enforces a 20 ms median rebuild budget. Allocation budgets catch temporary string cloning even
when wall-clock measurements are noisy. Normal `mise run check` leaves these timing tests ignored.
The generator's boundary tests run with
`mise exec -- python3 -m unittest discover -s e2e -p test_scale.py`.

Always finish with `mise run e2e:scale:down`, then `e2e/verify` using a temporary kubeconfig containing
only the two kind contexts. Teardown deletes only `flock-scale` on kind-fleet-b and preserves the
normal Flux scenarios. Benchmark setup fails if kind-fleet-b or the scale objects are missing.
