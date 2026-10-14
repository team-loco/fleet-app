use serde::{Deserialize, Serialize};

use crate::history::{History, Location};
use crate::resource::short_kind;
use crate::{ClusterId, Page, Snapshot, Status};

pub const FLOCK_KIND: &str = "flock";
pub const LOGS_KIND: &str = "logs";
pub const COMPARE_KIND: &str = "compare";
pub const CLUSTER_KIND: &str = "cluster";
const NAME_SEPARATOR: char = '-';

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(u64);

impl TabId {
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    id: TabId,
    location: Location,
    history: History,
}

impl Tab {
    fn new(id: TabId, location: Location) -> Self {
        Self {
            id,
            history: History::starting_at(location.clone()),
            location,
        }
    }

    pub const fn id(&self) -> TabId {
        self.id
    }

    pub const fn location(&self) -> &Location {
        &self.location
    }

    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    pub(crate) fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }

    pub(crate) fn record(&mut self) {
        self.history.visit(self.location.clone());
    }

    pub(crate) fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cycle {
    Next,
    Previous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    tabs: Vec<Tab>,
    active: usize,
}

impl Pane {
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub const fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &Tab {
        match self.tabs.get(self.active) {
            Some(tab) => tab,
            None => unreachable!("a pane always has an active tab"),
        }
    }

    fn active_mut(&mut self) -> &mut Tab {
        match self.tabs.get_mut(self.active) {
            Some(tab) => tab,
            None => unreachable!("a pane always has an active tab"),
        }
    }

    fn position(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panes {
    all: Vec<Pane>,
    focused: usize,
    next_id: u64,
}

impl Default for Panes {
    fn default() -> Self {
        Self::restore(&SavedWindow::default())
    }
}

impl Panes {
    pub fn restore(saved: &SavedWindow) -> Self {
        let mut panes = Self {
            all: Vec::new(),
            focused: 0,
            next_id: 0,
        };
        for pane in &saved.panes {
            let tabs: Vec<Tab> = pane
                .tabs
                .iter()
                .map(|location| {
                    let id = panes.allocate();
                    Tab::new(id, location.clone())
                })
                .collect();
            if !tabs.is_empty() {
                let active = pane.active.min(tabs.len() - 1);
                panes.all.push(Pane { tabs, active });
            }
        }
        if panes.all.is_empty() {
            let id = panes.allocate();
            panes.all.push(Pane {
                tabs: vec![Tab::new(id, Location::default())],
                active: 0,
            });
        }
        panes.focused = saved.focused.min(panes.all.len() - 1);
        panes
    }

    pub fn saved(&self) -> SavedWindow {
        SavedWindow {
            panes: self
                .all
                .iter()
                .map(|pane| SavedPane {
                    tabs: pane.tabs.iter().map(|tab| tab.location.clone()).collect(),
                    active: pane.active,
                })
                .collect(),
            focused: self.focused,
        }
    }

    pub fn panes(&self) -> &[Pane] {
        &self.all
    }

    pub const fn focused_index(&self) -> usize {
        self.focused
    }

    pub fn focused(&self) -> &Pane {
        match self.all.get(self.focused) {
            Some(pane) => pane,
            None => unreachable!("a window always has a focused pane"),
        }
    }

    fn focused_mut(&mut self) -> &mut Pane {
        match self.all.get_mut(self.focused) {
            Some(pane) => pane,
            None => unreachable!("a window always has a focused pane"),
        }
    }

    pub fn active(&self) -> &Tab {
        self.focused().active()
    }

    pub(crate) fn active_mut(&mut self) -> &mut Tab {
        self.focused_mut().active_mut()
    }

    pub(crate) fn tabs_mut(&mut self) -> impl Iterator<Item = &mut Tab> {
        self.all.iter_mut().flat_map(|pane| pane.tabs.iter_mut())
    }

    fn allocate(&mut self) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn open(&mut self, location: Location) -> TabId {
        let pane = self.focused();
        let existing = pane
            .tabs
            .iter()
            .find(|tab| tab.location == location)
            .map(Tab::id);
        if let Some(id) = existing {
            self.activate(id);
            return id;
        }
        self.add(location)
    }

    pub fn add(&mut self, location: Location) -> TabId {
        let id = self.allocate();
        let pane = self.focused_mut();
        let at = pane.active + 1;
        pane.tabs.insert(at, Tab::new(id, location));
        pane.active = at;
        id
    }

    pub fn activate(&mut self, id: TabId) -> bool {
        let found = self
            .all
            .iter()
            .enumerate()
            .find_map(|(pane_ix, pane)| pane.position(id).map(|tab_ix| (pane_ix, tab_ix)));
        let Some((pane_ix, tab_ix)) = found else {
            return false;
        };
        self.focused = pane_ix;
        if let Some(pane) = self.all.get_mut(pane_ix) {
            pane.active = tab_ix;
        }
        true
    }

    pub fn activate_nth(&mut self, ix: usize) -> bool {
        let pane = self.focused_mut();
        if ix >= pane.tabs.len() {
            return false;
        }
        pane.active = ix;
        true
    }

    pub fn cycle(&mut self, direction: Cycle) -> bool {
        let pane = self.focused_mut();
        let len = pane.tabs.len();
        if len < 2 {
            return false;
        }
        pane.active = match direction {
            Cycle::Next => (pane.active + 1) % len,
            Cycle::Previous => (pane.active + len - 1) % len,
        };
        true
    }

    pub fn close(&mut self, id: TabId) -> bool {
        let Some(pane_ix) = self.all.iter().position(|pane| pane.position(id).is_some()) else {
            return false;
        };
        let replacement = self.allocate();
        let Some(pane) = self.all.get_mut(pane_ix) else {
            return false;
        };
        let Some(ix) = pane.position(id) else {
            return false;
        };
        pane.tabs.remove(ix);
        if pane.tabs.is_empty() {
            pane.tabs.push(Tab::new(replacement, Location::default()));
        }
        if ix < pane.active || pane.active >= pane.tabs.len() {
            pane.active = pane.active.saturating_sub(1);
        }
        true
    }

    pub fn move_tab(&mut self, id: TabId, to: usize) -> bool {
        let pane = self.focused_mut();
        let Some(from) = pane.position(id) else {
            return false;
        };
        let to = to.min(pane.tabs.len() - 1);
        if from == to {
            return false;
        }
        let active = pane.active().id;
        let tab = pane.tabs.remove(from);
        pane.tabs.insert(to, tab);
        pane.active = pane.position(active).unwrap_or(to);
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPane {
    #[serde(deserialize_with = "lenient::locations")]
    pub tabs: Vec<Location>,
    pub active: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedWindow {
    pub panes: Vec<SavedPane>,
    pub focused: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub windows: Vec<SavedWindow>,
}

mod lenient {
    use serde::{Deserialize, Deserializer, de::IgnoredAny};

    use crate::history::Location;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Valid(Box<Location>),
        Invalid(IgnoredAny),
    }

    pub fn locations<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Location>, D::Error> {
        let entries = Vec::<Entry>::deserialize(deserializer)?;
        let valid = entries.into_iter().filter_map(|entry| match entry {
            Entry::Valid(location) => Some(*location),
            Entry::Invalid(_) => None,
        });
        Ok(valid.collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabTitle {
    pub status: Option<Status>,
    pub kind: String,
    pub label: String,
    pub cluster: Option<String>,
}

pub fn title(location: &Location, snapshot: &Snapshot) -> TabTitle {
    let names: Vec<&str> = snapshot.clusters.iter().map(|c| c.name.as_str()).collect();
    let cluster = location.cluster.as_ref().map(|id| {
        let name = snapshot
            .cluster(id)
            .map_or(id.as_str(), |c| c.name.as_str());
        short_name(name, &names)
    });
    let (status, kind, label) = describe(location, snapshot);
    let cluster = cluster.filter(|_| !location.page.is_flock());
    TabTitle {
        status,
        kind,
        label,
        cluster,
    }
}

fn describe(location: &Location, snapshot: &Snapshot) -> (Option<Status>, String, String) {
    if let Some(key) = &location.flux_object {
        let status = snapshot.flux_object(key).map(crate::FluxObject::status);
        return (status, short_kind(key.kind.label()), key.name.clone());
    }
    if location.page.is_flock() {
        return (
            None,
            FLOCK_KIND.to_owned(),
            location.page.label().to_owned(),
        );
    }
    let Some(resource) = &location.resource else {
        return (
            None,
            page_kind(location.page),
            location.page.label().to_owned(),
        );
    };
    let status = resource_status(resource, snapshot);
    match location.page {
        Page::Logs => (
            None,
            LOGS_KIND.to_owned(),
            pod_suffix(&resource.name).to_owned(),
        ),
        Page::Compare => (None, COMPARE_KIND.to_owned(), resource.name.clone()),
        _ => (status, short_kind(&resource.kind), resource.name.clone()),
    }
}

fn resource_status(resource: &crate::ResourceRef, snapshot: &Snapshot) -> Option<Status> {
    let pod = snapshot.pods.iter().find(|pod| pod.is(resource));
    if let Some(pod) = pod {
        return Some(pod.status);
    }
    snapshot
        .resources
        .iter()
        .find(|summary| &summary.resource == resource)
        .and_then(|summary| summary.status)
}

fn page_kind(page: Page) -> String {
    crate::CLUSTER_NAV
        .iter()
        .find(|section| section.pages.contains(&page))
        .and_then(|section| section.label)
        .map_or_else(|| CLUSTER_KIND.to_owned(), str::to_lowercase)
}

pub fn pod_suffix(name: &str) -> &str {
    name.rsplit_once(NAME_SEPARATOR)
        .map(|(_, suffix)| suffix)
        .filter(|suffix| !suffix.is_empty())
        .unwrap_or(name)
}

pub fn short_name(name: &str, names: &[&str]) -> String {
    let Some((head, rest)) = name.split_once(NAME_SEPARATOR) else {
        return name.to_owned();
    };
    if head.is_empty() || rest.is_empty() {
        return name.to_owned();
    }
    let others = || names.iter().copied().filter(move |other| *other != name);
    let shared = others().any(|other| {
        other
            .split_once(NAME_SEPARATOR)
            .is_some_and(|(other_head, _)| other_head == head)
    });
    let collides = others().any(|other| {
        let remainder = other.split_once(NAME_SEPARATOR).map(|(_, r)| r);
        other == rest || remainder == Some(rest)
    });
    if shared && !collides {
        rest.to_owned()
    } else {
        name.to_owned()
    }
}

pub fn cluster_name(id: &ClusterId, snapshot: &Snapshot) -> String {
    snapshot
        .cluster(id)
        .map_or_else(|| id.to_string(), |cluster| cluster.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: [&str; 6] = [
        "prod-us-east",
        "prod-eu-west",
        "staging",
        "dev-kind",
        "qa-gke",
        "edge-k3s",
    ];

    fn at(page: Page) -> Location {
        Location::page_on(ClusterId::new("a"), page)
    }

    fn ids(panes: &Panes) -> Vec<u64> {
        panes
            .focused()
            .tabs()
            .iter()
            .map(|t| t.id().as_u64())
            .collect()
    }

    #[test]
    fn shared_leading_segments_drop_when_the_rest_stays_unique() {
        assert_eq!(short_name("prod-us-east", &DEMO), "us-east");
        assert_eq!(short_name("prod-eu-west", &DEMO), "eu-west");
        assert_eq!(short_name("staging", &DEMO), "staging");
        assert_eq!(short_name("dev-kind", &DEMO), "dev-kind");
        assert_eq!(
            short_name("prod-us-east", &["prod-us-east"]),
            "prod-us-east"
        );
        let colliding = ["prod-a", "prod-b", "a"];
        assert_eq!(short_name("prod-a", &colliding), "prod-a");
        assert_eq!(short_name("prod-b", &colliding), "b");
        let twins = ["prod-us-east", "prod-x", "dev-us-east"];
        assert_eq!(short_name("prod-us-east", &twins), "prod-us-east");
        assert_eq!(short_name("prod-x", &twins), "x");
        assert_eq!(short_name("-odd", &["-odd", "-x"]), "-odd");
    }

    #[test]
    fn log_tabs_name_the_pod_by_its_hash_suffix() {
        assert_eq!(pod_suffix("checkout-api-7f9c6d-x2lqp"), "x2lqp");
        assert_eq!(pod_suffix("etcd"), "etcd");
        assert_eq!(pod_suffix("trailing-"), "trailing-");
    }

    #[test]
    fn a_new_window_has_one_flock_home_tab() {
        let panes = Panes::default();
        assert_eq!(panes.panes().len(), 1);
        assert_eq!(panes.focused().tabs().len(), 1);
        assert_eq!(panes.active().location(), &Location::default());
    }

    #[test]
    fn opening_inserts_after_the_active_tab_and_dedupes() {
        let mut panes = Panes::default();
        let nodes = panes.open(at(Page::Nodes));
        let pods = panes.open(at(Page::Pods));
        assert_eq!(ids(&panes), [0, 1, 2]);
        assert!(panes.activate(nodes));
        let events = panes.open(at(Page::Events));
        assert_eq!(ids(&panes), [0, 1, events.as_u64(), 2]);
        assert_eq!(panes.open(at(Page::Pods)), pods);
        assert_eq!(panes.active().id(), pods);
        assert_eq!(panes.focused().tabs().len(), 4);
    }

    #[test]
    fn closing_picks_the_right_neighbour_and_the_last_tab_becomes_flock_home() {
        let mut panes = Panes::default();
        let first = panes.active().id();
        let nodes = panes.open(at(Page::Nodes));
        let pods = panes.open(at(Page::Pods));
        assert!(panes.activate(nodes));
        assert!(panes.close(nodes));
        assert_eq!(panes.active().id(), pods);
        assert!(panes.close(pods));
        assert_eq!(panes.active().id(), first);
        assert!(panes.close(first));
        assert_eq!(panes.focused().tabs().len(), 1);
        assert_ne!(panes.active().id(), first);
        assert_eq!(panes.active().location(), &Location::default());
        assert!(!panes.close(first));
    }

    #[test]
    fn closing_a_tab_left_of_the_active_one_keeps_the_active_tab() {
        let mut panes = Panes::default();
        let first = panes.active().id();
        panes.open(at(Page::Nodes));
        let pods = panes.open(at(Page::Pods));
        assert!(panes.close(first));
        assert_eq!(panes.active().id(), pods);
    }

    #[test]
    fn cycling_wraps_and_numbers_select_by_position() {
        let mut panes = Panes::default();
        panes.open(at(Page::Nodes));
        panes.open(at(Page::Pods));
        assert!(panes.cycle(Cycle::Next));
        assert_eq!(panes.focused().active_index(), 0);
        assert!(panes.cycle(Cycle::Previous));
        assert_eq!(panes.focused().active_index(), 2);
        assert!(panes.activate_nth(1));
        assert_eq!(panes.active().location(), &at(Page::Nodes));
        assert!(!panes.activate_nth(3));
        assert_eq!(panes.focused().active_index(), 1);
        let mut single = Panes::default();
        assert!(!single.cycle(Cycle::Next));
    }

    #[test]
    fn moving_a_tab_reorders_and_keeps_the_active_tab() {
        let mut panes = Panes::default();
        let first = panes.active().id();
        let nodes = panes.open(at(Page::Nodes));
        let pods = panes.open(at(Page::Pods));
        assert!(panes.move_tab(pods, 0));
        assert_eq!(ids(&panes), [pods.as_u64(), first.as_u64(), nodes.as_u64()]);
        assert_eq!(panes.active().id(), pods);
        assert!(!panes.move_tab(nodes, 9));
        assert!(panes.move_tab(first, 2));
        assert_eq!(ids(&panes), [pods.as_u64(), nodes.as_u64(), first.as_u64()]);
        assert_eq!(panes.active().id(), pods);
    }

    #[test]
    fn windows_save_and_restore_their_tabs() {
        let mut panes = Panes::default();
        panes.open(at(Page::Nodes));
        panes.open(at(Page::Pods));
        assert!(panes.activate_nth(1));
        let saved = panes.saved();
        let restored = Panes::restore(&saved);
        assert_eq!(restored.saved(), saved);
        assert_eq!(restored.active().location(), &at(Page::Nodes));
        let empty = Panes::restore(&SavedWindow {
            panes: vec![SavedPane {
                tabs: Vec::new(),
                active: 4,
            }],
            focused: 3,
        });
        assert_eq!(empty, Panes::default());
        let clamped = Panes::restore(&SavedWindow {
            panes: vec![SavedPane {
                tabs: vec![at(Page::Nodes)],
                active: 7,
            }],
            focused: 0,
        });
        assert_eq!(clamped.active().location(), &at(Page::Nodes));
    }

    #[test]
    fn unknown_tabs_are_skipped_when_reading_a_session() -> Result<(), serde_json::Error> {
        let text = r#"{"windows":[{"panes":[{"tabs":[
            {"cluster":"a","page":"Nodes"},
            {"cluster":"a","page":"NoSuchPage"},
            {"cluster":null,"page":"GitOps"}
        ],"active":1}],"focused":0}]}"#;
        let session: Session = serde_json::from_str(text)?;
        let tabs = session
            .windows
            .first()
            .and_then(|window| window.panes.first())
            .map(|pane| pane.tabs.clone())
            .unwrap_or_default();
        assert_eq!(tabs, [at(Page::Nodes), Location::flock(Page::GitOps)]);
        let json = serde_json::to_string(&session)?;
        assert_eq!(serde_json::from_str::<Session>(&json)?, session);
        Ok(())
    }

    #[test]
    fn titles_follow_the_design_kinds() {
        let snapshot = crate::flock::tests::sample();
        let flock = title(&Location::flock(Page::GitOps), &snapshot);
        assert_eq!(
            (flock.kind.as_str(), flock.label.as_str(), flock.cluster),
            (FLOCK_KIND, "GitOps", None)
        );
        let nodes = title(&at(Page::Nodes), &snapshot);
        assert_eq!(
            (nodes.kind.as_str(), nodes.label.as_str()),
            (CLUSTER_KIND, "Nodes")
        );
        assert_eq!(nodes.cluster.as_deref(), Some("a"));
        let deployments = title(&at(Page::Deployments), &snapshot);
        assert_eq!(deployments.kind, "workloads");
        let pod = crate::ResourceRef {
            cluster: ClusterId::new("a"),
            kind: "Pod".into(),
            namespace: Some("shop".into()),
            name: "cart-7f9c-x2lqp".into(),
        };
        let logs = Location {
            resource: Some(pod.clone()),
            container: Some("cart".into()),
            ..at(Page::Logs)
        };
        let logs = title(&logs, &snapshot);
        assert_eq!(
            (logs.kind.as_str(), logs.label.as_str()),
            (LOGS_KIND, "x2lqp")
        );
        let workload = Location {
            resource: Some(crate::ResourceRef {
                kind: "Deployment".into(),
                name: "checkout-api".into(),
                ..pod
            }),
            ..at(Page::Deployments)
        };
        let workload = title(&workload, &snapshot);
        assert_eq!(
            (workload.kind.as_str(), workload.label.as_str()),
            ("deploy", "checkout-api")
        );
    }
}
