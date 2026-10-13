use fleet_core::Status;
use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div};

use crate::theme::{ActiveTheme, FONT_MONO, GLYPH_SLOT, SPACE_3, TEXT_GLYPH};

#[derive(IntoElement)]
pub struct StatusGlyph {
    status: Status,
}

impl StatusGlyph {
    pub fn new(status: Status) -> Self {
        Self { status }
    }
}

impl RenderOnce for StatusGlyph {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .w(GLYPH_SLOT)
            .flex_none()
            .font_family(FONT_MONO)
            .text_size(TEXT_GLYPH)
            .text_color(cx.palette().status(self.status))
            .child(self.status.glyph())
    }
}

#[derive(IntoElement)]
pub struct StatusText {
    status: Status,
    label: SharedString,
}

impl StatusText {
    pub fn new(status: Status, label: impl Into<SharedString>) -> Self {
        Self {
            status,
            label: label.into(),
        }
    }
}

impl RenderOnce for StatusText {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(SPACE_3)
            .min_w_0()
            .whitespace_nowrap()
            .text_color(cx.palette().status(self.status))
            .child(StatusGlyph::new(self.status))
            .child(div().truncate().child(self.label))
    }
}
