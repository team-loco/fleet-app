use gpui::{
    App, Context, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, svg,
};

use super::popover::{self, Dismissal, Step};
use crate::assets::CHEVRON_DOWN;
use crate::theme::{
    ActiveTheme, CONTROL, FONT_MONO, ICON, MENU_MAX_HEIGHT, MENU_WIDTH, RADIUS, ROW_TABLE, RULE,
    SPACE_4, SPACE_5, TEXT_CONTROL, TEXT_SMALL,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectEvent {
    Changed(usize),
}

pub struct Select {
    id: SharedString,
    options: Vec<SharedString>,
    selected: usize,
    width: Pixels,
    mono: bool,
    open: bool,
    highlighted: Option<usize>,
    generation: u64,
    dismissal: Dismissal,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
}

impl EventEmitter<SelectEvent> for Select {}

impl Select {
    pub fn new(
        id: impl Into<SharedString>,
        options: Vec<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            id: id.into(),
            options,
            selected: 0,
            width: MENU_WIDTH,
            mono: false,
            open: false,
            highlighted: None,
            generation: 0,
            dismissal: Dismissal::default(),
            focus: cx.focus_handle(),
            restore: None,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected_label(&self) -> Option<&SharedString> {
        self.options.get(self.selected)
    }

    pub fn set_selected(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = index.min(self.options.len().saturating_sub(1));
        cx.notify();
    }

    pub fn set_options(&mut self, options: Vec<SharedString>, cx: &mut Context<Self>) {
        self.options = options;
        self.selected = self.selected.min(self.options.len().saturating_sub(1));
        cx.notify();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
            return;
        }
        self.open = true;
        self.generation += 1;
        self.highlighted = Some(self.selected);
        self.restore = window.focused(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        if let Some(previous) = self.restore.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    fn choose(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        if ix < self.options.len() && ix != self.selected {
            self.selected = ix;
            cx.emit(SelectEvent::Changed(ix));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.options.len();
        match event.keystroke.key.as_str() {
            "down" => self.highlighted = popover::step(self.highlighted, len, |_| true, Step::Next),
            "up" => {
                self.highlighted = popover::step(self.highlighted, len, |_| true, Step::Previous);
            }
            "enter" => {
                if let Some(ix) = self.highlighted {
                    self.choose(ix, window, cx);
                }
            }
            "escape" => self.close(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for Select {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Select {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let label = self.selected_label().cloned().unwrap_or_default();
        let hover = palette.hover;
        let trigger = div()
            .id(ElementId::Name(self.id.clone()))
            .flex()
            .items_center()
            .gap(SPACE_4)
            .h(CONTROL)
            .px(SPACE_4)
            .rounded(RADIUS)
            .border(RULE)
            .border_color(palette.line2)
            .text_size(if self.mono { TEXT_SMALL } else { TEXT_CONTROL })
            .text_color(palette.fg)
            .cursor_pointer()
            .when(self.open, |trigger| trigger.bg(palette.sel))
            .when(!self.open, |trigger| {
                trigger.hover(move |style| style.bg(hover))
            })
            .child(div().truncate().child(label))
            .child(
                svg()
                    .path(CHEVRON_DOWN)
                    .size(ICON)
                    .flex_none()
                    .text_color(palette.mute),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                if this.dismissal.take_suppressed() {
                    return;
                }
                this.toggle(window, cx);
            }));
        let popover = self.open.then(|| {
            let rows: Vec<_> = self
                .options
                .iter()
                .enumerate()
                .map(|(ix, option)| {
                    let selected = ix == self.selected;
                    let highlighted = self.highlighted == Some(ix);
                    div()
                        .id(ix)
                        .flex()
                        .flex_none()
                        .items_center()
                        .h(ROW_TABLE)
                        .px(SPACE_4)
                        .rounded(RADIUS)
                        .cursor_pointer()
                        .when(highlighted && !selected, |row| row.bg(palette.hover))
                        .when(selected, |row| {
                            row.bg(palette.sel).text_color(palette.selfg)
                        })
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered {
                                this.highlighted = Some(ix);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose(ix, window, cx);
                        }))
                        .child(div().truncate().child(option.clone()))
                        .into_any_element()
                })
                .collect();
            let surface = popover::surface(&palette)
                .id("options")
                .w(self.width)
                .max_h(MENU_MAX_HEIGHT)
                .overflow_y_scroll()
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key_down))
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.dismissal.clicked_outside();
                    this.close(window, cx);
                }))
                .children(rows);
            let key = ElementId::NamedInteger(self.id.clone(), self.generation);
            popover::below_left(CONTROL, popover::appear(surface, key))
        });
        div()
            .id(ElementId::NamedInteger(self.id.clone(), 0))
            .relative()
            .flex_none()
            .gap(SPACE_5)
            .when(self.mono, |select| select.font_family(FONT_MONO))
            .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                this.dismissal.hover(*hovered);
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(trigger)
            .children(popover)
    }
}
