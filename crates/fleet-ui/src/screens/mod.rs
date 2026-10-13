pub mod add_cluster;
pub mod changes;
pub mod command;
mod events;
pub mod fleet;
mod namespaces;
mod nodes;
mod overview;
mod resources;

use fleet_core::{ClusterDetail, ClusterSummary, Command, Page, ResourceKind, ResourceRef, Status};
use gpui::{AnyElement, App, AppContext, Context, Entity, IntoElement, Subscription, Window};

pub use events::EventsScreen;
pub use namespaces::NamespacesScreen;
pub use nodes::NodesScreen;
pub use overview::OverviewScreen;
pub use resources::ResourcesScreen;

pub const NODE_KIND: &str = "Node";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenEvent {
    Command(Command),
    Navigate(Page),
}

pub struct ClusterPages {
    overview: Entity<OverviewScreen>,
    nodes: Entity<NodesScreen>,
    namespaces: Entity<NamespacesScreen>,
    events: Entity<EventsScreen>,
    resources: Entity<ResourcesScreen>,
    _subscriptions: Vec<Subscription>,
}

impl ClusterPages {
    pub fn new<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        on_event: impl Fn(&mut V, &ScreenEvent, &mut Context<V>) + Clone + 'static,
    ) -> Self {
        let overview = cx.new(OverviewScreen::new);
        let nodes = cx.new(|cx| NodesScreen::new(window, cx));
        let namespaces = cx.new(NamespacesScreen::new);
        let events = cx.new(|cx| EventsScreen::new(window, cx));
        let resources = cx.new(|cx| ResourcesScreen::new(window, cx));
        let subscriptions = vec![
            subscribe(&overview, on_event.clone(), cx),
            subscribe(&nodes, on_event.clone(), cx),
            subscribe(&namespaces, on_event.clone(), cx),
            subscribe(&events, on_event.clone(), cx),
            subscribe(&resources, on_event, cx),
        ];
        Self {
            overview,
            nodes,
            namespaces,
            events,
            resources,
            _subscriptions: subscriptions,
        }
    }

    pub fn sync(
        &self,
        page: Page,
        cluster: Option<&ClusterSummary>,
        detail: Option<&ClusterDetail>,
        cx: &mut App,
    ) {
        let (Some(cluster), Some(detail)) = (cluster, detail) else {
            return;
        };
        match page {
            Page::ClusterOverview => self
                .overview
                .update(cx, |screen, cx| screen.set(cluster, detail, cx)),
            Page::Nodes => self
                .nodes
                .update(cx, |screen, cx| screen.set(cluster, detail, cx)),
            Page::Namespaces => self
                .namespaces
                .update(cx, |screen, cx| screen.set(detail, cx)),
            Page::Events => self.events.update(cx, |screen, cx| screen.set(detail, cx)),
            other => {
                if let Some(kind) = ResourceKind::for_page(other) {
                    self.resources
                        .update(cx, |screen, cx| screen.set(cluster, kind, detail, cx));
                }
            }
        }
    }

    pub fn reveal(&self, target: &ResourceRef, cx: &mut App) {
        if target.kind == NODE_KIND {
            let name = target.name.clone();
            self.nodes.update(cx, |screen, cx| screen.reveal(name, cx));
            return;
        }
        let kind = ResourceKind::ALL
            .into_iter()
            .find(|kind| kind.name() == target.kind);
        if let (Some(kind), Some(namespace)) = (kind, target.namespace.clone()) {
            let name = target.name.clone();
            self.resources
                .update(cx, |screen, cx| screen.reveal(kind, namespace, name, cx));
        }
    }

    pub fn render(&self, page: Page, has_detail: bool) -> Option<AnyElement> {
        if !has_detail {
            return None;
        }
        let element = match page {
            Page::ClusterOverview => self.overview.clone().into_any_element(),
            Page::Nodes => self.nodes.clone().into_any_element(),
            Page::Namespaces => self.namespaces.clone().into_any_element(),
            Page::Events => self.events.clone().into_any_element(),
            other => {
                ResourceKind::for_page(other)?;
                self.resources.clone().into_any_element()
            }
        };
        Some(element)
    }

    pub fn header_note(&self, page: Page, cx: &App) -> Option<(Status, String)> {
        match page {
            Page::Nodes => Some(self.nodes.read(cx).header_note()),
            Page::Namespaces => Some(self.namespaces.read(cx).header_note()),
            Page::Events => Some(self.events.read(cx).header_note()),
            other => {
                ResourceKind::for_page(other)?;
                Some(self.resources.read(cx).header_note())
            }
        }
    }
}

fn subscribe<V: 'static, S: gpui::EventEmitter<ScreenEvent>>(
    screen: &Entity<S>,
    on_event: impl Fn(&mut V, &ScreenEvent, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Subscription {
    cx.subscribe(screen, move |view, _, event, cx| on_event(view, event, cx))
}
