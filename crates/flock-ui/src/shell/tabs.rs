use flock_core::tabs::{Tab, TabId, TabTitle, title};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, ElementId, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};

use super::Shell;
use crate::kit::{Tooltip, status_symbols};
use crate::theme::{
    ActiveTheme, FONT_MONO, FONT_STATUS, Palette, RULE, SPACE_3, SPACE_4, SPACE_6, TAB_CLOSE,
    TEXT_BODY, TEXT_GLYPH, TEXT_LABEL, TEXT_UI, TITLE_ROW,
};

const CLOSE_GLYPH: &str = "×";
const CLOSE_TOOLTIP: &str = "Close tab  ⌘W";

#[derive(Default)]
pub(super) struct TabStrip {
    scroll: ScrollHandle,
    shown: Option<TabId>,
    pub(super) announce: bool,
    pub(super) saved: Option<flock_core::tabs::SavedWindow>,
}

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
        leading: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = *cx.palette();
        let pane = self.state.panes().focused();
        let active = pane.active_index();
        let active_id = pane.active().id();
        if self.tab_strip.shown != Some(active_id) {
            self.tab_strip.shown = Some(active_id);
            self.tab_strip.scroll.scroll_to_item(active);
        }
        let tabs: Vec<(usize, Tab)> = pane.tabs().iter().cloned().enumerate().collect();
        let last = tabs.len().saturating_sub(1);
        let mut children: Vec<AnyElement> = Vec::new();
        for (ix, tab) in tabs {
            children.push(self.tab(ix, &tab, ix == active, &palette, cx));
        }
        let rest = div()
            .id("tab-strip-rest")
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(palette.hover))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                this.drop_tab(dragged.id, last, cx);
            }))
            .child(div().flex_1())
            .child(div().h(RULE).bg(palette.line));
        let tabs = div()
            .id("tab-strip-tabs")
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_x_scroll()
            .track_scroll(&self.tab_strip.scroll)
            .children(children)
            .child(rest);
        div()
            .debug_selector(|| "tab-strip".to_owned())
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
        ix: usize,
        tab: &Tab,
        active: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id();
        let title = title(tab.location(), &self.state.snapshot);
        let hover = palette.hover;
        let top = if active {
            palette.acc
        } else {
            gpui::transparent_black()
        };
        let bottom = if active { palette.editor } else { palette.line };
        let dragged = DraggedTab {
            id,
            title: title.clone(),
        };
        let close = close_button(ix, id, active, palette, cx);
        div()
            .debug_selector(move || format!("tab-{ix}"))
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
            .on_aux_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.is_middle_click() {
                    this.close_tab(id, cx);
                }
            }))
            .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(hover))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                this.drop_tab(dragged.id, ix, cx);
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

    fn drop_tab(&mut self, id: TabId, to: usize, cx: &mut Context<Self>) {
        if self.state.panes_mut().move_tab(id, to) {
            self.save_tabs(cx);
            cx.notify();
        }
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

fn close_button(
    ix: usize,
    id: TabId,
    active: bool,
    palette: &Palette,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let fg = palette.fg;
    div()
        .debug_selector(move || format!("tab-close-{ix}"))
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
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.close_tab(id, cx);
        }))
        .child(CLOSE_GLYPH)
        .into_any_element()
}
