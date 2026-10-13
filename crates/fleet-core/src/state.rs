use crate::resource::page_for_kind;
use crate::{
    AddClusterStep, CheckLine, ClusterId, ContextRef, ExecRequest, FluxAction, FluxKey, FluxKind,
    KubeconfigFile, LogRequest, NewCluster, Notice, Page, Preferences, ResourceRef, Sink, Snapshot,
    Verdict, Workload,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    SelectCluster(Option<ClusterId>),
    SelectPage(Page),
    OpenCommit(String),
    ToggleClustersPanel,
    ToggleResourcesPanel,
    OpenResource(ResourceRef),
    InspectResource(ResourceRef),
    FollowLogs(ResourceRef),
    Compare(ResourceRef),
    PortForward {
        service: ResourceRef,
        local: u16,
        remote: u16,
    },
    StopPortForward {
        local: u16,
    },
    Cordon(ResourceRef),
    Uncordon(ResourceRef),
    Drain(ResourceRef),
    Delete(ResourceRef),
    OpenAddCluster(AddClusterStep),
    CheckContexts(Vec<ContextRef>),
    AddClusters(Vec<NewCluster>),
    RemoveCluster(ClusterId),
    WatchWorkload {
        target: ResourceRef,
        sink: Sink<Workload>,
    },
    StreamLogs(LogRequest),
    Exec(ExecRequest),
    RestartRollout(ResourceRef),
    Scale {
        target: ResourceRef,
        replicas: u32,
    },
    Rollback {
        target: ResourceRef,
        revision: u64,
    },
    OpenFluxObject(FluxKey),
    Flux(FluxKey, FluxAction),
    #[cfg(feature = "loco")]
    Loco(crate::loco::LocoCommand),
    CopySpec {
        from: ClusterId,
        to: ResourceRef,
        path: String,
    },
    ShowClusterSettings(ClusterId),
    SetPreferences(ClusterId, Preferences),
    Connect(ClusterId),
    Reauthenticate(ClusterId),
    Disconnect(ClusterId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Snapshot(Box<Snapshot>),
    Contexts(Vec<KubeconfigFile>),
    Check(ContextRef, CheckLine),
    Checked(ContextRef, Verdict),
    Notice(Notice),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub snapshot: Snapshot,
    pub cluster: Option<ClusterId>,
    pub page: Page,
    pub resource: Option<ResourceRef>,
    pub clusters_open: bool,
    pub resources_open: bool,
    pub flux_object: Option<FluxKey>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            snapshot: Snapshot::default(),
            cluster: None,
            page: Page::Overview,
            resource: None,
            clusters_open: true,
            resources_open: true,
            flux_object: None,
        }
    }
}

impl AppState {
    pub fn apply(&mut self, command: &Command) {
        match command {
            Command::SelectCluster(Some(id)) => {
                if self.snapshot.cluster(id).is_some() {
                    self.cluster = Some(id.clone());
                    self.page = Page::ClusterOverview;
                    self.resource = None;
                    self.flux_object = None;
                }
            }
            Command::SelectCluster(None) => {
                self.cluster = None;
                self.page = Page::Overview;
                self.resource = None;
                self.flux_object = None;
            }
            Command::SelectPage(page) => self.select_page(*page),
            Command::OpenCommit(_) => self.select_page(Page::Changes),
            Command::ToggleClustersPanel => self.clusters_open = !self.clusters_open,
            Command::ToggleResourcesPanel => self.resources_open = !self.resources_open,
            Command::OpenResource(resource) => self.open_resource(resource),
            Command::InspectResource(_) => {}
            Command::FollowLogs(resource) => self.open_resource(resource),
            Command::Compare(resource) => self.compare(resource),
            Command::PortForward { .. } => {}
            Command::StopPortForward { .. } => {}
            Command::Cordon(_) => {}
            Command::Uncordon(_) => {}
            Command::Drain(_) => {}
            Command::Delete(_) => {}
            Command::OpenAddCluster(_) => {}
            Command::CheckContexts(_) => {}
            Command::AddClusters(_) => {}
            Command::RemoveCluster(_) => {}
            Command::WatchWorkload { .. } => {}
            Command::StreamLogs(_) => {}
            Command::Exec(_) => {}
            Command::RestartRollout(_) => {}
            Command::Scale { .. } => {}
            Command::Rollback { .. } => {}
            Command::OpenFluxObject(key) => self.open_flux_object(key),
            Command::Flux(..) => {}
            #[cfg(feature = "loco")]
            Command::Loco(_) => {}
            Command::CopySpec { .. } => {}
            Command::ShowClusterSettings(_) => self.select_page(Page::ClusterSettings),
            Command::SetPreferences(..) => {}
            Command::Connect(_) => {}
            Command::Reauthenticate(_) => {}
            Command::Disconnect(_) => {}
        }
    }

    pub fn ready_for(&self, command: &Command) -> bool {
        let has_cluster = |id: &ClusterId| self.snapshot.cluster(id).is_some();
        match command {
            Command::SelectCluster(Some(id)) => has_cluster(id),
            Command::OpenResource(resource) => has_cluster(&resource.cluster),
            Command::FollowLogs(resource) => has_cluster(&resource.cluster),
            Command::Compare(resource) => has_cluster(&resource.cluster),
            Command::ShowClusterSettings(id) => has_cluster(id),
            Command::OpenFluxObject(key) => self.snapshot.flux_object(key).is_some(),
            _ => true,
        }
    }

    pub fn update(&mut self, update: Update) {
        match update {
            Update::Snapshot(snapshot) => {
                self.snapshot = *snapshot;
                let selection_gone = self
                    .cluster
                    .as_ref()
                    .is_some_and(|id| self.snapshot.cluster(id).is_none());
                if selection_gone {
                    self.apply(&Command::SelectCluster(None));
                }
                let object_gone = self
                    .flux_object
                    .as_ref()
                    .is_some_and(|key| self.snapshot.flux_object(key).is_none());
                if object_gone {
                    self.flux_object = None;
                }
            }
            Update::Contexts(_) => {}
            Update::Check(..) => {}
            Update::Checked(..) => {}
            Update::Notice(_) => {}
        }
    }

    fn select_page(&mut self, page: Page) {
        if page.is_fleet() {
            self.cluster = None;
            self.page = page;
            self.resource = None;
            self.flux_object = None;
        } else if self.cluster.is_some() {
            self.page = page;
            self.resource = None;
            self.flux_object = None;
        }
    }

    fn compare(&mut self, resource: &ResourceRef) {
        if self.snapshot.cluster(&resource.cluster).is_none() {
            return;
        }
        self.cluster = Some(resource.cluster.clone());
        self.page = Page::Compare;
        self.resource = Some(resource.clone());
    }

    fn open_resource(&mut self, resource: &ResourceRef) {
        let flux = FluxKind::from_label(&resource.kind).zip(resource.namespace.as_ref());
        let key = flux.map(|(kind, namespace)| FluxKey {
            cluster: resource.cluster.clone(),
            kind,
            namespace: namespace.clone(),
            name: resource.name.clone(),
        });
        if let Some(key) = key.filter(|key| self.snapshot.flux_object(key).is_some()) {
            self.open_flux_object(&key);
            return;
        }
        if self.snapshot.cluster(&resource.cluster).is_none() {
            return;
        }
        self.cluster = Some(resource.cluster.clone());
        self.page = page_for_kind(&resource.kind);
        self.resource = Some(resource.clone());
        self.flux_object = None;
    }

    fn open_flux_object(&mut self, key: &FluxKey) {
        if self.snapshot.flux_object(key).is_none() {
            return;
        }
        self.cluster = Some(key.cluster.clone());
        self.page = key.kind.page();
        self.resource = None;
        self.flux_object = Some(key.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::tests::sample;

    fn loaded() -> AppState {
        let mut state = AppState::default();
        state.update(Update::Snapshot(Box::new(sample())));
        state
    }

    #[test]
    fn selecting_a_cluster_opens_its_overview() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        assert_eq!(state.cluster, Some(ClusterId::new("a")));
        assert_eq!(state.page, Page::ClusterOverview);
    }

    #[test]
    fn selecting_an_unknown_cluster_is_ignored() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("nope"))));
        assert_eq!(state, loaded());
    }

    #[test]
    fn cluster_pages_need_a_cluster() {
        let mut state = loaded();
        state.apply(&Command::SelectPage(Page::Pods));
        assert_eq!(state.page, Page::Overview);
        state.apply(&Command::SelectCluster(Some(ClusterId::new("b"))));
        state.apply(&Command::SelectPage(Page::Pods));
        assert_eq!(state.page, Page::Pods);
    }

    #[test]
    fn fleet_pages_clear_the_cluster() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        state.apply(&Command::SelectPage(Page::GitOps));
        assert_eq!(state.cluster, None);
        assert_eq!(state.page, Page::GitOps);
    }

    fn deployment(cluster: &str) -> ResourceRef {
        ResourceRef {
            cluster: ClusterId::new(cluster),
            kind: "Deployment".into(),
            namespace: Some("payments".into()),
            name: "checkout-api".into(),
        }
    }

    #[test]
    fn opening_a_workload_selects_its_cluster_and_list_page() {
        let mut state = loaded();
        state.apply(&Command::OpenResource(deployment("b")));
        assert_eq!(state.cluster, Some(ClusterId::new("b")));
        assert_eq!(state.page, Page::Deployments);
        assert_eq!(state.resource, Some(deployment("b")));
        state.apply(&Command::SelectPage(Page::Pods));
        assert_eq!(state.resource, None);
    }

    #[test]
    fn inspecting_a_listed_workload_stays_on_the_list() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("b"))));
        state.apply(&Command::SelectPage(Page::Deployments));
        let before = state.clone();
        state.apply(&Command::InspectResource(deployment("b")));
        assert_eq!(state, before);
        state.apply(&Command::OpenResource(deployment("b")));
        assert_eq!(state.resource, Some(deployment("b")));
    }

    #[test]
    fn opening_a_commit_shows_changes_for_the_fleet() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        state.apply(&Command::OpenCommit("a41e07c".into()));
        assert_eq!(state.cluster, None);
        assert_eq!(state.page, Page::Changes);
    }

    #[test]
    fn opening_a_resource_selects_its_cluster_and_list_page() {
        let mut state = loaded();
        let pod = ResourceRef {
            cluster: ClusterId::new("b"),
            kind: "Pod".into(),
            namespace: Some("shop".into()),
            name: "cart-7f9c".into(),
        };
        state.apply(&Command::OpenResource(pod.clone()));
        assert_eq!(state.cluster, Some(ClusterId::new("b")));
        assert_eq!(state.page, Page::Pods);
        assert_eq!(state.resource, Some(pod));
        state.apply(&Command::SelectPage(Page::Nodes));
        assert_eq!(state.resource, None);
    }

    #[test]
    fn opening_a_resource_on_an_unknown_cluster_is_ignored() {
        let mut state = loaded();
        let ghost = ResourceRef {
            cluster: ClusterId::new("nope"),
            kind: "Pod".into(),
            namespace: None,
            name: "x".into(),
        };
        state.apply(&Command::FollowLogs(ghost));
        assert_eq!(state, loaded());
    }

    #[test]
    fn compare_opens_the_compare_page_for_the_resource() {
        let mut state = loaded();
        let deploy = ResourceRef {
            cluster: ClusterId::new("a"),
            kind: "Deployment".into(),
            namespace: Some("shop".into()),
            name: "cart".into(),
        };
        state.apply(&Command::Compare(deploy.clone()));
        assert_eq!(state.page, Page::Compare);
        assert_eq!(state.cluster, Some(ClusterId::new("a")));
        assert_eq!(state.resource, Some(deploy));
    }

    #[test]
    fn cluster_settings_is_a_fleet_page() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        state.apply(&Command::ShowClusterSettings(ClusterId::new("b")));
        assert_eq!(state.page, Page::ClusterSettings);
        assert_eq!(state.cluster, None);
    }

    #[test]
    fn toggles_flip_panels() {
        let mut state = loaded();
        state.apply(&Command::ToggleClustersPanel);
        state.apply(&Command::ToggleResourcesPanel);
        state.apply(&Command::ToggleResourcesPanel);
        assert!(!state.clusters_open);
        assert!(state.resources_open);
    }

    fn with_flux() -> (AppState, FluxKey) {
        let mut snapshot = sample();
        let object = crate::flux::tests::object("b", crate::flux::Phase::Ready, "a41e07c");
        let key = object.key.clone();
        snapshot.flux = vec![object];
        let mut state = AppState::default();
        state.update(Update::Snapshot(Box::new(snapshot)));
        (state, key)
    }

    #[test]
    fn opening_a_flux_object_selects_its_cluster_and_nav_page() {
        let (mut state, key) = with_flux();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        state.apply(&Command::OpenFluxObject(key.clone()));
        assert_eq!(state.cluster, Some(ClusterId::new("b")));
        assert_eq!(state.page, Page::Kustomizations);
        assert_eq!(state.flux_object, Some(key));
        state.apply(&Command::SelectPage(Page::Kustomizations));
        assert_eq!(state.flux_object, None);
    }

    #[test]
    fn opening_a_flux_resource_shows_its_object_page() {
        let (mut state, key) = with_flux();
        let target = ResourceRef {
            cluster: key.cluster.clone(),
            kind: "Kustomization".into(),
            namespace: Some(key.namespace.clone()),
            name: key.name.clone(),
        };
        state.apply(&Command::OpenResource(target.clone()));
        assert_eq!(state.flux_object, Some(key));
        state.apply(&Command::OpenResource(ResourceRef {
            kind: "Deployment".into(),
            ..target
        }));
        assert_eq!(state.flux_object, None);
        assert_eq!(state.page, Page::Deployments);
    }

    #[test]
    fn opening_an_unknown_flux_object_is_ignored() {
        let (mut state, key) = with_flux();
        let before = state.clone();
        state.apply(&Command::OpenFluxObject(key.on(&ClusterId::new("a"))));
        assert_eq!(state, before);
    }

    #[test]
    fn flux_object_selection_clears_when_it_disappears() {
        let (mut state, key) = with_flux();
        state.apply(&Command::OpenFluxObject(key));
        state.update(Update::Snapshot(Box::new(sample())));
        assert_eq!(state.flux_object, None);
        assert_eq!(state.page, Page::Kustomizations);
    }

    #[test]
    fn startup_commands_wait_for_what_they_name() {
        let (state, key) = with_flux();
        assert!(state.ready_for(&Command::SelectCluster(Some(ClusterId::new("a")))));
        assert!(!state.ready_for(&Command::SelectCluster(Some(ClusterId::new("nope")))));
        assert!(state.ready_for(&Command::OpenFluxObject(key.clone())));
        assert!(!state.ready_for(&Command::OpenFluxObject(key.on(&ClusterId::new("a")))));
        assert!(!state.ready_for(&Command::OpenResource(deployment("nope"))));
        assert!(state.ready_for(&Command::Compare(deployment("b"))));
        assert!(state.ready_for(&Command::SelectPage(Page::GitOps)));
        assert!(!AppState::default().ready_for(&Command::ShowClusterSettings(ClusterId::new("a"))));
    }

    #[test]
    fn snapshot_without_selected_cluster_returns_to_fleet() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        state.apply(&Command::SelectPage(Page::Nodes));
        state.update(Update::Snapshot(Box::default()));
        assert_eq!(state.cluster, None);
        assert_eq!(state.page, Page::Overview);
    }
}
