use fleet_core::{Command, Page, Status, WorkloadKind};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div,
};

use fleet_core::AddClusterStep;

use super::Shell;
use crate::screens::add_cluster::empty_fleet;
use crate::screens::gitops::GitOpsScreen;
use crate::screens::workload::WorkloadScreen;
use crate::theme::{
    ActiveTheme, FONT_MONO, ICON_BUTTON, PAGE_HEADER, Palette, RADIUS, RULE, SPACE_2, SPACE_5,
    SPACE_7, SPACE_8, TEXT_LABEL, TEXT_SMALL, TEXT_TITLE,
};

const LIVE: &str = "live";
const MORE: &str = "⋯";

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
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(palette.editor)
            .child(self.page_header(&palette, cx))
            .child(
                div()
                    .id("page-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.content(&palette, cx)),
            )
            .into_any_element()
    }

    fn workload_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<WorkloadScreen>> {
        let Some(target) = self
            .state
            .resource
            .clone()
            .filter(|target| WorkloadKind::from_kind(&target.kind).is_some())
        else {
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
        self.workload = Some((screen.clone(), subscription));
        Some(screen)
    }

    fn page_header(&self, palette: &Palette, cx: &App) -> impl IntoElement {
        let note = self.pages.header_note(self.state.page, cx);
        let status = match (self.selected_cluster(), note) {
            (Some(_), Some((Status::Ok, text))) => Some((text, palette.mute)),
            (Some(_), Some((status, text))) => {
                Some((format!("{} {text}", status.glyph()), palette.status(status)))
            }
            (Some(cluster), None) => {
                let text = format!("{} {}", cluster.status.glyph(), cluster.state);
                Some((text, palette.status(cluster.status)))
            }
            (None, _) if self.state.page == Page::GitOps => None,
            (None, _) => Some((LIVE.to_owned(), palette.mute)),
        };
        let status = status.map(|(text, color)| {
            div()
                .font_family(FONT_MONO)
                .text_size(TEXT_SMALL)
                .text_color(color)
                .child(text)
        });
        let more = self.selected_cluster().map(|_| more_button(palette));
        let filter = (self.state.page == Page::GitOps)
            .then(|| GitOpsScreen::filter_control(&self.gitops, cx));
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
                    .child(self.state.page.label()),
            )
            .children(status)
            .child(div().flex_1())
            .children(filter)
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
            page => {
                let has_detail = self
                    .selected_cluster()
                    .is_some_and(|c| self.state.snapshot.detail(&c.id).is_some());
                self.pages
                    .render(page, has_detail)
                    .unwrap_or_else(|| self.placeholder(palette).into_any_element())
            }
        }
    }

    fn placeholder(&self, palette: &Palette) -> impl IntoElement {
        let scope: SharedString = match self.selected_cluster() {
            Some(cluster) => cluster.name.clone().into(),
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
