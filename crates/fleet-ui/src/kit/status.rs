use fleet_core::Status;
use gpui::{
    App, HighlightStyle, IntoElement, ParentElement, RenderOnce, SharedString, Styled, StyledText,
    Window, div,
};

use crate::theme::{ActiveTheme, FONT_STATUS, GLYPH_SLOT, SPACE_3, TEXT_GLYPH};

pub(crate) const STATUSES: [Status; 6] = [
    Status::Ok,
    Status::Warn,
    Status::Err,
    Status::Progress,
    Status::Suspended,
    Status::Offline,
];

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
            .font_family(FONT_STATUS)
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

pub(crate) fn status_symbols(text: impl Into<SharedString>) -> StyledText {
    let text = text.into();
    let ranges = status_symbol_ranges(&text);
    let highlights = ranges
        .iter()
        .map(|range| (range.clone(), HighlightStyle::default()));
    let fonts = ranges
        .iter()
        .map(|range| (range.clone(), FONT_STATUS.into()));
    StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(fonts)
}

fn status_symbol_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    text.char_indices()
        .filter(|(_, character)| {
            STATUSES
                .iter()
                .any(|status| status.glyph().starts_with(*character))
        })
        .map(|(offset, character)| offset..offset + character.len_utf8())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_only_status_symbols_in_mixed_text() {
        let text = "● 12 ▲ warn ✕ failed ◐ syncing ‖ held ○ offline · –";
        let ranges = status_symbol_ranges(text);
        let symbols: Vec<_> = ranges.into_iter().map(|range| &text[range]).collect();
        assert_eq!(symbols, ["●", "▲", "✕", "◐", "‖", "○"]);
        let plain = status_symbol_ranges("healthy 12 · –");
        assert_eq!(plain, Vec::<std::ops::Range<usize>>::new());
    }
}
