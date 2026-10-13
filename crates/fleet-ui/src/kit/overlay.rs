use std::rc::Rc;

use fleet_core::Status;
use gpui::{
    Anchor, AnchoredPositionMode, Animation, AnimationExt, App, AppContext, Context, ElementId,
    Entity, FocusHandle, FontWeight, Global, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, anchored,
    deferred, div, ease_out_quint, point, prelude::FluentBuilder, relative,
};

use super::button::{Button, IconButton};
use super::popover;
use super::status::StatusGlyph;
use crate::theme::{
    ActiveTheme, DIALOG_TOP, DIALOG_WIDTH, FONT_MONO, LINE_PROSE, PAGE_HEADER, POPOVER_IN,
    RADIUS_MENU, RULE, SPACE_2, SPACE_4, SPACE_5, SPACE_6, SPACE_7, SPACE_8, STATUS_BAR, TEXT_BODY,
    TEXT_SMALL, TEXT_TITLE, TOAST_TIMEOUT, TOAST_WIDTH,
};

const CANCEL: &str = "Cancel";
const CLOSE: &str = "×";
const CLOSE_TOOLTIP: &str = "Close";
const CONFIRM_KEYS: &str = "⌘↵";
const OVERLAY_PRIORITY: usize = 10;

pub type ConfirmHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub struct ConfirmDialog {
    title: SharedString,
    body: SharedString,
    details: Vec<SharedString>,
    confirm: SharedString,
    on_confirm: ConfirmHandler,
}

impl ConfirmDialog {
    pub fn new(
        title: impl Into<SharedString>,
        body: impl Into<SharedString>,
        confirm: impl Into<SharedString>,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            details: Vec::new(),
            confirm: confirm.into(),
            on_confirm: Rc::new(on_confirm),
        }
    }

    pub fn detail(mut self, line: impl Into<SharedString>) -> Self {
        self.details.push(line.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    status: Status,
    title: SharedString,
    detail: Option<SharedString>,
}

impl Toast {
    pub fn new(status: Status, title: impl Into<SharedString>) -> Self {
        Self {
            status,
            title: title.into(),
            detail: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

struct Shown {
    id: u64,
    toast: Toast,
}

pub struct Overlays {
    dialog: Option<(u64, ConfirmDialog)>,
    toasts: Vec<Shown>,
    next: u64,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
}

struct OverlayHost(Entity<Overlays>);

impl Global for OverlayHost {}

pub fn init(cx: &mut App) {
    let overlays = cx.new(|cx| Overlays {
        dialog: None,
        toasts: Vec::new(),
        next: 0,
        focus: cx.focus_handle(),
        restore: None,
    });
    cx.set_global(OverlayHost(overlays));
}

pub fn layer(cx: &App) -> Entity<Overlays> {
    cx.global::<OverlayHost>().0.clone()
}

pub fn confirm(dialog: ConfirmDialog, window: &mut Window, cx: &mut App) {
    layer(cx).update(cx, |overlays, cx| overlays.open(dialog, window, cx));
}

pub fn toast(toast: Toast, cx: &mut App) {
    layer(cx).update(cx, |overlays, cx| overlays.push(toast, cx));
}

impl Overlays {
    fn open(&mut self, dialog: ConfirmDialog, window: &mut Window, cx: &mut Context<Self>) {
        self.next += 1;
        self.dialog = Some((self.next, dialog));
        self.restore = window.focused(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        if let Some(previous) = self.restore.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, dialog)) = self.dialog.as_ref() else {
            return;
        };
        let handler = dialog.on_confirm.clone();
        self.dismiss(window, cx);
        handler(window, cx);
    }

    fn push(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.next += 1;
        let id = self.next;
        self.toasts.push(Shown { id, toast });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_TIMEOUT).await;
            this.update(cx, |overlays, cx| overlays.remove(id, cx)).ok();
        })
        .detach();
        cx.notify();
    }

    fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        self.toasts.retain(|shown| shown.id != id);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "escape" => self.dismiss(window, cx),
            "enter" if keystroke.modifiers.platform => self.accept(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_dialog(
        &self,
        id: u64,
        dialog: &ConfirmDialog,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = *cx.palette();
        let viewport = window.viewport_size();
        let header = div()
            .flex()
            .items_center()
            .h(PAGE_HEADER)
            .px(SPACE_7)
            .border_b(RULE)
            .border_color(palette.line)
            .child(
                div()
                    .flex_1()
                    .text_size(TEXT_TITLE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(dialog.title.clone()),
            )
            .child(
                IconButton::glyph("dialog-close", CLOSE, CLOSE_TOOLTIP)
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx))),
            );
        let details = dialog.details.iter().map(|line| {
            div()
                .font_family(FONT_MONO)
                .text_size(TEXT_SMALL)
                .text_color(palette.fg)
                .child(line.clone())
        });
        let body = div()
            .flex()
            .flex_col()
            .gap(SPACE_4)
            .px(SPACE_7)
            .py(SPACE_6)
            .text_size(TEXT_BODY)
            .line_height(relative(LINE_PROSE))
            .text_color(palette.fg2)
            .child(dialog.body.clone())
            .when(!dialog.details.is_empty(), |body| {
                body.child(div().flex().flex_col().gap(SPACE_2).children(details))
            });
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(SPACE_4)
            .px(SPACE_7)
            .py(SPACE_6)
            .border_t(RULE)
            .border_color(palette.line)
            .bg(palette.chrome)
            .child(
                Button::new("dialog-cancel", CANCEL)
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx))),
            )
            .child(
                Button::new("dialog-confirm", dialog.confirm.clone())
                    .danger()
                    .shortcut(CONFIRM_KEYS)
                    .on_click(cx.listener(|this, _, window, cx| this.accept(window, cx))),
            );
        let panel = div()
            .id("dialog")
            .w(DIALOG_WIDTH)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(palette.panel)
            .border(RULE)
            .border_color(palette.line2)
            .rounded(RADIUS_MENU)
            .shadow(popover::shadow(&palette))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .child(footer);
        let key = ElementId::NamedInteger("dialog".into(), id);
        let scrim = div()
            .id("scrim")
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .justify_center()
            .items_start()
            .pt(DIALOG_TOP)
            .bg(palette.scrim)
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| this.dismiss(window, cx)),
            )
            .child(popover::appear(panel, key));
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(gpui::Pixels::ZERO, gpui::Pixels::ZERO))
                .child(scrim),
        )
        .priority(OVERLAY_PRIORITY)
    }

    fn render_toasts(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let viewport = window.viewport_size();
        let toasts: Vec<_> = self
            .toasts
            .iter()
            .map(|shown| {
                let id = shown.id;
                let toast = &shown.toast;
                let card = div()
                    .id(ElementId::NamedInteger("toast".into(), id))
                    .w(TOAST_WIDTH)
                    .flex()
                    .gap(SPACE_4)
                    .px(SPACE_6)
                    .py(SPACE_5)
                    .bg(palette.panel)
                    .border(RULE)
                    .border_color(palette.line2)
                    .rounded(RADIUS_MENU)
                    .shadow(popover::shadow(&palette))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.remove(id, cx)))
                    .child(div().pt(SPACE_2).child(StatusGlyph::new(toast.status)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(SPACE_2)
                            .min_w_0()
                            .child(div().text_size(TEXT_BODY).child(toast.title.clone()))
                            .when_some(toast.detail.clone(), |text, detail| {
                                text.child(
                                    div()
                                        .text_size(TEXT_SMALL)
                                        .text_color(palette.mute)
                                        .child(detail),
                                )
                            }),
                    );
                let animation = Animation::new(POPOVER_IN).with_easing(ease_out_quint());
                card.with_animation(
                    ElementId::NamedInteger("toast-in".into(), id),
                    animation,
                    Styled::opacity,
                )
                .into_any_element()
            })
            .collect();
        let corner = point(
            viewport.width - SPACE_8,
            viewport.height - STATUS_BAR - SPACE_8,
        );
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .anchor(Anchor::BottomRight)
                .position(corner)
                .child(div().flex().flex_col().gap(SPACE_4).children(toasts)),
        )
        .priority(OVERLAY_PRIORITY)
    }
}

impl Render for Overlays {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = self.dialog.as_ref().map(|(id, dialog)| {
            self.render_dialog(*id, dialog, window, cx)
                .into_any_element()
        });
        let toasts = (!self.toasts.is_empty()).then(|| self.render_toasts(window, cx));
        div().children(dialog).children(toasts)
    }
}
