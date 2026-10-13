use std::collections::BTreeMap;

use fleet_core::{ClusterSummary, Command, Counts, NavSection, Page};
use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, StatefulInteractiveElement, Styled, Window,
    div, ease_out_quint, prelude::FluentBuilder, svg,
};

use super::Shell;
use super::keys::SIDEBAR_CONTEXT;
use crate::assets::{CHEVRON_LEFT, CHEVRON_RIGHT};
use crate::kit::{Tooltip, status_symbols};
use crate::theme::{
    ActiveTheme, COLLAPSE, FONT_MONO, ICON, ICON_BUTTON, Palette, RADIUS, ROW_NAV, RULE, SIDEBAR,
    SIDEBAR_COLLAPSED, SPACE_2, SPACE_3, SPACE_4, SPACE_6, STRIP, TEXT_LABEL, TEXT_SMALL,
};

const COLLAPSE_TOOLTIP: &str = "Collapse sidebar  ⌘B";
const EXPAND_TOOLTIP: &str = "Expand sidebar  ⌘B";

impl Shell {
    pub(super) fn sidebar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        let open = self.state.sidebar_open;
        let cursor = self
            .keys
            .sidebar_focus
            .is_focused(window)
            .then(|| self.keys.nav_cursor.unwrap_or(self.state.page));
        let content = if open {
            self.sidebar_open(cursor, &palette, cx).into_any_element()
        } else {
            sidebar_collapsed(&palette, cx).into_any_element()
        };
        let (from, to) = if open {
            (SIDEBAR_COLLAPSED, SIDEBAR)
        } else {
            (SIDEBAR, SIDEBAR_COLLAPSED)
        };
        let panel = div()
            .debug_selector(|| "sidebar".to_owned())
            .id("sidebar")
            .h_full()
            .flex_none()
            .overflow_hidden()
            .w(to)
            .bg(palette.panel)
            .border_r(RULE)
            .border_color(palette.line)
            .child(content);
        if self.sidebar_generation == 0 {
            return panel.into_any_element();
        }
        let animation = Animation::new(COLLAPSE).with_easing(ease_out_quint());
        let key = ElementId::NamedInteger("sidebar".into(), self.sidebar_generation as u64);
        panel
            .with_animation(key, animation, move |panel, delta| {
                panel.w(from + (to - from) * delta)
            })
            .into_any_element()
    }

    pub(super) fn selected_cluster(&self) -> Option<&ClusterSummary> {
        let id = self.state.cluster.as_ref()?;
        self.state.snapshot.cluster(id)
    }

    fn nav_counts(&self) -> &BTreeMap<Page, Counts> {
        match self.selected_cluster() {
            Some(cluster) => &cluster.nav,
            None => &self.state.snapshot.fleet_nav,
        }
    }

    fn sidebar_open(
        &self,
        cursor: Option<Page>,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sections = self
            .state
            .snapshot
            .nav_sections(self.state.cluster.as_ref());
        let counts = self.nav_counts();
        let mut items: Vec<AnyElement> = Vec::new();
        for section in sections {
            if let Some(label) = section.label {
                items.push(section_head(label, palette).into_any_element());
            }
            items.extend(section_rows(
                section,
                self.state.page,
                cursor,
                counts,
                palette,
                cx,
            ));
        }
        let collapse = icon_button("collapse-sidebar", CHEVRON_LEFT, COLLAPSE_TOOLTIP, palette)
            .on_click(cx.listener(|this, _, _, cx| this.dispatch(Command::ToggleSidebar, cx)));
        div()
            .w(SIDEBAR)
            .h_full()
            .flex()
            .flex_col()
            .child(top_strip(palette).justify_end().pr(SPACE_3).child(collapse))
            .child(
                div()
                    .id("sidebar-scroll")
                    .debug_selector(|| "sidebar-nav".to_owned())
                    .key_context(SIDEBAR_CONTEXT)
                    .track_focus(&self.keys.sidebar_focus)
                    .on_key_down(cx.listener(Self::sidebar_key))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(SPACE_4)
                    .pt(SPACE_3)
                    .pb(SPACE_3)
                    .children(items),
            )
    }
}

fn sidebar_collapsed(palette: &Palette, cx: &mut Context<Shell>) -> impl IntoElement {
    let expand = icon_button("expand-sidebar", CHEVRON_RIGHT, EXPAND_TOOLTIP, palette)
        .on_click(cx.listener(|this, _, _, cx| this.dispatch(Command::ToggleSidebar, cx)));
    div()
        .w(SIDEBAR_COLLAPSED)
        .h_full()
        .flex()
        .flex_col()
        .child(top_strip(palette))
        .child(div().flex().pt(SPACE_3).px(SPACE_4).child(expand))
}

fn top_strip(palette: &Palette) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(STRIP)
        .flex_none()
        .bg(palette.chrome)
        .border_b(RULE)
        .border_color(palette.line)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

fn section_rows(
    section: &NavSection,
    current: Page,
    cursor: Option<Page>,
    counts: &BTreeMap<Page, Counts>,
    palette: &Palette,
    cx: &mut Context<Shell>,
) -> Vec<AnyElement> {
    section
        .pages
        .iter()
        .map(|&page| {
            let selected = page == current;
            let badges = counts
                .get(&page)
                .map(|c| badges(c, palette))
                .unwrap_or_default();
            let key = ElementId::NamedInteger("page".into(), page as u64);
            nav_row(key, ROW_NAV, selected, palette)
                .when(!selected, |row| row.text_color(palette.fg2))
                .border(RULE)
                .border_color(if cursor == Some(page) {
                    palette.focus
                } else {
                    gpui::transparent_black()
                })
                .child(div().flex_1().truncate().child(page.label()))
                .children(badges)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.dispatch(Command::SelectPage(page), cx);
                }))
                .into_any_element()
        })
        .collect()
}

fn badges(counts: &Counts, palette: &Palette) -> Vec<AnyElement> {
    let flagged = counts
        .badges()
        .into_iter()
        .map(|(status, n)| badge(format!("{} {n}", status.glyph()), palette.status(status)));
    let total = counts.total.map(|n| badge(n.to_string(), palette.mute));
    flagged.chain(total).collect()
}

fn badge(text: String, color: Hsla) -> AnyElement {
    div()
        .ml(SPACE_4)
        .font_family(FONT_MONO)
        .text_size(TEXT_SMALL)
        .text_color(color)
        .child(status_symbols(text))
        .into_any_element()
}

fn nav_row(
    id: impl Into<ElementId>,
    height: Pixels,
    selected: bool,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    let hover = palette.hover;
    let id = id.into();
    div()
        .debug_selector(|| id.to_string())
        .id(id)
        .flex()
        .items_center()
        .h(height)
        .px(SPACE_4)
        .rounded(RADIUS)
        .cursor_pointer()
        .when(selected, |row| {
            row.bg(palette.sel)
                .text_color(palette.selfg)
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!selected, |row| row.hover(move |style| style.bg(hover)))
}

fn section_head(text: &str, palette: &Palette) -> impl IntoElement {
    div()
        .pt(SPACE_6)
        .pb(SPACE_2)
        .px(SPACE_4)
        .text_size(TEXT_LABEL)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.mute)
        .child(text.to_uppercase())
}

fn icon_button(
    id: &'static str,
    path: &'static str,
    tooltip: &'static str,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    let hover = palette.hover;
    div()
        .debug_selector(|| id.to_owned())
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .size(ICON_BUTTON)
        .rounded(RADIUS)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .tooltip(Tooltip::text(tooltip))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(svg().path(path).size(ICON).text_color(palette.mute))
}
