use serde::{Deserialize, Serialize};

use crate::{ClusterId, Command, FluxKey, Page, ResourceRef, WorkloadKind};

pub const HISTORY_LIMIT: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub cluster: Option<ClusterId>,
    pub page: Page,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<ResourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flux_object: Option<FluxKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

impl Default for Location {
    fn default() -> Self {
        Self::flock(Page::Overview)
    }
}

impl Location {
    pub const fn flock(page: Page) -> Self {
        Self {
            cluster: None,
            page,
            resource: None,
            flux_object: None,
            container: None,
        }
    }

    pub fn page_on(cluster: ClusterId, page: Page) -> Self {
        Self {
            cluster: Some(cluster),
            ..Self::flock(page)
        }
    }

    pub fn workload(&self) -> Option<&ResourceRef> {
        self.resource
            .as_ref()
            .filter(|_| self.page != Page::Compare && self.page != Page::Logs)
            .filter(|target| WorkloadKind::from_kind(&target.kind).is_some())
    }

    pub fn commands(&self) -> Vec<Command> {
        if let Some(key) = &self.flux_object {
            return vec![Command::OpenFluxObject(key.clone())];
        }
        match (&self.resource, self.page, &self.container) {
            (Some(pod), Page::Logs, Some(container)) => {
                return vec![Command::OpenLogs {
                    pod: pod.clone(),
                    container: container.clone(),
                }];
            }
            (Some(resource), Page::Compare, _) => {
                return vec![Command::Compare(resource.clone())];
            }
            (Some(resource), _, _) => return vec![Command::OpenResource(resource.clone())],
            (None, _, _) => {}
        }
        match &self.cluster {
            Some(cluster) if !self.page.is_flock() => {
                let select = Command::SelectCluster(Some(cluster.clone()));
                if self.page == Page::ClusterOverview {
                    vec![select]
                } else {
                    vec![select, Command::SelectPage(self.page)]
                }
            }
            Some(_) => vec![Command::SelectPage(self.page)],
            None => vec![Command::SelectPage(self.page)],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    current: Option<Location>,
    back: Vec<Location>,
    forward: Vec<Location>,
}

impl History {
    pub fn starting_at(location: Location) -> Self {
        Self {
            current: Some(location),
            ..Self::default()
        }
    }

    pub fn visit(&mut self, location: Location) {
        if self.current.as_ref() == Some(&location) {
            return;
        }
        if let Some(previous) = self.current.replace(location) {
            self.back.push(previous);
            trim(&mut self.back);
        }
        self.forward.clear();
    }

    pub fn back(&mut self) -> Option<Location> {
        let target = self.back.pop()?;
        if let Some(current) = self.current.replace(target.clone()) {
            self.forward.push(current);
        }
        Some(target)
    }

    pub fn forward(&mut self) -> Option<Location> {
        let target = self.forward.pop()?;
        if let Some(current) = self.current.replace(target.clone()) {
            self.back.push(current);
        }
        Some(target)
    }

    pub fn current(&self) -> Option<&Location> {
        self.current.as_ref()
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
}

fn trim(stack: &mut Vec<Location>) {
    let excess = stack.len().saturating_sub(HISTORY_LIMIT);
    stack.drain(..excess);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(page: Page) -> Location {
        Location {
            cluster: (!page.is_flock()).then(|| ClusterId::new("a")),
            ..Location::flock(page)
        }
    }

    #[test]
    fn back_and_forward_walk_the_visits() {
        let mut history = History::default();
        for p in [Page::Overview, Page::Nodes, Page::Pods] {
            history.visit(page(p));
        }
        assert_eq!(history.back(), Some(page(Page::Nodes)));
        history.visit(page(Page::Nodes));
        assert_eq!(history.back(), Some(page(Page::Overview)));
        assert_eq!(history.back(), None);
        assert_eq!(history.forward(), Some(page(Page::Nodes)));
        assert_eq!(history.forward(), Some(page(Page::Pods)));
        assert_eq!(history.forward(), None);
        history.back();
        history.visit(page(Page::Events));
        assert_eq!(history.forward(), None);
        assert_eq!(history.back(), Some(page(Page::Nodes)));
    }

    #[test]
    fn repeated_visits_to_the_same_place_are_one_entry() {
        let mut history = History::starting_at(page(Page::Overview));
        history.visit(page(Page::Overview));
        assert!(!history.can_go_back());
        assert_eq!(history.current(), Some(&page(Page::Overview)));
    }

    #[test]
    fn history_is_bounded() {
        let mut history = History::default();
        let pages = [Page::Nodes, Page::Pods];
        for ix in 0..=HISTORY_LIMIT * 2 {
            history.visit(page(pages[ix % 2]));
        }
        let mut steps = 0;
        while history.back().is_some() {
            steps += 1;
        }
        assert_eq!(steps, HISTORY_LIMIT);
    }

    #[test]
    fn locations_restore_through_commands() {
        let nodes = page(Page::Nodes);
        assert_eq!(
            nodes.commands(),
            [
                Command::SelectCluster(Some(ClusterId::new("a"))),
                Command::SelectPage(Page::Nodes)
            ]
        );
        assert_eq!(
            page(Page::GitOps).commands(),
            [Command::SelectPage(Page::GitOps)]
        );
        let pod = ResourceRef {
            cluster: ClusterId::new("a"),
            kind: "Pod".into(),
            namespace: Some("shop".into()),
            name: "cart".into(),
        };
        let open = Location {
            resource: Some(pod.clone()),
            ..page(Page::Pods)
        };
        assert_eq!(open.commands(), [Command::OpenResource(pod.clone())]);
        let logs = Location {
            resource: Some(pod.clone()),
            container: Some("api".into()),
            ..page(Page::Logs)
        };
        assert_eq!(
            logs.commands(),
            [Command::OpenLogs {
                pod: pod.clone(),
                container: "api".into()
            }]
        );
        let compare = Location {
            resource: Some(pod.clone()),
            ..page(Page::Compare)
        };
        assert_eq!(compare.commands(), [Command::Compare(pod)]);
        let mut state = crate::AppState::default();
        state.update(crate::Update::Snapshot(Box::new(
            crate::flock::tests::sample(),
        )));
        for command in nodes.commands() {
            state.apply(&command);
        }
        assert_eq!(state.location(), &nodes);
    }
}
