use std::collections::BTreeMap;

use fleet_core::gitops::{FluxInstall, FluxKind, Source, SyncCell, SyncGroup, SyncRow};
use fleet_core::{ClusterId, GitOps, Status};
use fleet_flux::matrix::{self, ClusterObjects};
use fleet_flux::revision::short;
use fleet_flux::{ObjectIdentity, ObjectKind, Summary, redact};
use jiff::Timestamp;
use serde_json::Value;

use super::convert::{Parsed, core_status, elapsed, git_reference, note, summary, timestamp};
use super::watch::FluxInfo;

const SEPARATOR: &str = " · ";
const SOURCE_KINDS: [ObjectKind; 3] = [
    ObjectKind::GitRepository,
    ObjectKind::HelmRepository,
    ObjectKind::OCIRepository,
];

pub struct ClusterFluxView<'a> {
    pub id: &'a ClusterId,
    pub online: bool,
    pub objects: &'a [Parsed],
    pub info: &'a FluxInfo,
}

pub fn gitops(clusters: &[ClusterFluxView<'_>], now: Timestamp) -> GitOps {
    let flux: Vec<&ClusterFluxView<'_>> = clusters.iter().filter(|c| c.info.detected()).collect();
    let matrix_input: Vec<ClusterObjects> = flux
        .iter()
        .map(|cluster| ClusterObjects {
            cluster: cluster.id.to_string(),
            online: cluster.online,
            objects: cluster.objects.iter().map(|p| p.object.clone()).collect(),
        })
        .collect();
    let rows = match matrix::rows(&matrix_input) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "GitOps grid could not be built");
            Vec::new()
        }
    };
    let groups = [FluxKind::Kustomization, FluxKind::HelmRelease]
        .into_iter()
        .map(|kind| group(kind, &rows, &flux))
        .collect();
    GitOps {
        clusters: flux.iter().map(|c| c.id.clone()).collect(),
        sources: sources(&flux, now),
        groups,
        installs: flux.iter().map(|c| install(c)).collect(),
    }
}

const fn object_kind(kind: FluxKind) -> ObjectKind {
    match kind {
        FluxKind::Kustomization => ObjectKind::Kustomization,
        FluxKind::HelmRelease => ObjectKind::HelmRelease,
    }
}

fn group(kind: FluxKind, rows: &[matrix::Row], clusters: &[&ClusterFluxView<'_>]) -> SyncGroup {
    let rows: Vec<SyncRow> = rows
        .iter()
        .filter(|row| row.identity.kind == object_kind(kind))
        .map(|row| sync_row(kind, row, clusters))
        .collect();
    SyncGroup {
        kind,
        total: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        rows,
    }
}

fn find<'a>(cluster: &ClusterFluxView<'a>, identity: &ObjectIdentity) -> Option<&'a Parsed> {
    cluster
        .objects
        .iter()
        .find(|parsed| &parsed.object.identity == identity)
}

fn sync_row(kind: FluxKind, row: &matrix::Row, clusters: &[&ClusterFluxView<'_>]) -> SyncRow {
    let present: Vec<(usize, &Parsed)> = clusters
        .iter()
        .enumerate()
        .filter_map(|(ix, cluster)| find(cluster, &row.identity).map(|parsed| (ix, parsed)))
        .collect();
    let majority = majority(row);
    let cells = row
        .cells
        .iter()
        .enumerate()
        .map(|(ix, cell)| {
            let parsed = present.iter().find(|(at, _)| *at == ix).map(|(_, p)| *p);
            let cluster = clusters.get(ix).copied();
            match (parsed, cluster) {
                (Some(parsed), Some(cluster)) => {
                    let summary = summary(parsed, cluster.objects);
                    sync_cell(cell, &summary, parsed, majority.as_deref())
                }
                (None, _) => SyncCell::not_deployed(),
                (_, None) => SyncCell::not_deployed(),
            }
        })
        .collect();
    let detail = present
        .first()
        .map(|(_, parsed)| row_detail(parsed))
        .unwrap_or_default();
    SyncRow {
        kind,
        namespace: row.identity.namespace.clone(),
        name: row.identity.name.clone(),
        detail,
        cells,
    }
}

fn majority(row: &matrix::Row) -> Option<String> {
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for revision in row.cells.iter().filter_map(|cell| cell.revision.as_deref()) {
        *tally.entry(revision).or_default() += 1;
    }
    let best = tally.values().copied().max()?;
    let leaders: Vec<&str> = tally
        .iter()
        .filter(|(_, count)| **count == best)
        .map(|(revision, _)| *revision)
        .collect();
    match leaders.as_slice() {
        [only] => Some((*only).to_owned()),
        _ => None,
    }
}

fn sync_cell(
    cell: &matrix::Cell,
    summary: &Summary,
    parsed: &Parsed,
    majority: Option<&str>,
) -> SyncCell {
    let behind =
        cell.differs && majority.is_some_and(|leader| cell.revision.as_deref() != Some(leader));
    let revision = cell
        .revision
        .as_deref()
        .or(cell.attempted_revision.as_deref())
        .map(short)
        .unwrap_or_default();
    SyncCell {
        status: Some(core_status(summary.status)),
        revision,
        note: note(summary, &parsed.object),
        behind,
    }
}

fn row_detail(parsed: &Parsed) -> String {
    let spec = &parsed.object.spec;
    match parsed.kind() {
        ObjectKind::HelmRelease => {
            let chart = spec
                .pointer("/chart/spec/chart")
                .or_else(|| spec.pointer("/chartRef/name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!(
                "{}{SEPARATOR}chart {chart}",
                parsed.object.identity.namespace
            )
        }
        ObjectKind::Kustomization => spec
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        ObjectKind::GitRepository => String::new(),
        ObjectKind::HelmRepository => String::new(),
        ObjectKind::HelmChart => String::new(),
        ObjectKind::OCIRepository => String::new(),
    }
}

fn sources(clusters: &[&ClusterFluxView<'_>], now: Timestamp) -> Vec<Source> {
    let mut found: BTreeMap<&ObjectIdentity, Vec<(&ClusterFluxView<'_>, &Parsed)>> =
        BTreeMap::new();
    for cluster in clusters {
        let kept = cluster
            .objects
            .iter()
            .filter(|parsed| SOURCE_KINDS.contains(&parsed.kind()));
        for parsed in kept {
            found
                .entry(&parsed.object.identity)
                .or_default()
                .push((cluster, parsed));
        }
    }
    found
        .into_iter()
        .filter_map(|(identity, copies)| {
            let (_, first) = copies.first()?;
            let ready = copies
                .iter()
                .filter(|(cluster, parsed)| {
                    core_status(summary(parsed, cluster.objects).status) == Status::Ok
                })
                .count();
            let fetched = copies
                .iter()
                .filter_map(|(_, parsed)| {
                    timestamp(parsed.raw.pointer("/status/artifact/lastUpdateTime"))
                })
                .max()
                .map(|then| elapsed(then, now))
                .unwrap_or_default();
            let reference =
                (identity.kind == ObjectKind::GitRepository).then(|| git_reference(first));
            Some(Source {
                kind: identity.kind.as_str().to_owned(),
                namespace: identity.namespace.clone(),
                name: identity.name.clone(),
                url: redact::credentials(
                    first
                        .object
                        .spec
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                reference,
                revision: first
                    .object
                    .artifact_revision
                    .as_deref()
                    .map(short)
                    .unwrap_or_default(),
                fetched,
                ready: u32::try_from(ready).unwrap_or(u32::MAX),
                clusters: u32::try_from(copies.len()).unwrap_or(u32::MAX),
            })
        })
        .collect()
}

fn install(cluster: &ClusterFluxView<'_>) -> FluxInstall {
    FluxInstall {
        cluster: cluster.id.clone(),
        version: cluster.info.version.clone().unwrap_or_default(),
        upgrade: None,
        controllers: cluster.info.controllers.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitops::convert::tests::{git_repository, kustomization, parsed};

    fn info() -> FluxInfo {
        FluxInfo {
            kinds: vec![ObjectKind::Kustomization],
            unserved: BTreeMap::new(),
            version: Some("v2.9.6".to_owned()),
            controllers: Vec::new(),
        }
    }

    #[test]
    fn grid_lines_up_clusters_and_flags_the_odd_revision() {
        let a = ClusterId::new("fleet-a");
        let b = ClusterId::new("fleet-b");
        let c = ClusterId::new("fleet-c");
        let on_a = [
            parsed(&kustomization("apps", "True", "3e0ff8ae123b")),
            parsed(&kustomization("broken", "False", "3e0ff8ae123b")),
            parsed(&git_repository()),
        ];
        let on_b = [parsed(&kustomization("apps", "True", "3e0ff8ae123b"))];
        let on_c = [parsed(&kustomization("apps", "True", "0000000aaaaa"))];
        let info = info();
        let clusters = [
            ClusterFluxView {
                id: &a,
                online: true,
                objects: &on_a,
                info: &info,
            },
            ClusterFluxView {
                id: &b,
                online: true,
                objects: &on_b,
                info: &info,
            },
            ClusterFluxView {
                id: &c,
                online: true,
                objects: &on_c,
                info: &info,
            },
        ];
        let now = "2026-10-13T10:00:00Z".parse().unwrap_or_default();
        let grid = gitops(&clusters, now);
        assert_eq!(grid.clusters, [a, b, c]);
        let kustomizations = &grid.groups[0];
        assert_eq!(kustomizations.total, 2);
        let apps = &kustomizations.rows[0];
        assert_eq!(apps.detail, "./kustomize");
        let cells: Vec<(Option<Status>, &str, bool)> = apps
            .cells
            .iter()
            .map(|cell| (cell.status, cell.revision.as_str(), cell.behind))
            .collect();
        assert_eq!(
            cells,
            [
                (Some(Status::Ok), "3e0ff8a", false),
                (Some(Status::Ok), "3e0ff8a", false),
                (Some(Status::Ok), "0000000", true),
            ]
        );
        let broken = &kustomizations.rows[1];
        assert_eq!(broken.cells[0].note, "health check failed");
        assert!(!broken.cells[1].is_deployed());
        assert_eq!(grid.sources.len(), 1);
        assert_eq!(grid.sources[0].ready_label(), "● 1/1");
        assert_eq!(grid.sources[0].reference.as_deref(), Some("master"));
        assert_eq!(grid.installs[0].version, "v2.9.6");
    }
}
