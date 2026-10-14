use std::time::Duration;

use flock_core::gitops::{Controller, FluxInstall, FluxKind, Source, SyncCell, SyncGroup, SyncRow};
use flock_core::{ClusterId, GitOps, Status};

const MINUTE: u64 = 60;
const HEAD: &str = "a41e07c";
const HELD_AT: &str = "7c2d9e0";
const FLUX_NAMESPACE: &str = "flux-system";
const FLUX_VERSION: &str = "v2.4.0";
const CLUSTERS: [&str; 4] = ["dev-kind", "staging", "prod-us-east", "prod-eu-west"];
const KUSTOMIZATIONS_TOTAL: u32 = 12;
const HELM_RELEASES_TOTAL: u32 = 9;
const SOURCE_CLUSTERS: u32 = 4;
const CONTROLLERS: [&str; 4] = ["source", "kustomize", "helm", "notification"];
const IMAGE_AUTOMATION: &str = "image-automation";

pub fn gitops() -> GitOps {
    GitOps {
        clusters: CLUSTERS.into_iter().map(ClusterId::new).collect(),
        sources: sources(),
        groups: vec![
            SyncGroup {
                kind: FluxKind::Kustomization,
                total: KUSTOMIZATIONS_TOTAL,
                rows: kustomizations(),
            },
            SyncGroup {
                kind: FluxKind::HelmRelease,
                total: HELM_RELEASES_TOTAL,
                rows: helm_releases(),
            },
        ],
        installs: installs(),
    }
}

fn cell(status: Status, revision: &str, note: &str, behind: bool) -> SyncCell {
    SyncCell {
        status: Some(status),
        revision: revision.into(),
        note: note.into(),
        behind,
    }
}

fn ready(revision: &str) -> SyncCell {
    cell(Status::Ok, revision, "ready", false)
}

fn none() -> SyncCell {
    SyncCell::not_deployed()
}

fn everywhere(revision: &str) -> Vec<SyncCell> {
    CLUSTERS.iter().map(|_| ready(revision)).collect()
}

fn row(kind: FluxKind, namespace: &str, name: &str, detail: &str, cells: Vec<SyncCell>) -> SyncRow {
    SyncRow {
        label_sets: Vec::new(),
        kind,
        namespace: namespace.into(),
        name: name.into(),
        detail: detail.into(),
        cells,
    }
}

fn kustomization(name: &str, detail: &str, cells: Vec<SyncCell>) -> SyncRow {
    row(FluxKind::Kustomization, FLUX_NAMESPACE, name, detail, cells)
}

fn kustomizations() -> Vec<SyncRow> {
    vec![
        kustomization(
            "infra-controllers",
            "./infrastructure/controllers",
            everywhere(HEAD),
        ),
        kustomization(
            "infra-configs",
            "dependsOn infra-controllers",
            everywhere(HEAD),
        ),
        kustomization(
            "apps-payments",
            "./apps/payments/overlays/<cluster>",
            vec![
                cell(Status::Ok, HEAD, "ready · 3h", false),
                cell(Status::Ok, HEAD, "ready · 3h", false),
                cell(Status::Err, HEAD, "health check failed", false),
                cell(Status::Suspended, HELD_AT, "suspended 2d · 2 behind", true),
            ],
        ),
        kustomization(
            "apps-web",
            "./apps/web/overlays/<cluster>",
            everywhere(HEAD),
        ),
        kustomization(
            "apps-search",
            "./apps/search/overlays/<cluster>",
            vec![
                none(),
                cell(Status::Progress, HEAD, "waiting on search-indexer-0", false),
                ready(HEAD),
                ready(HEAD),
            ],
        ),
        kustomization(
            "apps-ml",
            "./apps/ml",
            vec![
                none(),
                none(),
                cell(Status::Ok, HEAD, "ready · 1 pod pending", false),
                none(),
            ],
        ),
    ]
}

fn helm_release(namespace: &str, name: &str, detail: &str, cells: Vec<SyncCell>) -> SyncRow {
    row(FluxKind::HelmRelease, namespace, name, detail, cells)
}

fn helm_releases() -> Vec<SyncRow> {
    let prometheus_behind = cell(Status::Ok, "61.9.0", "ready", true);
    vec![
        helm_release(
            "ingress-nginx",
            "ingress-nginx",
            "ingress-nginx · chart 4.11.2",
            everywhere("4.11.2"),
        ),
        helm_release(
            "cert-manager",
            "cert-manager",
            "cert-manager · jetstack",
            everywhere("v1.15.3"),
        ),
        helm_release(
            "monitoring",
            "kube-prometheus-stack",
            "monitoring · prometheus-community",
            vec![
                prometheus_behind.clone(),
                cell(Status::Warn, "62.7.0", "upgrade retry 2/3 · timeout", false),
                prometheus_behind.clone(),
                prometheus_behind,
            ],
        ),
        helm_release(
            "external-secrets",
            "external-secrets",
            "external-secrets · 0.10.4",
            everywhere("0.10.4"),
        ),
        helm_release(
            "kube-system",
            "karpenter",
            "kube-system · EKS only",
            vec![none(), none(), ready("1.0.6"), ready("1.0.6")],
        ),
    ]
}

fn source(
    kind: &str,
    name: &str,
    url: &str,
    reference: Option<&str>,
    revision: &str,
    fetched_minutes: u64,
) -> Source {
    Source {
        kind: kind.into(),
        namespace: FLUX_NAMESPACE.into(),
        name: name.into(),
        url: url.into(),
        reference: reference.map(Into::into),
        revision: revision.into(),
        fetched: Duration::from_secs(fetched_minutes * MINUTE),
        ready: SOURCE_CLUSTERS,
        clusters: SOURCE_CLUSTERS,
    }
}

fn sources() -> Vec<Source> {
    vec![
        source(
            "GitRepository",
            "fleet-infra",
            "ssh://git@github.com/acme/fleet-infra",
            Some("main"),
            HEAD,
            4,
        ),
        source(
            "HelmRepository",
            "prometheus-community",
            "https://prometheus-community.github.io/helm-charts",
            None,
            "index 2026-10-12",
            12,
        ),
        source(
            "OCIRepository",
            "ingress-nginx",
            "oci://ghcr.io/acme/charts/ingress-nginx",
            Some("4.11.x"),
            "4.11.2",
            12,
        ),
    ]
}

fn install(cluster: &str, version: &str, upgrade: Option<&str>, automation: bool) -> FluxInstall {
    let core = CONTROLLERS.iter().map(|name| Controller {
        name: (*name).into(),
        status: Some(Status::Ok),
    });
    let automation = Controller {
        name: IMAGE_AUTOMATION.into(),
        status: automation.then_some(Status::Ok),
    };
    FluxInstall {
        cluster: ClusterId::new(cluster),
        version: version.into(),
        upgrade: upgrade.map(Into::into),
        controllers: core.chain(std::iter::once(automation)).collect(),
    }
}

fn installs() -> Vec<FluxInstall> {
    vec![
        install("dev-kind", "v2.3.0", Some(FLUX_VERSION), false),
        install("staging", FLUX_VERSION, None, true),
        install("prod-us-east", FLUX_VERSION, None, true),
        install("prod-eu-west", FLUX_VERSION, None, true),
    ]
}

#[cfg(test)]
mod tests {
    use flock_core::filter::TextFilter;
    use flock_core::gitops::{SyncFilter, visible_groups};

    use super::*;

    #[test]
    fn every_row_has_a_cell_per_cluster() {
        let gitops = gitops();
        for group in &gitops.groups {
            for row in &group.rows {
                assert_eq!(row.cells.len(), gitops.clusters.len(), "{}", row.name);
            }
        }
    }

    #[test]
    fn out_of_sync_matches_the_design() {
        let gitops = gitops();
        let visible = visible_groups(
            &gitops.groups,
            SyncFilter::OutOfSync,
            &TextFilter::default(),
        );
        let names: Vec<Vec<&str>> = visible
            .iter()
            .map(|g| g.rows.iter().map(|r| r.name.as_str()).collect())
            .collect();
        assert_eq!(
            names,
            [
                vec!["apps-payments", "apps-search"],
                vec!["kube-prometheus-stack"]
            ]
        );
        assert_eq!(visible[0].count_label(), "2 of 12");
    }
}
