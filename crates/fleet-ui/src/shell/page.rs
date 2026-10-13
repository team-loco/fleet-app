use fleet_core::Page;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div,
};

use super::Shell;
use crate::theme::{
    ActiveTheme, FONT_MONO, ICON_BUTTON, PAGE_HEADER, Palette, RADIUS, RULE, SPACE_2, SPACE_5,
    SPACE_7, SPACE_8, TEXT_LABEL, TEXT_SMALL, TEXT_TITLE,
};

const LIVE: &str = "live";
const MORE: &str = "⋯";

impl Shell {
    pub(super) fn page(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(palette.editor)
            .child(self.page_header(&palette))
            .child(
                div()
                    .id("page-content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.content(&palette)),
            )
            .into_any_element()
    }

    fn page_header(&self, palette: &Palette) -> impl IntoElement {
        let (status, status_color) = match self.selected_cluster() {
            Some(cluster) => {
                let text = format!("{} {}", cluster.status.glyph(), cluster.state);
                (text, palette.status(cluster.status))
            }
            None => (LIVE.to_owned(), palette.mute),
        };
        let more = self.selected_cluster().map(|_| more_button(palette));
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
            .child(
                div()
                    .font_family(FONT_MONO)
                    .text_size(TEXT_SMALL)
                    .text_color(status_color)
                    .child(status),
            )
            .child(div().flex_1())
            .children(more)
    }

    fn content(&self, palette: &Palette) -> AnyElement {
        match self.state.page {
            Page::Overview => self.fleet.clone().into_any_element(),
            Page::Problems => self.fleet.clone().into_any_element(),
            _ => self.placeholder(palette).into_any_element(),
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
