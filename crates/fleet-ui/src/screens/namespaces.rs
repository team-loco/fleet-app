use fleet_core::cluster::Namespace;
use fleet_core::overview::number;
use fleet_core::time::age;
use fleet_core::{ClusterDetail, Status};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels,
    Render, Styled, Window, div, px,
};

use super::ScreenEvent;
use crate::kit::{Column, SectionHeader, StatusGlyph, Table};
use crate::theme::{ActiveTheme, FONT_MONO, SPACE_4};

const NAME_WEIGHT: f32 = 1.4;
const NAME_MIN: Pixels = px(200.);
const STATUS_WIDTH: Pixels = px(72.);
const PODS_WIDTH: Pixels = px(52.);
const PROBLEMS_WIDTH: Pixels = px(96.);
const CPU_WIDTH: Pixels = px(64.);
const MEMORY_WIDTH: Pixels = px(84.);
const AGE_WIDTH: Pixels = px(44.);
const MEMORY_UNIT: &str = "GiB";

pub struct NamespacesScreen {
    namespaces: Vec<Namespace>,
    table: Entity<Table<Namespace>>,
}

impl EventEmitter<ScreenEvent> for NamespacesScreen {}

impl NamespacesScreen {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            Table::new("namespaces", columns(), cx)
                .row_key(|n: &Namespace| n.name.clone().into())
                .empty("No namespaces.")
        });
        Self {
            namespaces: Vec::new(),
            table,
        }
    }

    pub fn set(&mut self, detail: &ClusterDetail, cx: &mut Context<Self>) {
        if self.namespaces == detail.namespaces {
            return;
        }
        self.namespaces.clone_from(&detail.namespaces);
        let rows = self.namespaces.clone();
        self.table.update(cx, |table, cx| table.set_rows(rows, cx));
        cx.notify();
    }

    pub fn header_note(&self) -> (Status, String) {
        let flagged = self
            .namespaces
            .iter()
            .filter(|n| n.status != Status::Ok)
            .count();
        let worst = Status::worst(self.namespaces.iter().map(|n| n.status));
        (worst, format!("{flagged} with problems"))
    }
}

impl Render for NamespacesScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let pods: u32 = self.namespaces.iter().map(|n| n.pods).sum();
        let count = format!("{} · {pods} pods", self.namespaces.len());
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(SectionHeader::new("Namespaces").count(count))
            .child(div().flex_1().min_h_0().child(self.table.clone()))
    }
}

fn problems(namespace: &Namespace) -> AnyElement {
    let badges: Vec<AnyElement> = namespace
        .problems
        .badges()
        .into_iter()
        .map(|(status, n)| ProblemBadge { status, n }.into_any_element())
        .collect();
    div()
        .flex()
        .gap(SPACE_4)
        .children(badges)
        .into_any_element()
}

#[derive(gpui::IntoElement)]
struct ProblemBadge {
    status: Status,
    n: u32,
}

impl gpui::RenderOnce for ProblemBadge {
    fn render(self, _: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        div()
            .font_family(FONT_MONO)
            .text_color(cx.palette().status(self.status))
            .child(format!("{} {}", self.status.glyph(), self.n))
    }
}

fn columns() -> Vec<Column<Namespace>> {
    vec![
        Column::new("Name", |n: &Namespace, _| {
            div()
                .flex()
                .items_center()
                .gap(SPACE_4)
                .min_w_0()
                .child(StatusGlyph::new(n.status))
                .child(div().truncate().child(n.name.clone()))
                .into_any_element()
        })
        .mono()
        .flex(NAME_WEIGHT, NAME_MIN)
        .sort_by_key(|n: &Namespace| n.name.clone()),
        Column::colored_text(
            "Status",
            |n: &Namespace| n.phase.clone().into(),
            |_, p| p.fg2,
        )
        .fixed(STATUS_WIDTH),
        Column::text("Pods", |n: &Namespace| n.pods.to_string().into())
            .mono()
            .end()
            .fixed(PODS_WIDTH)
            .sort_by_key(|n: &Namespace| n.pods),
        Column::new("Problems", |n: &Namespace, _| problems(n))
            .fixed(PROBLEMS_WIDTH)
            .sort_by_key(|n: &Namespace| std::cmp::Reverse(n.status)),
        Column::colored_text("CPU", |n: &Namespace| number(n.cpu).into(), |_, p| p.fg2)
            .mono()
            .end()
            .fixed(CPU_WIDTH)
            .sortable(|a: &Namespace, b: &Namespace| a.cpu.total_cmp(&b.cpu)),
        Column::colored_text(
            "Memory",
            |n: &Namespace| format!("{} {MEMORY_UNIT}", number(n.memory)).into(),
            |_, p| p.fg2,
        )
        .mono()
        .end()
        .fixed(MEMORY_WIDTH)
        .sortable(|a: &Namespace, b: &Namespace| a.memory.total_cmp(&b.memory)),
        Column::colored_text("Age", |n: &Namespace| age(n.age).into(), |_, p| p.mute)
            .mono()
            .fixed(AGE_WIDTH)
            .sort_by_key(|n: &Namespace| n.age),
    ]
}
