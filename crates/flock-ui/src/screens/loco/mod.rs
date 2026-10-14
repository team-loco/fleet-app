mod applications;
mod builds;
mod platform;
mod routes;

use flock_core::history::Location;
use flock_core::loco::LocoCluster;
use flock_core::{Command, Page, ResourceRef, Snapshot};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div,
};

use self::applications::ApplicationsScreen;
use self::builds::BuildsScreen;
use self::platform::PlatformScreen;
use self::routes::RoutesScreen;
use crate::kit::{FilterBar, PageTitle};
use crate::theme::{ActiveTheme, SPACE_7, SPACE_8};

const NOT_INSTALLED: &str = "Loco is not installed on this cluster.";

pub fn owns(page: Page) -> bool {
    matches!(
        page,
        Page::LocoPlatform | Page::LocoApplications | Page::LocoBuilds | Page::LocoRoutes
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocoEvent {
    Command(Command),
    ShowApplication(ResourceRef),
}

pub struct LocoScreens {
    pub(crate) navigation: Option<Entity<crate::kit::HeaderNavigation>>,
    page: Page,
    loco: Option<LocoCluster>,
    applications: Entity<ApplicationsScreen>,
    builds: Entity<BuildsScreen>,
    platform: Entity<PlatformScreen>,
    routes: Entity<RoutesScreen>,
    _subscriptions: [Subscription; 4],
}

impl EventEmitter<Command> for LocoScreens {}

impl LocoScreens {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let applications = cx.new(|cx| ApplicationsScreen::new(window, cx));
        let builds = cx.new(|cx| BuildsScreen::new(window, cx));
        let platform = cx.new(PlatformScreen::new);
        let routes = cx.new(|cx| RoutesScreen::new(window, cx));
        let subscriptions = [
            cx.subscribe(&applications, |_, _, command: &Command, cx| {
                cx.emit(command.clone());
            }),
            cx.subscribe(&builds, |_, _, command: &Command, cx| {
                cx.emit(command.clone());
            }),
            cx.subscribe(&platform, |this, _, event: &LocoEvent, cx| {
                this.handle(event, cx);
            }),
            cx.subscribe(&routes, |this, _, event: &LocoEvent, cx| {
                this.handle(event, cx);
            }),
        ];
        Self {
            navigation: None,
            page: Page::LocoApplications,
            loco: None,
            applications,
            builds,
            platform,
            routes,
            _subscriptions: subscriptions,
        }
    }

    fn handle(&mut self, event: &LocoEvent, cx: &mut Context<Self>) {
        match event {
            LocoEvent::Command(command) => cx.emit(command.clone()),
            LocoEvent::ShowApplication(app) => {
                let app = app.clone();
                self.applications
                    .update(cx, |screen, cx| screen.select(Some(app), cx));
                cx.emit(Command::SelectPage(Page::LocoApplications));
            }
        }
    }

    pub(crate) fn set_navigation(
        &mut self,
        navigation: Entity<crate::kit::HeaderNavigation>,
        cx: &mut Context<Self>,
    ) {
        self.navigation = Some(navigation.clone());
        self.applications
            .update(cx, |screen, _| screen.navigation = Some(navigation.clone()));
        self.builds
            .update(cx, |screen, _| screen.navigation = Some(navigation.clone()));
        self.platform
            .update(cx, |screen, _| screen.navigation = Some(navigation.clone()));
        self.routes
            .update(cx, |screen, _| screen.navigation = Some(navigation));
    }

    pub fn set_context(
        &mut self,
        location: &Location,
        snapshot: &Snapshot,
        cx: &mut Context<Self>,
    ) {
        self.page = location.page;
        let name = location
            .cluster
            .as_ref()
            .map(|id| flock_core::tabs::cluster_name(id, snapshot))
            .unwrap_or_default();
        self.applications
            .update(cx, |screen, _| screen.set_cluster_name(&name));
        self.builds
            .update(cx, |screen, _| screen.set_cluster_name(&name));
        self.routes
            .update(cx, |screen, _| screen.set_cluster_name(&name));
        let loco = location
            .cluster
            .as_ref()
            .and_then(|id| snapshot.loco.get(id));
        if self.loco.as_ref() != loco {
            self.loco = loco.cloned();
            if let Some(loco) = &self.loco {
                let applications = loco.applications.clone();
                let builds = loco.builds.clone();
                let routes = loco.routes.clone();
                let whole = loco.clone();
                self.applications
                    .update(cx, |screen, cx| screen.set_applications(applications, cx));
                self.builds
                    .update(cx, |screen, cx| screen.set_builds(builds, cx));
                self.routes
                    .update(cx, |screen, cx| screen.set_routes(routes, cx));
                self.platform
                    .update(cx, |screen, cx| screen.set_loco(whole, cx));
            }
        }
        cx.notify();
    }

    pub fn filter_bar(&self, page: Page, cx: &gpui::App) -> Option<Entity<FilterBar>> {
        self.loco.as_ref()?;
        match page {
            Page::LocoApplications => Some(self.applications.read(cx).filter_bar().clone()),
            Page::LocoBuilds => Some(self.builds.read(cx).filter_bar().clone()),
            Page::LocoRoutes => Some(self.routes.read(cx).filter_bar().clone()),
            _ => None,
        }
    }

    fn not_installed(&self, cx: &Context<Self>) -> AnyElement {
        let palette = cx.palette();
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(PageTitle::new(self.page.label()).leading(self.navigation.clone()))
            .child(
                div()
                    .px(SPACE_8)
                    .pt(SPACE_7)
                    .text_color(palette.fg2)
                    .child(NOT_INSTALLED),
            )
            .into_any_element()
    }
}

impl Render for LocoScreens {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match (self.loco.is_some(), self.page) {
            (false, _) => self.not_installed(cx),
            (true, Page::LocoBuilds) => self.builds.clone().into_any_element(),
            (true, Page::LocoPlatform) => self.platform.clone().into_any_element(),
            (true, Page::LocoRoutes) => self.routes.clone().into_any_element(),
            (true, _) => self.applications.clone().into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(cx.palette().editor)
            .child(content)
    }
}
