use gpui::{
    App, Entity, Focusable, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, Styled,
    Window, div, prelude::FluentBuilder,
};
use gpui_component::input::{Input, InputState};

use crate::theme::{
    ActiveTheme, CONTROL, CONTROL_SMALL, FONT_MONO, RADIUS, RULE, SEARCH_WIDTH, SPACE_3, SPACE_4,
    TEXT_MONO, TEXT_SMALL,
};

pub use gpui_component::input::{InputEvent, InputState as TextFieldState};

#[derive(IntoElement)]
pub struct TextField {
    state: Entity<InputState>,
    prefix: Option<SharedString>,
    width: Option<Pixels>,
    small: bool,
}

impl TextField {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            prefix: None,
            width: None,
            small: false,
        }
    }

    pub fn search(state: &Entity<InputState>) -> Self {
        Self::new(state).width(SEARCH_WIDTH).small()
    }

    pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let focused = self
            .state
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        let (height, text, padding) = if self.small {
            (CONTROL_SMALL, TEXT_SMALL, SPACE_3)
        } else {
            (CONTROL, TEXT_MONO, SPACE_4)
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(SPACE_3)
            .h(height)
            .px(padding)
            .rounded(RADIUS)
            .border(RULE)
            .border_color(if focused {
                palette.selline
            } else {
                palette.line2
            })
            .bg(palette.editor)
            .overflow_hidden()
            .font_family(FONT_MONO)
            .text_size(text)
            .when_some(self.width, Styled::w)
            .when(self.width.is_none(), |field| field.flex_1().min_w_0())
            .when_some(self.prefix, |field, prefix| {
                field.child(div().flex_none().text_color(palette.mute).child(prefix))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.state).appearance(false).h(height)),
            )
    }
}
