pub mod add_cluster;
pub mod app_settings;
pub mod changes;
pub mod command;
pub mod compare;
mod events;
pub mod fleet;
pub(crate) mod flux_object;
pub mod gitops;
mod namespaces;
mod nodes;
mod overview;
pub mod pods;
mod resources;
pub mod settings;
pub mod workload;

use fleet_core::{ClusterDetail, ClusterSummary, Command, Counts, Page, ResourceKind, ResourceRef};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, IntoElement, Subscription, Window,
};

use crate::kit::FilterBar;

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
        let namespaces = cx.new(|cx| NamespacesScreen::new(window, cx));
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
        confirm_destructive: bool,
        cx: &mut App,
    ) {
        let (Some(cluster), Some(detail)) = (cluster, detail) else {
            return;
        };
        match page {
            Page::ClusterOverview => self
                .overview
                .update(cx, |screen, cx| screen.set(cluster, detail, cx)),
            Page::Nodes => self.nodes.update(cx, |screen, cx| {
                screen.set(cluster, detail, confirm_destructive, cx);
            }),
            Page::Namespaces => self
                .namespaces
                .update(cx, |screen, cx| screen.set(&cluster.id, detail, cx)),
            Page::Events => self
                .events
                .update(cx, |screen, cx| screen.set(&cluster.id, detail, cx)),
            other => {
                if let Some(kind) = ResourceKind::for_page(other) {
                    self.resources.update(cx, |screen, cx| {
                        screen.set(cluster, kind, detail, confirm_destructive, cx);
                    });
                }
            }
        }
    }

    pub fn reveal(&self, target: &ResourceRef, cx: &mut App) {
        if target.kind == NODE_KIND {
            let name = target.name.clone();
            let cluster = target.cluster.clone();
            self.nodes
                .update(cx, |screen, cx| screen.reveal(cluster, name, cx));
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

    pub fn filter_bar(&self, page: Page, cx: &App) -> Option<Entity<FilterBar>> {
        let bar = match page {
            Page::Nodes => self.nodes.read(cx).filter_bar(),
            Page::Namespaces => self.namespaces.read(cx).filter_bar(),
            Page::Events => self.events.read(cx).filter_bar(),
            other => {
                ResourceKind::for_page(other)?;
                self.resources.read(cx).filter_bar()
            }
        };
        Some(bar.clone())
    }

    pub fn close_inspector(&self, page: Page, cx: &mut App) -> bool {
        match page {
            Page::Nodes => self.nodes.update(cx, NodesScreen::close_inspector),
            other if ResourceKind::for_page(other).is_some() => {
                self.resources.update(cx, ResourcesScreen::close_inspector)
            }
            _ => false,
        }
    }

    pub fn list_focus(&self, page: Page, cx: &mut App) -> Option<FocusHandle> {
        match page {
            Page::Nodes => Some(self.nodes.read(cx).list_focus(cx)),
            Page::Namespaces => Some(self.namespaces.read(cx).list_focus(cx)),
            Page::Events => Some(self.events.read(cx).list_focus(cx)),
            other => {
                ResourceKind::for_page(other)?;
                Some(self.resources.update(cx, ResourcesScreen::list_focus))
            }
        }
    }

    pub fn header_controls(&self, page: Page, cx: &App) -> Option<AnyElement> {
        (page == Page::ClusterOverview)
            .then(|| OverviewScreen::header_controls(&self.overview, cx).into_any_element())
    }

    pub fn header_note(&self, page: Page, has_detail: bool, cx: &App) -> Option<(Counts, String)> {
        if !has_detail {
            return None;
        }
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

pub(crate) fn init(_: &mut App) {}
#[cfg(feature = "loco")]
pub mod loco;
