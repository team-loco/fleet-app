use fleet_core::{Command, Counts, Page};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, StatefulInteractiveElement, Styled, Window, div,
};

use fleet_core::AddClusterStep;

use super::Shell;
use crate::kit::{Button, EmptyState, Skeleton, status_symbols};
use crate::screens::add_cluster::empty_fleet;
use crate::screens::logs::LogsScreen;
use crate::screens::workload::WorkloadScreen;
use crate::theme::{
    ActiveTheme, FONT_MONO, PAGE_HEADER, Palette, RULE, SPACE_2, SPACE_4, SPACE_5, SPACE_7,
    SPACE_8, TEXT_LABEL, TEXT_SMALL, TEXT_TITLE,
};

const LIVE: &str = "live";
const SEPARATOR: &str = "· ";
const UNAVAILABLE_REASON: &str =
    "It was removed from Fleet or its kubeconfig context is gone, so this tab has nothing to show.";
const SHOW_ALL_CLUSTERS: &str = "Show all clusters";

impl Shell {
    pub(super) fn page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.loaded && !self.state.cluster_available() {
            return self.unavailable(cx);
        }
        #[cfg(feature = "loco")]
        if crate::screens::loco::owns(self.state.location().page) {
            return self.loco.clone().into_any_element();
        }
        if let Some(screen) = self.logs_screen(window, cx) {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(screen)
                .into_any_element();
        }
        if let Some(screen) = self.workload_screen(window, cx) {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(screen)
                .into_any_element();
        }
        let palette = *cx.palette();
        let full_page: Option<AnyElement> = match self.state.location().page {
            Page::Changes => Some(self.changes.clone().into_any_element()),
            Page::Compare => Some(self.compare.clone().into_any_element()),
            Page::ClusterSettings => Some(self.settings.clone().into_any_element()),
            Page::Settings => Some(self.app_settings.clone().into_any_element()),
            _ => None,
        };
        if let Some(screen) = full_page {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .bg(palette.editor)
                .child(screen)
                .into_any_element();
        }
        if self.state.location().flux_object.is_some() {
            return self.flux_object.clone().into_any_element();
        }
        let loading = self.load_view(cx);
        let body = if let Some(view) = loading {
            div()
                .id("page-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(view)
                .into_any_element()
        } else if self.state.location().page == Page::Pods && self.has_detail() {
            div()
                .flex_1()
                .min_h_0()
                .child(self.pods.clone())
                .into_any_element()
        } else {
            div()
                .id("page-content")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(self.content(&palette, cx))
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(palette.editor)
            .child(self.page_header(&palette, cx))
            .child(body)
            .into_any_element()
    }

    fn workload_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<WorkloadScreen>> {
        let Some(target) = self.state.workload().cloned() else {
            self.workload = None;
            return None;
        };
        let current = self
            .workload
            .as_ref()
            .filter(|(screen, _)| screen.read(cx).target() == &target)
            .map(|(screen, _)| screen.clone());
        if current.is_some() {
            return current;
        }
        let screen = cx.new(|cx| WorkloadScreen::new(target, window, cx));
        let subscription = cx.subscribe(&screen, |shell, _, command: &Command, cx| {
            shell.dispatch(command.clone(), cx);
        });
        screen.update(cx, WorkloadScreen::watch);
        self.sync_workload_yaml(&screen, cx);
        self.workload = Some((screen.clone(), subscription));
        Some(screen)
    }

    fn logs_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<LogsScreen>> {
        let location = self.state.location();
        let target = location
            .resource
            .clone()
            .zip(location.container.clone())
            .filter(|_| location.page == Page::Logs);
        let Some((pod, container)) = target else {
            self.logs = None;
            return None;
        };
        let current = self
            .logs
            .as_ref()
            .filter(|(screen, _)| screen.read(cx).shows(&pod, &container))
            .map(|(screen, _)| screen.clone());
        if current.is_some() {
            return current;
        }
        let screen = cx.new(|cx| LogsScreen::new(pod, container, window, cx));
        let subscription = cx.subscribe(&screen, |shell, _, command: &Command, cx| {
            shell.dispatch(command.clone(), cx);
        });
        screen.update(cx, LogsScreen::watch);
        self.logs = Some((screen.clone(), subscription));
        Some(screen)
    }

    fn unavailable(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        let name = self
            .state
            .location()
            .cluster
            .as_ref()
            .map(|id| fleet_core::tabs::cluster_name(id, &self.state.snapshot))
            .unwrap_or_default();
        let sentence = format!("{name} is not available. {UNAVAILABLE_REASON}");
        let action = Button::new("tab-show-fleet", SHOW_ALL_CLUSTERS)
            .on_click(cx.listener(|this, _, _, cx| this.retarget(None, cx)));
        div()
            .debug_selector(|| "cluster-unavailable".to_owned())
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(palette.editor)
            .child(EmptyState::new(sentence, action))
            .into_any_element()
    }

    fn page_header(&self, palette: &Palette, cx: &App) -> impl IntoElement {
        let note = if self.state.location().page == Page::Pods {
            Some(self.pods.read(cx).header_note())
        } else {
            self.pages
                .header_note(self.state.location().page, self.has_detail(), cx)
        };
        let status: Option<Vec<(String, gpui::Hsla)>> = match (self.selected_cluster(), note) {
            (Some(_), Some((counts, text))) => Some(note_parts(&counts, text, palette)),
            (Some(cluster), None) => {
                let text = format!("{} {}", cluster.status.glyph(), cluster.state);
                Some(vec![(text, palette.status(cluster.status))])
            }
            (None, _) if self.state.location().page == Page::GitOps => None,
            (None, _) => Some(vec![(LIVE.to_owned(), palette.mute)]),
        };
        let status = status.map(|parts| {
            div()
                .flex()
                .gap(SPACE_4)
                .font_family(FONT_MONO)
                .text_size(TEXT_SMALL)
                .children(
                    parts
                        .into_iter()
                        .map(|(text, color)| div().text_color(color).child(status_symbols(text))),
                )
        });
        let controls = self
            .selected_cluster()
            .and_then(|_| self.pages.header_controls(self.state.location().page, cx));
        div()
            .flex()
            .items_center()
            .flex_none()
            .gap(SPACE_5)
            .h(PAGE_HEADER)
            .px(SPACE_8)
            .border_b(RULE)
            .border_color(palette.line)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .child(
                div()
                    .text_size(TEXT_TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.state.location().page.title()),
            )
            .children(status)
            .child(div().flex_1())
            .children(controls)
    }

    fn content(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.loaded
            && self.state.snapshot.clusters.is_empty()
            && self.state.location().page == Page::Overview;
        if empty {
            let add = cx.listener(|this, _, window, cx| {
                this.open_add_cluster(AddClusterStep::Pick, window, cx);
            });
            return empty_fleet(palette, add).into_any_element();
        }
        match self.state.location().page {
            Page::Overview => self.fleet.clone().into_any_element(),
            Page::Problems => self.fleet.clone().into_any_element(),
            Page::GitOps => self.gitops.clone().into_any_element(),
            page => self
                .pages
                .render(page, self.has_detail())
                .unwrap_or_else(|| self.placeholder(palette)),
        }
    }

    pub(super) fn has_detail(&self) -> bool {
        self.selected_cluster()
            .is_some_and(|c| self.state.snapshot.detail(&c.id).is_some())
    }

    fn placeholder(&self, palette: &Palette) -> AnyElement {
        if self.selected_cluster().is_some() {
            return Skeleton::rows().into_any_element();
        }
        let scope = format!("{} clusters", self.state.snapshot.clusters.len());
        div()
            .flex()
            .flex_col()
            .gap(SPACE_2)
            .px(SPACE_8)
            .pt(SPACE_7)
            .child(
                div()
                    .text_size(TEXT_LABEL)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.mute)
                    .child(self.state.location().page.label().to_uppercase()),
            )
            .child(div().text_color(palette.fg2).child(scope))
            .into_any_element()
    }
}

fn note_parts(counts: &Counts, text: String, palette: &Palette) -> Vec<(String, gpui::Hsla)> {
    let mut parts: Vec<(String, gpui::Hsla)> = counts
        .badges()
        .into_iter()
        .map(|(status, n)| (format!("{} {n}", status.glyph()), palette.status(status)))
        .collect();
    let text = if parts.is_empty() {
        text
    } else {
        format!("{SEPARATOR}{text}")
    };
    parts.push((text, palette.mute));
    parts
}
