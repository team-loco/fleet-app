use crate::{AppState, ClusterId, Command, FluxKey, Page, ResourceRef};

pub const HISTORY_LIMIT: usize = 50;
pub const RECENT_LIMIT: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub cluster: Option<ClusterId>,
    pub page: Page,
    pub resource: Option<ResourceRef>,
    pub flux_object: Option<FluxKey>,
}

impl Location {
    pub fn of(state: &AppState) -> Self {
        Self {
            cluster: state.cluster.clone(),
            page: state.page,
            resource: state.resource.clone(),
            flux_object: state.flux_object.clone(),
        }
    }

    pub fn commands(&self) -> Vec<Command> {
        if let Some(key) = &self.flux_object {
            return vec![Command::OpenFluxObject(key.clone())];
        }
        match (&self.resource, self.page) {
            (Some(resource), Page::Compare) => return vec![Command::Compare(resource.clone())],
            (Some(resource), _) => return vec![Command::OpenResource(resource.clone())],
            (None, _) => {}
        }
        match &self.cluster {
            Some(cluster) if !self.page.is_fleet() => {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cycle {
    Older,
    Newer,
}

#[derive(Debug, Default)]
pub struct History {
    current: Option<Location>,
    back: Vec<Location>,
    forward: Vec<Location>,
    recent: Vec<Location>,
    cycling: Option<(Vec<Location>, usize)>,
}

impl History {
    pub fn visit(&mut self, location: Location) {
        let cycled = self
            .cycling
            .as_ref()
            .is_some_and(|(frozen, at)| frozen.get(*at) == Some(&location));
        if !cycled {
            self.cycling = None;
        }
        self.recent.retain(|seen| seen != &location);
        self.recent.insert(0, location.clone());
        self.recent.truncate(RECENT_LIMIT);
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

    pub fn cycle(&mut self, direction: Cycle) -> Option<Location> {
        let (frozen, at) = self.cycling.get_or_insert_with(|| (self.recent.clone(), 0));
        let len = frozen.len();
        if len < 2 {
            return None;
        }
        *at = match direction {
            Cycle::Older => (*at + 1) % len,
            Cycle::Newer => (*at + len - 1) % len,
        };
        frozen.get(*at).cloned()
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
            cluster: (!page.is_fleet()).then(|| ClusterId::new("a")),
            page,
            resource: None,
            flux_object: None,
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
        let mut history = History::default();
        history.visit(page(Page::Overview));
        history.visit(page(Page::Overview));
        assert!(!history.can_go_back());
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
    fn control_tab_cycles_recent_pages_while_held() {
        let mut history = History::default();
        for p in [Page::Overview, Page::Nodes, Page::Pods] {
            history.visit(page(p));
        }
        let first = history.cycle(Cycle::Older);
        assert_eq!(first, Some(page(Page::Nodes)));
        history.visit(page(Page::Nodes));
        assert_eq!(history.cycle(Cycle::Older), Some(page(Page::Overview)));
        history.visit(page(Page::Overview));
        assert_eq!(history.cycle(Cycle::Newer), Some(page(Page::Nodes)));
        history.visit(page(Page::Nodes));
        history.visit(page(Page::Events));
        assert_eq!(history.cycle(Cycle::Older), Some(page(Page::Nodes)));
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
        let compare = Location {
            resource: Some(pod.clone()),
            ..page(Page::Compare)
        };
        assert_eq!(compare.commands(), [Command::Compare(pod)]);
        let mut state = AppState::default();
        state.update(crate::Update::Snapshot(Box::new(
            crate::fleet::tests::sample(),
        )));
        for command in nodes.commands() {
            state.apply(&command);
        }
        assert_eq!(Location::of(&state), nodes);
    }
}
