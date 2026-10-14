use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
    Pixels, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, hsla,
    prelude::FluentBuilder,
};

use crate::theme::{ActiveTheme, RULE, SPACE_5, SPACE_7, SPACE_8, TEXT_UI};

const ARROW: &str = "→";
const ARROW_HIDDEN: f32 = 0.;
const ARROW_SHOWN: f32 = 1.;
const ACTIVATE_KEYS: [&str; 2] = ["enter", "space"];

type Activate = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ClickableSection {
    id: ElementId,
    title: Option<AnyElement>,
    trailing: Option<AnyElement>,
    body: Vec<AnyElement>,
    pad_x: Pixels,
    pad_y: Pixels,
    gap: Pixels,
    on_activate: Option<Activate>,
}

impl ClickableSection {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            title: None,
            trailing: None,
            body: Vec::new(),
            pad_x: SPACE_8,
            pad_y: SPACE_7,
            gap: SPACE_5,
            on_activate: None,
        }
    }

    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    pub fn padding(mut self, x: Pixels, y: Pixels) -> Self {
        self.pad_x = x;
        self.pad_y = y;
        self
    }

    pub fn gap(mut self, gap: Pixels) -> Self {
        self.gap = gap;
        self
    }

    pub fn on_activate(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for ClickableSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for ClickableSection {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let selector = self.id.to_string();
        let group = SharedString::from(selector.clone());
        let focus = window.use_keyed_state((self.id.clone(), "focus"), cx, |_, cx| {
            cx.focus_handle().tab_index(0).tab_stop(true)
        });
        let handle = focus.read(cx).clone();
        let focused = handle.is_focused(window);
        let arrow_opacity = if focused { ARROW_SHOWN } else { ARROW_HIDDEN };
        let hover = palette.hover;
        let selline = palette.selline;
        let arrow = div()
            .flex_none()
            .text_size(TEXT_UI)
            .text_color(palette.mute)
            .opacity(arrow_opacity)
            .group_hover(group.clone(), |style| style.opacity(ARROW_SHOWN))
            .child(ARROW);
        let header = self.title.map(|title| {
            div()
                .flex()
                .items_center()
                .gap(SPACE_5)
                .child(title)
                .child(arrow)
                .child(div().flex_1())
                .children(self.trailing)
        });
        let click = self.on_activate.clone();
        let key = self.on_activate;
        div()
            .debug_selector(|| selector)
            .id(self.id)
            .group(group)
            .track_focus(&handle)
            .flex_1()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.gap)
            .px(self.pad_x)
            .py(self.pad_y)
            .border(RULE)
            .border_color(hsla(0., 0., 0., 0.))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .focus(move |style| style.bg(hover).border_color(selline))
            .when_some(click, |section, handler| {
                section.on_click(move |_, window, cx| handler(window, cx))
            })
            .when_some(key, |section, handler| {
                section.on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if ACTIVATE_KEYS.contains(&event.keystroke.key.as_str()) {
                        handler(window, cx);
                        cx.stop_propagation();
                    }
                })
            })
            .children(header)
            .children(self.body)
    }
}
