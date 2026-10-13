use std::collections::BTreeMap;

use fleet_core::{
    AddClusterStep, CLUSTER_NAV, ClusterSummary, Command, Counts, FLEET_NAV, NavSection, Page,
    Status,
};
use gpui::{
    Animation, AnimationExt, AnyElement, Context, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, SharedString, StatefulInteractiveElement,
    Styled, div, ease_out_quint, prelude::FluentBuilder, svg,
};

use super::Shell;
use super::strip::{ClusterStripRow, ResourceStripRow, cluster_strip_rows, resource_strip_rows};
use crate::assets::{CHEVRON_LEFT, CHEVRON_RIGHT};
use crate::kit::{Tooltip, status_symbols};
use crate::theme::{
    ActiveTheme, CLUSTERS_PANEL, COLLAPSE, FONT_MONO, GLYPH_SLOT, ICON, ICON_BUTTON, PANEL_HEADER,
    Palette, RADIUS, RESOURCES_PANEL, ROW_CLUSTER, ROW_NAV, RULE, SPACE_1, SPACE_2, SPACE_3,
    SPACE_4, SPACE_6, SPACE_7, SPACE_8, STRIP, STRIP_ITEM, TEXT_GLYPH, TEXT_LABEL, TEXT_SMALL,
    TEXT_UI,
};

const ALL_CLUSTERS: &str = "All clusters";
const ADD_CLUSTER: &str = "Add cluster";
const CLUSTERS: &str = "Clusters";
const FLEET: &str = "Fleet";

impl Shell {
    pub(super) fn clusters_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        let content = if self.state.clusters_open {
            self.clusters_list(&palette, cx).into_any_element()
        } else {
            let rows = cluster_strip_rows(&self.state.snapshot, self.state.cluster.as_ref());
            let items = rows
                .into_iter()
                .enumerate()
                .map(|(ix, row)| cluster_strip_item(ix, row, &palette, cx))
                .collect::<Vec<_>>();
            collapsed_strip(
                "clusters-strip",
                "expand-clusters",
                &palette,
                cx,
                |this, cx| {
                    this.dispatch(Command::ToggleClustersPanel, cx);
                },
            )
            .bg(palette.chrome)
            .border_r(RULE)
            .border_color(palette.line)
            .children(items)
            .into_any_element()
        };
        animated_panel(
            "clusters-panel",
            self.clusters_generation,
            self.state.clusters_open,
            CLUSTERS_PANEL,
            content,
        )
    }

    pub(super) fn resources_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        let label = self.scope_label();
        let content = if self.state.resources_open {
            self.resources_list(label, &palette, cx).into_any_element()
        } else {
            let (sections, counts) = self.resource_scope();
            let rows = resource_strip_rows(sections, counts, self.state.page);
            let items = rows
                .into_iter()
                .map(|row| resource_strip_item(&row, &palette, cx))
                .collect::<Vec<_>>();
            collapsed_strip(
                "resources-strip",
                "expand-resources",
                &palette,
                cx,
                |this, cx| {
                    this.dispatch(Command::ToggleResourcesPanel, cx);
                },
            )
            .children(items)
            .into_any_element()
        };
        animated_panel(
            "resources-panel",
            self.resources_generation,
            self.state.resources_open,
            RESOURCES_PANEL,
            content,
        )
    }

    pub(super) fn scope_label(&self) -> SharedString {
        let selected = self.selected_cluster();
        selected.map_or_else(|| FLEET.into(), |c| c.name.clone().into())
    }

    pub(super) fn selected_cluster(&self) -> Option<&ClusterSummary> {
        let id = self.state.cluster.as_ref()?;
        self.state.snapshot.cluster(id)
    }

    fn resource_scope(&self) -> (&'static [NavSection], &BTreeMap<Page, Counts>) {
        match self.selected_cluster() {
            Some(cluster) => (CLUSTER_NAV, &cluster.nav),
            None => (FLEET_NAV, &self.state.snapshot.fleet_nav),
        }
    }

    fn clusters_list(&self, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let fleet_mode = self.state.cluster.is_none();
        let all = nav_row("all-clusters", ROW_CLUSTER, fleet_mode, palette)
            .gap(SPACE_4)
            .font_weight(FontWeight::MEDIUM)
            .when(!fleet_mode, |row| row.text_color(palette.fg2))
            .child(div().w(GLYPH_SLOT).flex_none())
            .child(ALL_CLUSTERS)
            .on_click(cx.listener(|this, _, _, cx| {
                this.dispatch(Command::SelectCluster(None), cx);
            }));
        let rows = self
            .state
            .snapshot
            .clusters
            .iter()
            .enumerate()
            .map(|(ix, cluster)| self.cluster_row(ix, cluster, palette, cx))
            .collect::<Vec<_>>();
        let add = nav_row("add-cluster", ROW_CLUSTER, false, palette)
            .gap(SPACE_4)
            .mt(SPACE_2)
            .text_color(palette.mute)
            .child(div().w(GLYPH_SLOT).flex_none().child("+"))
            .child(ADD_CLUSTER)
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_add_cluster(AddClusterStep::Pick, window, cx);
            }));
        div()
            .w(CLUSTERS_PANEL)
            .h_full()
            .flex()
            .flex_col()
            .flex_none()
            .bg(palette.chrome)
            .border_r(RULE)
            .border_color(palette.line)
            .child(panel_header(
                section_label(CLUSTERS, palette),
                SPACE_7,
                collapse_button("collapse-clusters", palette).on_click(
                    cx.listener(|this, _, _, cx| this.dispatch(Command::ToggleClustersPanel, cx)),
                ),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(SPACE_3)
                    .pb(SPACE_3)
                    .child(all)
                    .child(div().h(RULE).mx(SPACE_1).my(SPACE_3).bg(palette.line))
                    .children(rows)
                    .child(add),
            )
    }

    fn cluster_row(
        &self,
        ix: usize,
        cluster: &ClusterSummary,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.state.cluster.as_ref() == Some(&cluster.id);
        let id = cluster.id.clone();
        let group: SharedString = format!("cluster-row-{ix}").into();
        let menu = self.cluster_menus.get(&cluster.id).cloned();
        let context_menu = menu.clone();
        let name_color = if cluster.status == Status::Offline {
            palette.mute
        } else {
            palette.fg2
        };
        nav_row(
            ElementId::NamedInteger("cluster".into(), ix as u64),
            ROW_CLUSTER,
            selected,
            palette,
        )
        .gap(SPACE_4)
        .when(!selected, |row| row.text_color(name_color))
        .group(group.clone())
        .child(glyph(cluster.status, palette))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(cluster.name.clone()),
        )
        .children(menu.map(|menu| {
            let open = menu.read(cx).is_open();
            div()
                .flex_none()
                .when(!open, |slot| {
                    slot.invisible().group_hover(group, Styled::visible)
                })
                .child(menu)
        }))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.dispatch(Command::SelectCluster(Some(id.clone())), cx);
        }))
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            if let Some(menu) = &context_menu {
                menu.update(cx, |menu, cx| menu.show(window, cx));
            }
            cx.stop_propagation();
        })
        .into_any_element()
    }

    fn resources_list(
        &self,
        label: SharedString,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (sections, counts) = self.resource_scope();
        let items = sections.iter().flat_map(|section| {
            let head = section
                .label
                .map(|l| section_head(l, palette).into_any_element());
            let rows = section_rows(section, self.state.page, counts, palette, cx);
            head.into_iter().chain(rows)
        });
        let items: Vec<AnyElement> = items.collect();
        div()
            .w(RESOURCES_PANEL)
            .h_full()
            .flex()
            .flex_col()
            .flex_none()
            .child(panel_header(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(TEXT_UI)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
                SPACE_8,
                collapse_button("collapse-resources", palette).on_click(
                    cx.listener(|this, _, _, cx| this.dispatch(Command::ToggleResourcesPanel, cx)),
                ),
            ))
            .child(
                div()
                    .id("resources-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(SPACE_4)
                    .pb(SPACE_3)
                    .children(items),
            )
    }
}

fn section_rows(
    section: &NavSection,
    current: Page,
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

fn glyph(status: Status, palette: &Palette) -> impl IntoElement {
    div()
        .w(GLYPH_SLOT)
        .flex_none()
        .font_family(FONT_MONO)
        .text_size(TEXT_GLYPH)
        .text_color(palette.status(status))
        .child(status_symbols(status.glyph()))
}

fn nav_row(
    id: impl Into<ElementId>,
    height: Pixels,
    selected: bool,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    let hover = palette.hover;
    div()
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

fn section_label(text: &str, palette: &Palette) -> gpui::Div {
    div()
        .flex_1()
        .text_size(TEXT_LABEL)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.mute)
        .child(text.to_uppercase())
}

fn section_head(text: &str, palette: &Palette) -> impl IntoElement {
    section_label(text, palette)
        .pt(SPACE_6)
        .pb(SPACE_2)
        .px(SPACE_4)
}

fn panel_header(
    label: impl IntoElement,
    left_pad: Pixels,
    button: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .flex_none()
        .h(PANEL_HEADER)
        .pl(left_pad)
        .pr(SPACE_3)
        .child(label)
        .child(button)
}

fn collapse_button(id: &'static str, palette: &Palette) -> gpui::Stateful<gpui::Div> {
    icon_button(id, CHEVRON_LEFT, palette)
}

fn icon_button(
    id: &'static str,
    path: &'static str,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    let hover = palette.hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .size(ICON_BUTTON)
        .rounded(RADIUS)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(svg().path(path).size(ICON).text_color(palette.mute))
}

fn collapsed_strip(
    id: &'static str,
    expand_id: &'static str,
    palette: &Palette,
    cx: &mut Context<Shell>,
    toggle: fn(&mut Shell, &mut Context<Shell>),
) -> gpui::Stateful<gpui::Div> {
    let expand = icon_button(expand_id, CHEVRON_RIGHT, palette)
        .on_click(cx.listener(move |this, _, _, cx| toggle(this, cx)));
    div()
        .id(id)
        .w(STRIP)
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(SPACE_2)
        .pt(SPACE_3)
        .child(expand)
}

fn strip_item(
    id: ElementId,
    selected: bool,
    tooltip: SharedString,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    let hover = palette.hover;
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_none()
        .w(STRIP_ITEM)
        .py(SPACE_2)
        .rounded(RADIUS)
        .cursor_pointer()
        .font_family(FONT_MONO)
        .when(selected, |item| item.bg(palette.sel))
        .when(!selected, |item| item.hover(move |style| style.bg(hover)))
        .tooltip(Tooltip::text(tooltip))
}

fn cluster_strip_item(
    ix: usize,
    row: ClusterStripRow,
    palette: &Palette,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let color = row
        .status
        .map_or(palette.fg2, |status| palette.status(status));
    let target = row.target;
    let key = ElementId::NamedInteger("cluster-strip".into(), ix as u64);
    strip_item(key, row.selected, row.tooltip.into(), palette)
        .text_size(TEXT_GLYPH)
        .text_color(color)
        .child(status_symbols(row.glyph))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.dispatch(Command::SelectCluster(target.clone()), cx);
        }))
        .into_any_element()
}

fn resource_strip_item(
    row: &ResourceStripRow,
    palette: &Palette,
    cx: &mut Context<Shell>,
) -> AnyElement {
    let page = row.page;
    let key = ElementId::NamedInteger("resource-strip".into(), page as u64);
    strip_item(key, row.selected, row.tooltip.into(), palette)
        .text_size(TEXT_SMALL)
        .text_color(palette.status(row.status))
        .child(
            div()
                .text_size(TEXT_GLYPH)
                .child(status_symbols(row.status.glyph())),
        )
        .child(row.count.to_string())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.dispatch(Command::SelectPage(page), cx);
        }))
        .into_any_element()
}

fn animated_panel(
    id: &'static str,
    generation: usize,
    open: bool,
    open_width: Pixels,
    content: AnyElement,
) -> AnyElement {
    let (from, to) = if open {
        (STRIP, open_width)
    } else {
        (open_width, STRIP)
    };
    let panel = div()
        .h_full()
        .flex_none()
        .overflow_hidden()
        .w(to)
        .child(content);
    if generation == 0 {
        return panel.into_any_element();
    }
    let animation = Animation::new(COLLAPSE).with_easing(ease_out_quint());
    let key = ElementId::NamedInteger(id.into(), generation as u64);
    panel
        .with_animation(key, animation, move |panel, delta| {
            panel.w(from + (to - from) * delta)
        })
        .into_any_element()
}
