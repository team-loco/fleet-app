use crate::{ClusterId, Page, Snapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    SelectCluster(Option<ClusterId>),
    SelectPage(Page),
    ToggleClustersPanel,
    ToggleResourcesPanel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    Snapshot(Snapshot),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppState {
    pub snapshot: Snapshot,
    pub cluster: Option<ClusterId>,
    pub page: Page,
    pub clusters_open: bool,
    pub resources_open: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            snapshot: Snapshot::default(),
            cluster: None,
            page: Page::Overview,
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
                }
            }
            Command::SelectCluster(None) => {
                self.cluster = None;
                self.page = Page::Overview;
            }
            Command::SelectPage(page) => self.select_page(*page),
            Command::ToggleClustersPanel => self.clusters_open = !self.clusters_open,
            Command::ToggleResourcesPanel => self.resources_open = !self.resources_open,
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
        } else if self.cluster.is_some() {
            self.page = page;
        }
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
