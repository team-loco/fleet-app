use flock_core::tabs::{Tab, TabId, TabTitle, title};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, ElementId, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder,
};

use super::Shell;
use crate::kit::{Tooltip, status_symbols};
use crate::theme::{
    ActiveTheme, FONT_MONO, FONT_STATUS, Palette, RULE, SPACE_3, SPACE_4, SPACE_6, TAB_CLOSE,
    TEXT_BODY, TEXT_GLYPH, TEXT_LABEL, TEXT_UI, TITLE_ROW,
};

const CLOSE_GLYPH: &str = "×";
const CLOSE_TOOLTIP: &str = "Close tab  ⌘W";

#[derive(Clone)]
pub(super) struct DraggedTab {
    id: TabId,
    title: TabTitle,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        div()
            .flex()
            .h(TITLE_ROW)
            .bg(palette.editor)
            .border(RULE)
            .border_color(palette.line2)
            .text_color(palette.fg)
            .child(tab_content(&self.title, true, &palette))
    }
}

impl Shell {
    pub(super) fn tab_strip(
        &mut self,
        pane_ix: usize,
        leading: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = *cx.palette();
        let Some(pane) = self.state.panes().panes().get(pane_ix) else {
            return div().into_any_element();
        };
        let focused = pane_ix == self.state.panes().focused_index();
        let active = pane.active_index();
        let active_id = pane.active().id();
        let tabs: Vec<(usize, Tab)> = pane.tabs().iter().cloned().enumerate().collect();
        let screens = self.screens_mut(pane_ix);
        if screens.shown != Some(active_id) {
            screens.shown = Some(active_id);
            screens.strip.scroll_to_item(active);
        }
        let scroll = screens.strip.clone();
        let end = tabs.len();
        let mut children: Vec<AnyElement> = Vec::new();
        for (ix, tab) in tabs {
            let shown = Shown {
                pane: pane_ix,
                ix,
                active: ix == active,
                focused,
            };
            children.push(self.tab(shown, &tab, &palette, cx));
        }
        let rest = div()
            .id(("tab-strip-rest", pane_ix))
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(palette.hover))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, window, cx| {
                this.drop_tab(dragged.id, pane_ix, end, window, cx);
            }))
            .child(div().flex_1())
            .child(div().h(RULE).bg(palette.line));
        let tabs = div()
            .id(("tab-strip-tabs", pane_ix))
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_x_scroll()
            .track_scroll(&scroll)
            .children(children)
            .child(rest);
        div()
            .debug_selector(move || strip_selector(pane_ix))
            .flex()
            .flex_none()
            .w_full()
            .h(TITLE_ROW)
            .bg(palette.chrome)
            .children(leading)
            .child(tabs)
            .into_any_element()
    }

    fn tab(
        &self,
        shown: Shown,
        tab: &Tab,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Shown {
            pane,
            ix,
            active,
            focused,
        } = shown;
        let id = tab.id();
        let title = title(tab.location(), &self.state.snapshot);
        let hover = palette.hover;
        let top = if active && focused {
            palette.acc
        } else {
            gpui::transparent_black()
        };
        let bottom = if active { palette.editor } else { palette.line };
        let dragged = DraggedTab {
            id,
            title: title.clone(),
        };
        let close = close_button(shown, id, palette, cx);
        div()
            .debug_selector(move || tab_selector(pane, ix))
            .id(ElementId::NamedInteger("tab".into(), id.as_u64()))
            .flex()
            .flex_none()
            .h_full()
            .cursor_pointer()
            .text_color(palette.mute)
            .when(active, |tab| tab.bg(palette.editor).text_color(palette.fg))
            .when(!active, |tab| tab.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if this.state.panes_mut().activate(id) {
                    this.show_active(cx);
                }
            }))
            .on_aux_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.is_middle_click() {
                    this.close_tab(id, window, cx);
                }
            }))
            .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(hover))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, window, cx| {
                this.drop_tab(dragged.id, pane, ix, window, cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h_full()
                    .child(div().h(RULE).bg(top))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .child(tab_content(&title, active, palette))
                            .child(close),
                    )
                    .child(div().h(RULE).bg(bottom)),
            )
            .child(div().w(RULE).h_full().bg(palette.line))
            .into_any_element()
    }

    fn drop_tab(
        &mut self,
        id: TabId,
        pane: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.state.panes().active().id();
        if !self.state.panes_mut().move_tab(id, pane, to) {
            return;
        }
        self.sync_panes(window, cx);
        if self.state.panes().active().id() == active {
            self.save_tabs(cx);
            cx.notify();
        } else {
            self.show_active(cx);
        }
    }
}

#[derive(Clone, Copy)]
struct Shown {
    pane: usize,
    ix: usize,
    active: bool,
    focused: bool,
}

fn strip_selector(pane: usize) -> String {
    if pane == 0 {
        "tab-strip".to_owned()
    } else {
        format!("tab-strip-{pane}")
    }
}

fn tab_selector(pane: usize, ix: usize) -> String {
    if pane == 0 {
        format!("tab-{ix}")
    } else {
        format!("pane-{pane}-tab-{ix}")
    }
}

fn tab_content(title: &TabTitle, active: bool, palette: &Palette) -> impl IntoElement {
    let glyph = title.status.map(|status| {
        div()
            .font_family(FONT_STATUS)
            .text_size(TEXT_GLYPH)
            .text_color(palette.status(status))
            .child(status_symbols(status.glyph()))
    });
    let label_color: Hsla = if active { palette.fg } else { palette.mute };
    div()
        .flex()
        .items_center()
        .gap(SPACE_3)
        .pl(SPACE_6)
        .pr(SPACE_3)
        .whitespace_nowrap()
        .text_size(TEXT_BODY)
        .children(glyph)
        .child(
            div()
                .font_family(FONT_MONO)
                .text_size(TEXT_LABEL)
                .text_color(palette.mute)
                .child(SharedString::from(title.kind.clone())),
        )
        .child(
            div()
                .text_color(label_color)
                .child(SharedString::from(title.label.clone())),
        )
        .children(title.cluster.clone().map(|cluster| {
            div()
                .text_color(palette.mute)
                .child(SharedString::from(cluster))
        }))
}

fn close_button(shown: Shown, id: TabId, palette: &Palette, cx: &mut Context<Shell>) -> AnyElement {
    let Shown {
        pane, ix, active, ..
    } = shown;
    let fg = palette.fg;
    let selector = if pane == 0 {
        format!("tab-close-{ix}")
    } else {
        format!("pane-{pane}-tab-close-{ix}")
    };
    div()
        .debug_selector(move || selector.clone())
        .id(ElementId::NamedInteger("tab-close".into(), id.as_u64()))
        .flex()
        .items_center()
        .justify_center()
        .w(TAB_CLOSE)
        .mr(SPACE_4)
        .text_size(TEXT_UI)
        .text_color(palette.mute)
        .when(!active, Styled::invisible)
        .hover(move |style| style.text_color(fg))
        .tooltip(Tooltip::text(CLOSE_TOOLTIP))
        .on_mouse_down(MouseButton::Left, |_, _, cx: &mut App| {
            cx.stop_propagation();
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            this.close_tab(id, window, cx);
        }))
        .child(CLOSE_GLYPH)
        .into_any_element()
}
