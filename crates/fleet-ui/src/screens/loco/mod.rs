mod applications;
mod builds;

use fleet_core::loco::LocoCluster;
use fleet_core::{AppState, Command, Page};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div,
};

use self::applications::ApplicationsScreen;
use self::builds::BuildsScreen;
use crate::kit::PageTitle;
use crate::theme::{ActiveTheme, SPACE_7, SPACE_8};

const NOT_INSTALLED: &str = "Loco is not installed on this cluster.";

pub fn owns(page: Page) -> bool {
    matches!(page, Page::LocoApplications | Page::LocoBuilds)
}

pub struct LocoScreens {
    page: Page,
    loco: Option<LocoCluster>,
    applications: Entity<ApplicationsScreen>,
    builds: Entity<BuildsScreen>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<Command> for LocoScreens {}

impl LocoScreens {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let applications = cx.new(|cx| ApplicationsScreen::new(window, cx));
        let builds = cx.new(BuildsScreen::new);
        let subscriptions = [
            cx.subscribe(&applications, |_, _, command: &Command, cx| {
                cx.emit(command.clone());
            }),
            cx.subscribe(&builds, |_, _, command: &Command, cx| {
                cx.emit(command.clone());
            }),
        ];
        Self {
            page: Page::LocoApplications,
            loco: None,
            applications,
            builds,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_context(&mut self, state: &AppState, cx: &mut Context<Self>) {
        self.page = state.page;
        let loco = state
            .cluster
            .as_ref()
            .and_then(|id| state.snapshot.loco.get(id));
        if self.loco.as_ref() != loco {
            self.loco = loco.cloned();
            if let Some(loco) = &self.loco {
                let applications = loco.applications.clone();
                let builds = loco.builds.clone();
                self.applications
                    .update(cx, |screen, cx| screen.set_applications(applications, cx));
                self.builds
                    .update(cx, |screen, cx| screen.set_builds(builds, cx));
            }
        }
        cx.notify();
    }

    fn not_installed(&self, cx: &Context<Self>) -> AnyElement {
        let palette = cx.palette();
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(PageTitle::new(self.page.label()))
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
