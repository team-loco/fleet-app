use std::rc::Rc;

use gpui::{
    AnchoredPositionMode, AnyElement, App, ElementId, FocusHandle, FontWeight, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels, RenderOnce, SharedString,
    Styled, Window, anchored, deferred, div, point, prelude::FluentBuilder,
};

use super::button::IconButton;
use super::popover;
use crate::theme::{
    ActiveTheme, DIALOG_TOP, PAGE_HEADER, RADIUS_MENU, RULE, SPACE_5, SPACE_7, SPACE_8, TEXT_TITLE,
};

const CLOSE: &str = "×";
const CLOSE_TOOLTIP: &str = "Close";
const ESCAPE: &str = "escape";
const MODAL_PRIORITY: usize = 5;
const BOTH_SIDES: f32 = 2.;

pub type CloseHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    title: SharedString,
    width: Pixels,
    focus: Option<FocusHandle>,
    on_close: Option<CloseHandler>,
    children: Vec<AnyElement>,
}

impl Modal {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, width: Pixels) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width,
            focus: None,
            on_close: None,
            children: Vec::new(),
        }
    }

    pub fn focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let viewport = window.viewport_size();
        let close_click = self.on_close.clone();
        let close_key = self.on_close.clone();
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(SPACE_5)
            .h(PAGE_HEADER)
            .px(SPACE_7)
            .border_b(RULE)
            .border_color(palette.line)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TEXT_TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title),
            )
            .child(
                IconButton::glyph("modal-close", CLOSE, CLOSE_TOOLTIP).on_click(
                    move |_, window, cx| {
                        if let Some(close) = &close_click {
                            close(window, cx);
                        }
                    },
                ),
            );
        let panel = div()
            .id(self.id)
            .w(self.width)
            .max_w(viewport.width - SPACE_8 * BOTH_SIDES)
            .max_h(viewport.height - DIALOG_TOP * BOTH_SIDES)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(palette.panel)
            .border(RULE)
            .border_color(palette.line2)
            .rounded(RADIUS_MENU)
            .shadow(popover::shadow(&palette))
            .when_some(self.focus, |panel, focus| panel.track_focus(&focus))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != ESCAPE {
                    return;
                }
                if let Some(close) = &close_key {
                    cx.stop_propagation();
                    close(window, cx);
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .children(self.children);
        let scrim = div()
            .id("modal-scrim")
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .justify_center()
            .items_start()
            .pt(DIALOG_TOP)
            .bg(palette.scrim)
            .occlude()
            .child(panel);
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(Pixels::ZERO, Pixels::ZERO))
                .child(scrim),
        )
        .priority(MODAL_PRIORITY)
    }
}
