use fleet_core::events::Event;
use fleet_core::nodes::NodeSummary;
use fleet_core::overview::{
    Capacity, PodPhases, TimeRange, Utilization, WorkloadHealth, percent, thousands,
};
use fleet_core::time::age;
use fleet_core::{ClusterDetail, ClusterSummary, Page, Status};
use gpui::{
    AnyElement, AppContext, Context, ElementId, Entity, EventEmitter, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px, relative,
};

use super::ScreenEvent;
use crate::kit::{
    Column, LineChart, Link, Meter, SectionHeader, SectionLabel, Segmented, Series, StackedBar,
    StatusText, Table, TableEvent, Tooltip,
};
use crate::theme::{
    ActiveTheme, FONT_MONO, LINE_PROSE, METER, Palette, RADIUS_SMALL, RULE, SPACE_1, SPACE_3,
    SPACE_5, SPACE_6, SPACE_7, SPACE_8, TEXT_BODY, TEXT_CONTROL, TEXT_FIGURE, TEXT_MONO, TEXT_UI,
};

const RECENT_WARNINGS: usize = 5;
const CHART_MAX: f32 = 100.;
const Y_LABELS: [&str; 5] = ["100%", "75%", "50%", "25%", "0"];
const SEPARATOR: &str = " · ";
const ALL_EVENTS: &str = "All events →";
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
    sentence: String,
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
            sentence: detail.sentence(),
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
        if self.data.as_ref() == Some(&data) {
            return;
        }
        self.data = Some(data);
        let recent = detail.recent_warnings(RECENT_WARNINGS);
        self.warnings
            .update(cx, |table, cx| table.set_rows(recent, cx));
        cx.notify();
    }

    fn select_range(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(range) = TimeRange::ALL.get(ix) {
            self.range = *range;
            cx.notify();
        }
    }

    fn lede(data: &OverviewData, palette: &Palette) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(SPACE_3)
            .px(SPACE_8)
            .pt(SPACE_7)
            .pb(SPACE_7)
            .border_b(RULE)
            .border_color(palette.line)
            .child(
                div()
                    .text_size(TEXT_UI)
                    .line_height(relative(LINE_PROSE))
                    .text_color(palette.fg2)
                    .child(data.sentence.clone()),
            )
            .child(
                div()
                    .font_family(FONT_MONO)
                    .text_size(TEXT_MONO)
                    .text_color(palette.mute)
                    .child(data.meta.clone()),
            )
    }

    fn capacity(data: &OverviewData, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let cpu = capacity_cell(
            "CPU",
            &data.cpu,
            palette.acc,
            palette,
            vec![
                note("Requests", percent(data.cpu.requests), palette.fg),
                note(
                    "Limits",
                    percent(data.cpu.limits),
                    limit_color(data.cpu.limits, palette),
                ),
            ],
        );
        let memory = capacity_cell(
            "Memory",
            &data.memory,
            palette.fg2,
            palette,
            vec![
                note("Requests", percent(data.memory.requests), palette.fg),
                note(
                    "Limits",
                    percent(data.memory.limits),
                    limit_color(data.memory.limits, palette),
                ),
            ],
        );
        let pods = data.pods;
        let pod_bar = StackedBar::new(vec![
            (Status::Ok, count_weight(pods.running)),
            (Status::Warn, count_weight(pods.pending)),
            (Status::Err, count_weight(pods.failing)),
        ]);
        let pods_cell = cell(
            "Pods",
            format!("capacity {}", thousands(data.nodes.pod_capacity)),
            thousands(pods.total()),
            pod_bar.into_any_element(),
            vec![
                counted(pods.running, "running", palette.fg),
                counted(pods.pending, "pending", palette.warn),
                counted(pods.failing, "failing", palette.err),
            ],
            palette,
        );
        let nodes_cell = cell(
            "Nodes",
            data.nodes.instance_label(),
            data.nodes.total.to_string(),
            Self::node_strip(data, palette, cx),
            vec![
                counted_usize(data.nodes.ready, "ready", palette.fg),
                counted_usize(data.nodes.cordoned, "cordoned", palette.warn),
            ],
            palette,
        );
        let cells = [cpu, memory, pods_cell, nodes_cell];
        let last = cells.len() - 1;
        let row = cells.into_iter().enumerate().map(|(ix, cell)| {
            let column = div().flex_1().min_w_0().px(SPACE_8).py(SPACE_7).child(cell);
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

    fn utilization(
        &self,
        data: &OverviewData,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = TimeRange::ALL
            .iter()
            .position(|r| *r == self.range)
            .unwrap_or_default();
        let ranges = TimeRange::ALL
            .iter()
            .fold(Segmented::new("range").small().mono(), |seg, r| {
                seg.option(r.label())
            })
            .selected(selected)
            .on_select(cx.listener(|this, ix: &usize, _, cx| this.select_range(*ix, cx)));
        let chart = data
            .utilization
            .iter()
            .find(|u| u.range == self.range)
            .map(|u| {
                LineChart::new(CHART_MAX)
                    .series(Series::new("CPU", palette.acc, u.cpu.clone()).area(palette.accbg))
                    .series(Series::new("Memory", palette.fg2, u.memory.clone()))
                    .threshold(u.cpu_requests, "CPU requests", palette.warn)
                    .y_labels(Y_LABELS.iter().map(|l| SharedString::from(*l)).collect())
                    .x_labels(
                        self.range
                            .axis()
                            .iter()
                            .map(|l| SharedString::from(*l))
                            .collect(),
                    )
            });
        div()
            .flex()
            .flex_col()
            .gap(SPACE_5)
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(SectionLabel::new("Utilization"))
                    .child(div().flex_1())
                    .child(ranges),
            )
            .children(chart)
            .into_any_element()
    }

    fn workloads(data: &OverviewData, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = data
            .workloads
            .iter()
            .map(|w| {
                let page = w.page;
                let hover = palette.hover;
                div()
                    .id(ElementId::NamedInteger("workload".into(), page as u64))
                    .flex()
                    .flex_col()
                    .gap(SPACE_3)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ScreenEvent::Navigate(page));
                    }))
                    .child(
                        div()
                            .flex()
                            .text_size(TEXT_BODY)
                            .child(div().flex_1().child(page.label()))
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(TEXT_MONO)
                                    .text_color(palette.mute)
                                    .child(w.summary.clone()),
                            ),
                    )
                    .child(
                        StackedBar::new(vec![
                            (Status::Ok, count_weight_usize(w.ok)),
                            (Status::Warn, count_weight_usize(w.mid)),
                            (Status::Err, count_weight_usize(w.bad)),
                        ])
                        .thin(),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(SPACE_6)
            .child(SectionLabel::new("Workloads"))
            .children(rows)
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
                    .child(self.utilization(&data, &palette, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(SPACE_8)
                    .py(SPACE_7)
                    .child(Self::workloads(&data, &palette, cx)),
            );
        let all_events =
            Link::new("all-events", ALL_EVENTS).on_click(cx.listener(|_, _, _, cx| {
                cx.emit(ScreenEvent::Navigate(Page::Events));
            }));
        div()
            .flex()
            .flex_col()
            .pb(SPACE_8)
            .child(Self::lede(&data, &palette))
            .child(Self::capacity(&data, &palette, cx))
            .child(middle)
            .child(
                SectionHeader::new("Warnings")
                    .count(data.warnings.to_string())
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

fn limit_color(limit: f32, palette: &Palette) -> Hsla {
    if limit > 1. { palette.warn } else { palette.fg }
}

fn count_weight(count: u32) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

fn count_weight_usize(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

fn note(label: &'static str, value: String, color: Hsla) -> AnyElement {
    div()
        .flex()
        .gap(SPACE_3)
        .child(label)
        .child(
            div()
                .text_color(color)
                .font_weight(FontWeight::MEDIUM)
                .child(value),
        )
        .into_any_element()
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

fn capacity_cell(
    label: &'static str,
    capacity: &Capacity,
    color: Hsla,
    palette: &Palette,
    notes: Vec<AnyElement>,
) -> AnyElement {
    let meter = Meter::new(capacity.fraction())
        .request(capacity.requests)
        .color(color);
    cell(
        label,
        capacity.amount(),
        percent(capacity.fraction()),
        meter.into_any_element(),
        notes,
        palette,
    )
}

fn cell(
    label: &'static str,
    detail: String,
    figure: String,
    bar: AnyElement,
    notes: Vec<AnyElement>,
    palette: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(SPACE_5)
        .child(
            div()
                .flex()
                .items_center()
                .gap(SPACE_5)
                .child(SectionLabel::new(label))
                .child(div().flex_1())
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(TEXT_MONO)
                        .text_color(palette.mute)
                        .truncate()
                        .child(detail),
                ),
        )
        .child(
            div()
                .text_size(TEXT_FIGURE)
                .font_weight(FontWeight::SEMIBOLD)
                .child(figure),
        )
        .child(bar)
        .child(
            div()
                .flex()
                .gap(SPACE_7)
                .text_size(TEXT_CONTROL)
                .text_color(palette.mute)
                .children(notes),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_core::ClusterId;
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
            problems: fleet_core::Counts::default(),
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

    #[test]
    fn limits_over_capacity_turn_warn() {
        let palette = Palette::dark();
        assert_eq!(limit_color(1.31, &palette), palette.warn);
        assert_eq!(limit_color(0.92, &palette), palette.fg);
    }
}
