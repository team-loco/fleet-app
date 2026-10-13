mod convert;
mod focus;
mod grid;
mod health;
mod watch;

use std::collections::BTreeMap;
use std::sync::Arc;

use fleet_core::{
    ClusterId, Command, Counts, FluxKey, FluxKind, Page, ResourceRef, Snapshot, Status,
};
use fleet_flux::ObjectKind;
use jiff::Timestamp;

use crate::manager::{ClusterHandle, ConnectionStatus, lock};
use convert::{Inputs, Parsed, core_status, flux_object, parse, summary};
use focus::Focus;
use grid::ClusterFluxView;
use health::Workloads;
use watch::{ClusterFlux, FluxInfo, KINDS};

const CHANGE_BUFFER: usize = 1;

pub struct GitOpsState {
    clusters: BTreeMap<ClusterId, ClusterFlux>,
    focus: Option<Focus>,
    changed_tx: async_channel::Sender<()>,
    changed_rx: async_channel::Receiver<()>,
}

impl Default for GitOpsState {
    fn default() -> Self {
        let (changed_tx, changed_rx) = async_channel::bounded(CHANGE_BUFFER);
        Self {
            clusters: BTreeMap::new(),
            focus: None,
            changed_tx,
            changed_rx,
        }
    }
}

struct ClusterData {
    id: ClusterId,
    online: bool,
    objects: Vec<Parsed>,
    info: FluxInfo,
}

impl GitOpsState {
    pub fn changed(&self) -> async_channel::Receiver<()> {
        self.changed_rx.clone()
    }

    pub fn sync<'a>(
        &mut self,
        clusters: impl IntoIterator<Item = (&'a ClusterId, &'a ClusterHandle)>,
    ) {
        let wanted: BTreeMap<&ClusterId, &ClusterHandle> = clusters.into_iter().collect();
        self.clusters.retain(|id, flux| {
            wanted
                .get(id)
                .is_some_and(|handle| Arc::ptr_eq(&handle.shared, &flux.handle.shared))
        });
        for (id, handle) in wanted {
            if !self.clusters.contains_key(id) {
                let flux = ClusterFlux::start(handle.clone(), self.changed_tx.clone());
                self.clusters.insert(id.clone(), flux);
            }
        }
        let focus_gone = self
            .focus
            .as_ref()
            .is_some_and(|focus| !self.clusters.contains_key(&focus.key.cluster));
        if focus_gone {
            self.focus = None;
        }
    }

    pub fn command(&mut self, command: &Command) {
        match command {
            Command::OpenFluxObject(key) => self.open(key),
            Command::SelectCluster(_) => self.focus = None,
            Command::SelectPage(_) => self.focus = None,
            Command::OpenResource(resource) => match flux_key(resource) {
                Some(key) => self.open(&key),
                None => self.focus = None,
            },
            Command::OpenCommit(_) => self.focus = None,
            _ => {}
        }
    }

    fn open(&mut self, key: &FluxKey) {
        if self.focus.as_ref().is_some_and(|focus| &focus.key == key) {
            return;
        }
        self.focus = self
            .clusters
            .get(&key.cluster)
            .map(|flux| Focus::start(key.clone(), &flux.handle, self.changed_tx.clone()));
    }

    fn data(&self) -> Vec<ClusterData> {
        self.clusters
            .iter()
            .map(|(id, flux)| {
                let objects = KINDS
                    .iter()
                    .flat_map(|kind| {
                        flux.objects(*kind)
                            .into_iter()
                            .filter_map(move |object| parse(&object, *kind))
                    })
                    .collect();
                let online = !matches!(
                    flux.handle.status(),
                    ConnectionStatus::Offline(_) | ConnectionStatus::AuthExpired(_)
                );
                ClusterData {
                    id: id.clone(),
                    online,
                    objects,
                    info: flux.info(),
                }
            })
            .collect()
    }

    pub fn fill(&self, snapshot: &mut Snapshot) {
        let now = Timestamp::now();
        let data = self.data();
        for cluster in &data {
            if !cluster.info.detected() {
                continue;
            }
            let Some(handle) = self.clusters.get(&cluster.id).map(|flux| &flux.handle) else {
                continue;
            };
            let workloads = Workloads::of(handle);
            let health = |entry: &fleet_flux::InventoryEntry| workloads.health(entry);
            for parsed in &cluster.objects {
                let focused = self
                    .focus
                    .as_ref()
                    .filter(|focus| convert::key(&cluster.id, parsed).as_ref() == Some(&focus.key));
                let detail = focused.map(|focus| lock(&focus.detail));
                let history = detail.as_ref().and_then(|d| d.history(now));
                let releases = detail.as_ref().and_then(|d| d.releases(now));
                drop(detail);
                let inputs = Inputs {
                    cluster: &cluster.id,
                    objects: &cluster.objects,
                    now,
                    health: &health,
                    history,
                    releases,
                };
                snapshot.flux.extend(flux_object(parsed, inputs));
            }
            if let Some(summary) = snapshot.clusters.iter_mut().find(|c| c.id == cluster.id) {
                summary.nav.extend(nav_counts(&cluster.objects));
            }
        }
        let views: Vec<ClusterFluxView<'_>> = data
            .iter()
            .map(|cluster| ClusterFluxView {
                id: &cluster.id,
                online: cluster.online,
                objects: &cluster.objects,
                info: &cluster.info,
            })
            .collect();
        snapshot.gitops = grid::gitops(&views, now);
    }
}

fn flux_key(resource: &ResourceRef) -> Option<FluxKey> {
    let kind = FluxKind::from_label(&resource.kind)?;
    let namespace = resource.namespace.clone()?;
    Some(FluxKey {
        cluster: resource.cluster.clone(),
        kind,
        namespace,
        name: resource.name.clone(),
    })
}

fn nav_counts(objects: &[Parsed]) -> [(Page, Counts); 3] {
    let tally = |kinds: &[ObjectKind]| {
        let statuses: Vec<Status> = objects
            .iter()
            .filter(|parsed| kinds.contains(&parsed.kind()))
            .map(|parsed| core_status(summary(parsed, objects).status))
            .collect();
        let count = |status: Status| {
            let n = statuses.iter().filter(|s| **s == status).count();
            u32::try_from(n).unwrap_or(u32::MAX)
        };
        Counts::total(u32::try_from(statuses.len()).unwrap_or(u32::MAX))
            .with_err(count(Status::Err))
            .with_warn(count(Status::Warn))
            .with_sus(count(Status::Suspended))
    };
    [
        (
            FluxKind::Kustomization.page(),
            tally(&[ObjectKind::Kustomization]),
        ),
        (
            FluxKind::HelmRelease.page(),
            tally(&[ObjectKind::HelmRelease]),
        ),
        (
            FluxKind::GitRepository.page(),
            tally(&[
                ObjectKind::GitRepository,
                ObjectKind::HelmRepository,
                ObjectKind::OCIRepository,
            ]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use convert::tests::{git_repository, kustomization, parsed};

    #[test]
    fn flux_resources_map_to_flux_keys() {
        let resource = |kind: &str| ResourceRef {
            cluster: ClusterId::new("fleet-a"),
            kind: kind.to_owned(),
            namespace: Some("payments".to_owned()),
            name: "checkout-api".to_owned(),
        };
        assert_eq!(
            flux_key(&resource("HelmRelease")),
            Some(FluxKey::new(
                "fleet-a",
                FluxKind::HelmRelease,
                "payments",
                "checkout-api"
            ))
        );
        assert_eq!(flux_key(&resource("Deployment")), None);
    }

    #[test]
    fn nav_counts_tally_flux_pages() {
        let objects = [
            parsed(&kustomization("apps", "True", "3e0ff8ae123b")),
            parsed(&kustomization("broken", "False", "3e0ff8ae123b")),
            parsed(&git_repository()),
        ];
        let counts = nav_counts(&objects);
        assert_eq!(
            counts,
            [
                (Page::Kustomizations, Counts::total(2).with_err(1)),
                (Page::HelmReleases, Counts::total(0)),
                (Page::Sources, Counts::total(1)),
            ]
        );
    }
}
