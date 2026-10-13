use fleet_core::{ClusterSummary, Command, Counts, Page};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div,
};

use fleet_core::AddClusterStep;

use super::Shell;
use crate::kit::status_symbols;
use crate::screens::add_cluster::empty_fleet;
use crate::screens::workload::WorkloadScreen;
use crate::theme::{
    ActiveTheme, FONT_MONO, ICON_BUTTON, PAGE_HEADER, Palette, RADIUS, RULE, SPACE_2, SPACE_4,
    SPACE_5, SPACE_7, SPACE_8, TEXT_LABEL, TEXT_SMALL, TEXT_TITLE,
};

const LIVE: &str = "live";
const MORE: &str = "⋯";
const SEPARATOR: &str = "· ";

impl Shell {
    pub(super) fn page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(feature = "loco")]
        if crate::screens::loco::owns(self.state.page) {
            return self.loco.clone().into_any_element();
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
        let full_page: Option<AnyElement> = match self.state.page {
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
        if self.state.flux_object.is_some() {
            return self.flux_object.clone().into_any_element();
        }
        let body = if self.state.page == Page::Pods && self.has_detail() {
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

    fn page_header(&self, palette: &Palette, cx: &App) -> impl IntoElement {
        let note = if self.state.page == Page::Pods {
            Some(self.pods.read(cx).header_note())
        } else {
            self.pages
                .header_note(self.state.page, self.has_detail(), cx)
        };
        let status: Option<Vec<(String, gpui::Hsla)>> = match (self.selected_cluster(), note) {
            (Some(_), Some((counts, text))) => Some(note_parts(&counts, text, palette)),
            (Some(cluster), None) => {
                let text = format!("{} {}", cluster.status.glyph(), cluster.state);
                Some(vec![(text, palette.status(cluster.status))])
            }
            (None, _) if self.state.page == Page::GitOps => None,
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
        let more = self.selected_cluster().map(|_| more_button(palette));
        let controls = self
            .selected_cluster()
            .and_then(|_| self.pages.header_controls(self.state.page, cx));
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
                    .child(self.state.page.title()),
            )
            .children(status)
            .child(div().flex_1())
            .children(controls)
            .children(more)
    }

    fn content(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.loaded
            && self.state.snapshot.clusters.is_empty()
            && self.state.page == Page::Overview;
        if empty {
            let add = cx.listener(|this, _, window, cx| {
                this.open_add_cluster(AddClusterStep::Pick, window, cx);
            });
            return empty_fleet(palette, add).into_any_element();
        }
        match self.state.page {
            Page::Overview => self.fleet.clone().into_any_element(),
            Page::Problems => self.fleet.clone().into_any_element(),
            Page::GitOps => self.gitops.clone().into_any_element(),
            page => self
                .pages
                .render(page, self.has_detail())
                .unwrap_or_else(|| self.placeholder(palette).into_any_element()),
        }
    }

    pub(super) fn has_detail(&self) -> bool {
        self.selected_cluster()
            .is_some_and(|c| self.state.snapshot.detail(&c.id).is_some())
    }

    fn placeholder(&self, palette: &Palette) -> impl IntoElement {
        let scope: SharedString = match self.selected_cluster() {
            Some(cluster) => missing_detail(cluster).into(),
            None => format!("{} clusters", self.state.snapshot.clusters.len()).into(),
        };
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
                    .child(self.state.page.label().to_uppercase()),
            )
            .child(div().text_color(palette.fg2).child(scope))
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

fn missing_detail(cluster: &ClusterSummary) -> String {
    if cluster.nodes.is_some() {
        format!("Loading {}…", cluster.name)
    } else {
        format!("No data from {}: {}.", cluster.name, cluster.state)
    }
}

fn more_button(palette: &Palette) -> impl IntoElement {
    let hover = palette.hover;
    div()
        .id("page-actions")
        .flex()
        .items_center()
        .justify_center()
        .size(ICON_BUTTON)
        .rounded(RADIUS)
        .cursor_pointer()
        .text_color(palette.fg2)
        .hover(move |style| style.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(MORE)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fleet_core::{ClusterId, Status};

    use super::*;

    fn summary(nodes: Option<u32>, state: &str) -> ClusterSummary {
        ClusterSummary {
            id: ClusterId::new("qa-gke"),
            name: "qa-gke".into(),
            status: Status::Err,
            state: state.into(),
            distro: "GKE".into(),
            region: "us-central1".into(),
            version: None,
            nodes,
            problems: Counts::default(),
            nav: BTreeMap::new(),
            metrics: None,
        }
    }

    #[test]
    fn unreachable_clusters_say_why_they_have_no_data() {
        assert_eq!(
            missing_detail(&summary(None, "Auth expired")),
            "No data from qa-gke: Auth expired."
        );
        assert_eq!(
            missing_detail(&summary(Some(3), "Healthy")),
            "Loading qa-gke…"
        );
    }
}
