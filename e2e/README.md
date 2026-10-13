# Fleet live scenarios

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
kubeconfig export can change the current context; Fleet can select both named
contexts independently. Keep the clusters running for desktop testing.

Flux reconciles asynchronously. `up` runs `e2e/verify` after applying both
clusters. You can also run `./e2e/verify` independently. It waits for healthy
objects, expected failures, the suspended spec, metrics availability, and
pod/event evidence; it exits unsuccessfully if the expected states do not
appear. `status` prints
`flux get all -A` for each cluster. Allow several minutes for image downloads,
Helm installation timeouts, and crash backoff on the first run.

| Scenario | fleet-a | fleet-b | Fleet screens |
| --- | --- | --- | --- |
| `HelmRelease/apps/podinfo` | Ready | Ready | Fleet home, HelmRelease page, GitOps grid |
| `HelmRelease/payments/checkout-api` | Not Ready; pod crashes on `--fleet-invalid-flag` | Ready; no extra arguments | Fleet home, Workload logs/events, HelmRelease page, GitOps grid |
| `Deployment/payments/stripe-webhook` | CreateContainerConfigError | CreateContainerConfigError | Fleet home, Workload environment/events |
| `HelmRelease/search/search-indexer` | Suspended | Suspended | HelmRelease page, GitOps grid |
| `Kustomization/flux-system/apps` | Ready; podinfo deployed into `web` | Ready; podinfo deployed into `web` | Kustomization page and inventory, Changes, GitOps grid |
| `Kustomization/flux-system/broken-kustomization` | ArtifactFailed; nonexistent path | ArtifactFailed; nonexistent path | Fleet home, Kustomization page, GitOps grid |
| `Pod/apps/unschedulable` | Pending; FailedScheduling | Pending; FailedScheduling | Fleet home, Workload/pod events |

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
`./fleet-nonexistent-path`. Sources and generated HelmCharts should be Ready on
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

To remove both clusters when desktop testing is finished:

```sh
mise run e2e:down
```

This deletes only `fleet-a` and `fleet-b` and their kind kubeconfig entries.
