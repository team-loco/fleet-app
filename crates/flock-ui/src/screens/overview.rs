use flock_core::events::Event;
use flock_core::nodes::NodeSummary;
use flock_core::overview::{
    Capacity, PodPhases, TimeRange, Utilization, WorkloadHealth, percent, thousands,
};
use flock_core::time::age;
use flock_core::{ClusterDetail, ClusterSummary, Page, Status};
use gpui::{
    AnyElement, App, AppContext, Context, ElementId, Entity, EventEmitter, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use super::ScreenEvent;
use crate::kit::{
    Button, CapacityMeter, ClickableSection, Column, LineChart, Link, MetaLine, SectionHeader,
    SectionLabel, Segmented, Series, StackedBar, StatusText, Table, TableEvent, Tooltip,
};
use crate::theme::{
    ActiveTheme, FONT_MONO, METER, Palette, RADIUS_SMALL, RULE, SPACE_1, SPACE_3, SPACE_4, SPACE_5,
    SPACE_7, SPACE_8, TEXT_BODY, TEXT_CONTROL, TEXT_FIGURE, TEXT_MONO, TEXT_SMALL,
};

const RECENT_WARNINGS: usize = 5;
const LIVE: &str = "Live";
const LIVE_TOOLTIP: &str = "Follow new data; turn off to hold the current view";
const CHART_MAX: f32 = 100.;
const Y_LABELS: [&str; 5] = ["100%", "75%", "50%", "25%", "0"];
const SEPARATOR: &str = " · ";
const ALL_EVENTS: &str = "All events →";
const UTILIZATION: &str = "Utilization";
const CHART_WEIGHT: f32 = 2.;
const REASON_WIDTH: Pixels = px(150.);
const OBJECT_WEIGHT: f32 = 1.2;
const OBJECT_MIN: Pixels = px(230.);
const NAMESPACE_WIDTH: Pixels = px(110.);
const MESSAGE_WEIGHT: f32 = 2.;
const MESSAGE_MIN: Pixels = px(260.);
const AGE_WIDTH: Pixels = px(56.);
const COUNT_WIDTH: Pixels = px(44.);

#[derive(Clone, Debug, PartialEq)]
struct OverviewData {
    meta: String,
    cpu: Capacity,
    memory: Capacity,
    pods: PodPhases,
    nodes: NodeSummary,
    node_cells: Vec<(Status, String)>,
    utilization: Vec<Utilization>,
    workloads: Vec<WorkloadHealth>,
    warnings: usize,
}

impl OverviewData {
    fn new(cluster: &ClusterSummary, detail: &ClusterDetail) -> Self {
        let version = cluster.version.clone().unwrap_or_default();
        let namespaces = format!("{} namespaces", detail.namespaces.len());
        let metrics = format!("metrics {}", detail.overview.metrics_source);
        let meta = [
            cluster.distro.clone(),
            version,
            cluster.region.clone(),
            namespaces,
            metrics,
        ];
        let node_cells = detail
            .nodes
            .iter()
            .map(|n| (n.status, format!("{}{SEPARATOR}{}", n.name, n.state)))
            .collect();
        Self {
            meta: meta.join(SEPARATOR),
            cpu: detail.cpu(),
            memory: detail.memory(),
            pods: detail.overview.pods,
            nodes: detail.node_summary(),
            node_cells,
            utilization: detail.overview.utilization.clone(),
            workloads: detail.workload_health(),
            warnings: detail.warnings(),
        }
    }
}

pub struct OverviewScreen {
    data: Option<OverviewData>,
    paused: Option<(OverviewData, Vec<Event>)>,
    live: bool,
    range: TimeRange,
    warnings: Entity<Table<Event>>,
}

impl EventEmitter<ScreenEvent> for OverviewScreen {}

impl OverviewScreen {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let warnings = cx.new(|cx| {
            Table::new("overview-warnings", warning_columns(), cx)
                .empty("No warnings.")
                .fit_content()
        });
        cx.subscribe(&warnings, |_, _, event: &TableEvent, cx| {
            if let TableEvent::Opened(_) = event {
                cx.emit(ScreenEvent::Navigate(Page::Events));
            }
        })
        .detach();
        Self {
            data: None,
            paused: None,
            live: true,
            range: TimeRange::default(),
            warnings,
        }
    }

    pub fn set(
        &mut self,
        cluster: &ClusterSummary,
        detail: &ClusterDetail,
        cx: &mut Context<Self>,
    ) {
        let data = OverviewData::new(cluster, detail);
        let recent = detail.recent_warnings(RECENT_WARNINGS);
        if self.live || self.data.is_none() {
            self.show(data, recent, cx);
        } else {
            self.paused = Some((data, recent));
        }
    }

    fn show(&mut self, data: OverviewData, recent: Vec<Event>, cx: &mut Context<Self>) {
        if self.data.as_ref() == Some(&data) {
            return;
        }
        self.data = Some(data);
        self.warnings
            .update(cx, |table, cx| table.set_rows(recent, cx));
        cx.notify();
    }

    fn toggle_live(&mut self, cx: &mut Context<Self>) {
        self.live = !self.live;
        if let Some((data, recent)) = self.paused.take().filter(|_| self.live) {
            self.show(data, recent, cx);
        }
        cx.notify();
    }

    pub fn header_controls(screen: &Entity<Self>, cx: &App) -> impl IntoElement {
        let this = screen.read(cx);
        let selected = TimeRange::ALL
            .iter()
            .position(|r| *r == this.range)
            .unwrap_or_default();
        let ranges_screen = screen.clone();
        let ranges = TimeRange::ALL
            .iter()
            .fold(Segmented::new("range").small().mono(), |seg, r| {
                seg.option(r.label())
            })
            .selected(selected)
            .on_select(move |ix, _, cx| {
                ranges_screen.update(cx, |screen, cx| screen.select_range(*ix, cx));
            });
        let live_screen = screen.clone();
        let live = Button::new("overview-live", LIVE)
            .small()
            .selected(this.live)
            .tooltip(LIVE_TOOLTIP)
            .on_click(move |_, _, cx| {
                live_screen.update(cx, OverviewScreen::toggle_live);
            });
        div()
            .flex()
            .items_center()
            .gap(SPACE_4)
            .child(ranges)
            .child(live)
    }

    fn select_range(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(range) = TimeRange::ALL.get(ix) {
            self.range = *range;
            cx.notify();
        }
    }

    fn capacity(data: &OverviewData, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let cpu = capacity_cell("CPU", &data.cpu, palette.acc, palette);
        let memory = capacity_cell("Memory", &data.memory, palette.ai, palette);
        let pods = data.pods;
        let pod_bar = StackedBar::new(vec![
            (Status::Ok, count_weight(pods.running)),
            (Status::Warn, count_weight(pods.pending)),
            (Status::Err, count_weight(pods.failing)),
        ]);
        let pods_cell = Cell {
            label: "Pods",
            detail: format!("capacity {}", thousands(data.nodes.pod_capacity)),
            figure: thousands(pods.total()),
            bar: pod_bar.into_any_element(),
            notes: vec![
                counted(pods.running, "running", palette.fg),
                counted(pods.pending, "pending", palette.warn),
                counted(pods.failing, "failing", palette.err),
            ],
        };
        let nodes_cell = Cell {
            label: "Nodes",
            detail: data.nodes.compact_instance_label(),
            figure: data.nodes.total.to_string(),
            bar: Self::node_strip(data, palette, cx),
            notes: vec![
                counted_usize(data.nodes.ready, "ready", palette.fg),
                counted_usize(data.nodes.cordoned, "cordoned", palette.warn),
            ],
        };
        let pods_cell = Self::cell_section("overview-pods", Page::Pods, pods_cell, palette, cx);
        let nodes_cell = Self::cell_section("overview-nodes", Page::Nodes, nodes_cell, palette, cx);
        let cells = [cpu, memory, pods_cell, nodes_cell];
        let last = cells.len() - 1;
        let row = cells.into_iter().enumerate().map(|(ix, cell)| {
            let column = div().flex().flex_col().flex_1().min_w_0().child(cell);
            if ix < last {
                column.border_r(RULE).border_color(palette.line)
            } else {
                column
            }
        });
        div()
            .flex()
            .border_b(RULE)
            .border_color(palette.line)
            .children(row)
            .into_any_element()
    }

    fn cell_section(
        id: &'static str,
        page: Page,
        cell: Cell,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let detail = detail_text(cell.detail, palette);
        ClickableSection::new(id)
            .title(SectionLabel::new(cell.label))
            .trailing(detail)
            .child(cell_figure(cell.figure))
            .child(cell.bar)
            .children(cell_notes(cell.notes, palette))
            .on_activate(Self::navigate(page, cx))
            .into_any_element()
    }

    fn navigate(page: Page, cx: &mut Context<Self>) -> impl Fn(&mut Window, &mut App) + 'static {
        let this = cx.entity().downgrade();
        move |_, cx| {
            this.update(cx, |_, cx| cx.emit(ScreenEvent::Navigate(page)))
                .ok();
        }
    }

    fn node_strip(data: &OverviewData, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let segments: Vec<AnyElement> = data
            .node_cells
            .iter()
            .enumerate()
            .map(|(ix, (status, label))| {
                let color = palette.status(*status);
                let label: SharedString = label.clone().into();
                div()
                    .id(ElementId::NamedInteger("node-cell".into(), ix as u64))
                    .flex_1()
                    .h(METER)
                    .rounded(RADIUS_SMALL)
                    .bg(color)
                    .cursor_pointer()
                    .tooltip(Tooltip::text(label))
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(ScreenEvent::Navigate(Page::Nodes));
                    }))
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .gap(SPACE_1)
            .children(segments)
            .into_any_element()
    }

    fn utilization(&self, data: &OverviewData, palette: &Palette) -> AnyElement {
        let chart = data
            .utilization
            .iter()
            .find(|u| u.range == self.range)
            .map(|u| {
                LineChart::new(CHART_MAX)
                    .series(Series::new("CPU", palette.acc, u.cpu.clone()).area(palette.accbg))
                    .series(Series::new("Memory", palette.ai, u.memory.clone()))
                    .threshold(u.cpu_requests, "CPU requests", palette.warn)
                    .y_labels(Y_LABELS.iter().map(|l| SharedString::from(*l)).collect())
                    .x_labels(
                        self.range
                            .axis()
                            .iter()
                            .map(|l| SharedString::from(*l))
                            .collect(),
                    )
                    .title(SectionLabel::new(UTILIZATION))
            });
        div().flex().flex_col().children(chart).into_any_element()
    }

    fn workloads(data: &OverviewData, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = data
            .workloads
            .iter()
            .map(|w| {
                let page = w.page;
                let id = format!("overview-{}", page.label().to_lowercase());
                let summary = div()
                    .font_family(FONT_MONO)
                    .text_size(TEXT_MONO)
                    .text_color(palette.mute)
                    .child(w.summary.clone());
                let bar = StackedBar::new(vec![
                    (Status::Ok, count_weight_usize(w.ok)),
                    (Status::Warn, count_weight_usize(w.mid)),
                    (Status::Err, count_weight_usize(w.bad)),
                ])
                .thin();
                ClickableSection::new(SharedString::from(id))
                    .title(div().text_size(TEXT_BODY).child(page.label()))
                    .trailing(summary)
                    .padding(SPACE_8, SPACE_4)
                    .gap(SPACE_3)
                    .child(bar)
                    .on_activate(Self::navigate(page, cx))
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(SPACE_8)
                    .pt(SPACE_7)
                    .pb(SPACE_4)
                    .child(SectionLabel::new("Workloads")),
            )
            .children(rows)
            .pb(SPACE_4)
            .into_any_element()
    }
}

impl Render for OverviewScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let Some(data) = self.data.clone() else {
            return div().into_any_element();
        };
        let middle = div()
            .flex()
            .border_b(RULE)
            .border_color(palette.line)
            .child(
                div()
                    .flex_grow(CHART_WEIGHT)
                    .flex_basis(Pixels::ZERO)
                    .min_w_0()
                    .px(SPACE_8)
                    .py(SPACE_7)
                    .border_r(RULE)
                    .border_color(palette.line)
                    .child(self.utilization(&data, &palette)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Self::workloads(&data, &palette, cx)),
            );
        let all_events =
            Link::new("all-events", ALL_EVENTS).on_click(cx.listener(|_, _, _, cx| {
                cx.emit(ScreenEvent::Navigate(Page::Events));
            }));
        div()
            .flex()
            .flex_col()
            .tab_group()
            .pb(SPACE_8)
            .child(MetaLine::new(data.meta.clone()))
            .child(Self::capacity(&data, &palette, cx))
            .child(middle)
            .child(
                SectionHeader::new("Warnings")
                    .annotation(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(TEXT_SMALL)
                            .text_color(warnings_color(data.warnings, &palette))
                            .child(data.warnings.to_string()),
                    )
                    .end(div().text_size(TEXT_CONTROL).child(all_events)),
            )
            .child(
                div()
                    .border_t(RULE)
                    .border_color(palette.line)
                    .child(self.warnings.clone()),
            )
            .into_any_element()
    }
}

fn warning_columns() -> Vec<Column<Event>> {
    vec![
        Column::new("Reason", |e: &Event, _| {
            StatusText::new(e.status, e.reason.clone()).into_any_element()
        })
        .fixed(REASON_WIDTH),
        Column::text("Object", |e: &Event| e.object.clone().into())
            .mono()
            .flex(OBJECT_WEIGHT, OBJECT_MIN),
        Column::colored_text(
            "Namespace",
            |e: &Event| e.namespace.clone().unwrap_or_else(|| "—".into()).into(),
            |_, palette| palette.fg2,
        )
        .fixed(NAMESPACE_WIDTH),
        Column::colored_text(
            "Message",
            |e: &Event| e.message.clone().into(),
            |_, palette| palette.fg2,
        )
        .flex(MESSAGE_WEIGHT, MESSAGE_MIN),
        Column::colored_text(
            "Age",
            |e: &Event| age(e.age).into(),
            |_, palette| palette.mute,
        )
        .mono()
        .fixed(AGE_WIDTH),
        Column::colored_text(
            "×",
            |e: &Event| e.count.to_string().into(),
            |_, palette| palette.mute,
        )
        .mono()
        .end()
        .fixed(COUNT_WIDTH),
    ]
}

fn warnings_color(warnings: usize, palette: &Palette) -> Hsla {
    if warnings > 0 {
        palette.warn
    } else {
        palette.mute
    }
}

fn count_weight(count: u32) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

fn count_weight_usize(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

fn counted(value: u32, label: &'static str, color: Hsla) -> AnyElement {
    div()
        .flex()
        .gap(SPACE_3)
        .child(
            div()
                .text_color(color)
                .font_weight(FontWeight::MEDIUM)
                .child(thousands(value)),
        )
        .child(label)
        .into_any_element()
}

fn counted_usize(value: usize, label: &'static str, color: Hsla) -> AnyElement {
    let value = u32::try_from(value).unwrap_or(u32::MAX);
    counted(value, label, color)
}

struct Cell {
    label: &'static str,
    detail: String,
    figure: String,
    bar: AnyElement,
    notes: Vec<AnyElement>,
}

fn capacity_cell(
    label: &'static str,
    capacity: &Capacity,
    color: Hsla,
    palette: &Palette,
) -> AnyElement {
    let id = format!("overview-capacity-{label}");
    let meter = CapacityMeter::new(id, capacity.clone(), color);
    let cell = Cell {
        label,
        detail: capacity.amount(),
        figure: percent(capacity.fraction()),
        bar: meter.into_any_element(),
        notes: Vec::new(),
    };
    static_cell(cell, palette)
}

fn static_cell(cell: Cell, palette: &Palette) -> AnyElement {
    let header = div()
        .flex()
        .items_center()
        .gap(SPACE_5)
        .child(SectionLabel::new(cell.label))
        .child(div().flex_1())
        .child(detail_text(cell.detail, palette));
    div()
        .flex()
        .flex_col()
        .gap(SPACE_5)
        .px(SPACE_8)
        .py(SPACE_7)
        .child(header)
        .child(cell_figure(cell.figure))
        .child(cell.bar)
        .children(cell_notes(cell.notes, palette))
        .into_any_element()
}

fn detail_text(detail: String, palette: &Palette) -> AnyElement {
    div()
        .font_family(FONT_MONO)
        .text_size(TEXT_MONO)
        .text_color(palette.mute)
        .truncate()
        .child(detail)
        .into_any_element()
}

fn cell_figure(figure: String) -> AnyElement {
    div()
        .text_size(TEXT_FIGURE)
        .font_weight(FontWeight::SEMIBOLD)
        .child(figure)
        .into_any_element()
}

fn cell_notes(notes: Vec<AnyElement>, palette: &Palette) -> Option<AnyElement> {
    if notes.is_empty() {
        return None;
    }
    let row = div()
        .flex()
        .gap(SPACE_7)
        .text_size(TEXT_CONTROL)
        .text_color(palette.mute)
        .children(notes);
    Some(row.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flock_core::ClusterId;
    use std::collections::BTreeMap;

    fn summary() -> ClusterSummary {
        ClusterSummary {
            id: ClusterId::new("prod-us-east"),
            name: "prod-us-east".into(),
            status: Status::Err,
            state: "Degraded".into(),
            distro: "EKS".into(),
            region: "us-east-1".into(),
            version: Some("v1.31.2".into()),
            nodes: Some(14),
            problems: flock_core::Counts::default(),
            nav: BTreeMap::new(),
            metrics: None,
        }
    }

    #[test]
    fn meta_line_reads_like_the_design() {
        let mut detail = ClusterDetail::default();
        detail.overview.metrics_source = "monitoring/prometheus-k8s".into();
        let data = OverviewData::new(&summary(), &detail);
        assert_eq!(
            data.meta,
            "EKS · v1.31.2 · us-east-1 · 0 namespaces · metrics monitoring/prometheus-k8s"
        );
    }
}
