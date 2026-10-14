use flock_core::filter::TextFilter;
use gpui::{
    AnyView, AppContext as _, Context, Entity, EventEmitter, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window, div,
    prelude::FluentBuilder,
};

use super::button::{Button, Segmented};
use super::input::{InputEvent, TextField, TextFieldEscape, TextFieldState};
use crate::theme::{ActiveTheme, FILTER_WIDTH, FONT_MONO, RULE, SPACE_4, SPACE_8, TEXT_SMALL};

const CLEAR: &str = "Clear";
const PLACEHOLDER: &str = "Filter   ns:  l:   /";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterEvent {
    Changed,
}

pub fn chip_label(label: &str, count: usize) -> SharedString {
    format!("{label} {count}").into()
}

pub fn match_note(shown: usize, total: usize) -> Option<String> {
    (shown != total).then(|| format!("{shown} of {total}"))
}

pub struct FilterBar {
    id: SharedString,
    field: Entity<TextFieldState>,
    scope: Option<AnyView>,
    chips: Vec<SharedString>,
    chip: usize,
    matches: Option<(usize, usize)>,
    query: TextFilter,
    _subscription: Subscription,
}

impl EventEmitter<FilterEvent> for FilterBar {}

impl FilterBar {
    pub fn new(id: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| TextFieldState::new(window, cx).placeholder(PLACEHOLDER));
        let subscription = cx.subscribe(&field, |this, field, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                this.query = TextFilter::parse(&field.read(cx).value());
                cx.emit(FilterEvent::Changed);
                cx.notify();
            }
        });
        Self {
            id: id.into(),
            field,
            scope: None,
            chips: Vec::new(),
            chip: 0,
            matches: None,
            query: TextFilter::default(),
            _subscription: subscription,
        }
    }

    pub fn scope(mut self, picker: impl Into<AnyView>) -> Self {
        self.scope = Some(picker.into());
        self
    }

    pub fn query(&self) -> &TextFilter {
        &self.query
    }

    pub fn words(&self) -> Vec<String> {
        self.query.words.clone()
    }

    pub fn text(&self, cx: &gpui::App) -> String {
        self.field.read(cx).value().to_string()
    }

    pub fn chip(&self) -> usize {
        self.chip
    }

    pub fn is_active(&self) -> bool {
        !self.query.is_empty() || self.chip != 0
    }

    pub fn set_chips(&mut self, chips: Vec<SharedString>, cx: &mut Context<Self>) {
        if self.chips != chips {
            self.chip = self.chip.min(chips.len().saturating_sub(1));
            self.chips = chips;
            cx.notify();
        }
    }

    pub fn select_chip(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.chips.len().max(1) && ix != self.chip {
            self.chip = ix;
            cx.emit(FilterEvent::Changed);
            cx.notify();
        }
    }

    pub fn set_matches(&mut self, shown: usize, total: usize, cx: &mut Context<Self>) {
        let matches = Some((shown, total));
        if self.matches != matches {
            self.matches = matches;
            cx.notify();
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.field.update(cx, |field, cx| field.focus(window, cx));
    }

    pub fn is_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.field
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx)
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_active() {
            return;
        }
        self.field
            .update(cx, |field, cx| field.set_value("", window, cx));
        self.query = TextFilter::default();
        self.chip = 0;
        cx.emit(FilterEvent::Changed);
        cx.notify();
    }
}

impl Render for FilterBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let chips = (!self.chips.is_empty()).then(|| {
            self.chips
                .iter()
                .fold(Segmented::new(format!("{}-chips", self.id)), |seg, chip| {
                    seg.option(chip.clone())
                })
                .selected(self.chip)
                .on_select(cx.listener(|this, ix: &usize, _, cx| this.select_chip(*ix, cx)))
        });
        let clear = self.is_active().then(|| {
            Button::new(format!("{}-clear", self.id), CLEAR)
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.clear(window, cx)))
        });
        let note = self
            .matches
            .and_then(|(shown, total)| match_note(shown, total))
            .map(|note| {
                div()
                    .font_family(FONT_MONO)
                    .text_size(TEXT_SMALL)
                    .text_color(palette.mute)
                    .child(note)
            });
        let selector = self.id.to_string();
        div()
            .debug_selector(move || selector)
            .id(self.id.clone())
            .flex()
            .flex_none()
            .items_center()
            .gap(SPACE_4)
            .px(SPACE_8)
            .py(SPACE_4)
            .border_b(RULE)
            .border_color(palette.line)
            .on_action(cx.listener(|this, _: &TextFieldEscape, window, cx| {
                this.clear(window, cx);
            }))
            .when_some(self.scope.clone(), ParentElement::child)
            .child(TextField::new(&self.field).width(FILTER_WIDTH))
            .children(chips)
            .children(clear)
            .child(div().flex_1())
            .children(note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_only_a_narrowed_list() {
        assert_eq!(match_note(12, 48).as_deref(), Some("12 of 48"));
        assert_eq!(match_note(48, 48), None);
        assert_eq!(
            chip_label("Running", 398),
            SharedString::from("Running 398")
        );
    }
}
