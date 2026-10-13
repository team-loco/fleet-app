use crate::resource::page_for_kind;
use crate::{ClusterId, Page, ResourceRef, Snapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    SelectCluster(Option<ClusterId>),
    SelectPage(Page),
    OpenCommit(String),
    ToggleClustersPanel,
    ToggleResourcesPanel,
    OpenResource(ResourceRef),
    FollowLogs(ResourceRef),
    Compare(ResourceRef),
    PortForward {
        service: ResourceRef,
        local: u16,
        remote: u16,
    },
    Cordon(ResourceRef),
    Uncordon(ResourceRef),
    Drain(ResourceRef),
    Delete(ResourceRef),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Snapshot(Snapshot),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub snapshot: Snapshot,
    pub cluster: Option<ClusterId>,
    pub page: Page,
    pub resource: Option<ResourceRef>,
    pub clusters_open: bool,
    pub resources_open: bool,
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
                }
            }
            Command::SelectCluster(None) => {
                self.cluster = None;
                self.page = Page::Overview;
                self.resource = None;
            }
            Command::SelectPage(page) => self.select_page(*page),
            Command::OpenCommit(_) => self.select_page(Page::Changes),
            Command::ToggleClustersPanel => self.clusters_open = !self.clusters_open,
            Command::ToggleResourcesPanel => self.resources_open = !self.resources_open,
            Command::OpenResource(resource) => self.open_resource(resource),
            Command::FollowLogs(resource) => self.open_resource(resource),
            Command::Compare(_) => {}
            Command::PortForward { .. } => {}
            Command::Cordon(_) => {}
            Command::Uncordon(_) => {}
            Command::Drain(_) => {}
            Command::Delete(_) => {}
        }
    }

    pub fn update(&mut self, update: Update) {
        match update {
            Update::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                let selection_gone = self
                    .cluster
                    .as_ref()
                    .is_some_and(|id| self.snapshot.cluster(id).is_none());
                if selection_gone {
                    self.apply(&Command::SelectCluster(None));
                }
            }
        }
    }

    fn select_page(&mut self, page: Page) {
        if page.is_fleet() {
            self.cluster = None;
            self.page = page;
            self.resource = None;
        } else if self.cluster.is_some() {
            self.page = page;
            self.resource = None;
        }
    }

    fn open_resource(&mut self, resource: &ResourceRef) {
        if self.snapshot.cluster(&resource.cluster).is_none() {
            return;
        }
        self.cluster = Some(resource.cluster.clone());
        self.page = page_for_kind(&resource.kind);
        self.resource = Some(resource.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::tests::sample;

    fn loaded() -> AppState {
        let mut state = AppState::default();
        state.update(Update::Snapshot(sample()));
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
    fn toggles_flip_panels() {
        let mut state = loaded();
        state.apply(&Command::ToggleClustersPanel);
        state.apply(&Command::ToggleResourcesPanel);
        state.apply(&Command::ToggleResourcesPanel);
        assert!(!state.clusters_open);
        assert!(state.resources_open);
    }

    #[test]
    fn snapshot_without_selected_cluster_returns_to_fleet() {
        let mut state = loaded();
        state.apply(&Command::SelectCluster(Some(ClusterId::new("a"))));
        state.apply(&Command::SelectPage(Page::Nodes));
        state.update(Update::Snapshot(Snapshot::default()));
        assert_eq!(state.cluster, None);
        assert_eq!(state.page, Page::Overview);
    }
}
