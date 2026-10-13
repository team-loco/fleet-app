use fleet_core::cluster::{CPU_UNIT, MEMORY_UNIT, Namespace, namespace_sentence};
use fleet_core::filter::StatusFilter;
use fleet_core::overview::fixed;
use fleet_core::time::age;
use fleet_core::{ClusterDetail, ClusterId, Counts, Status};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels,
    Render, Styled, Window, div, px,
};

use super::ScreenEvent;
use crate::kit::status_symbols;
use crate::kit::{Column, FilterBar, FilterEvent, Lede, StatusGlyph, Table, chip_label};
use crate::theme::{ActiveTheme, FONT_MONO, GLYPH_SLOT, Palette, SPACE_4};

const NAME_WEIGHT: f32 = 1.4;
const NAME_MIN: Pixels = px(200.);
const PROBLEMS_WEIGHT: f32 = 1.;
const PROBLEMS_MIN: Pixels = px(96.);
const PODS_WIDTH: Pixels = px(52.);
const CPU_WIDTH: Pixels = px(84.);
const MEMORY_WIDTH: Pixels = px(84.);
const AGE_WIDTH: Pixels = px(44.);

pub struct NamespacesScreen {
    namespaces: Vec<Namespace>,
    sentence: String,
    table: Entity<Table<Namespace>>,
    filter: Entity<FilterBar>,
}

impl EventEmitter<ScreenEvent> for NamespacesScreen {}

impl NamespacesScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            Table::new("namespaces", columns(), cx)
                .row_key(|n: &Namespace| n.name.clone().into())
                .empty("No namespaces.")
        });
        let filter = cx.new(|cx| FilterBar::new("namespace-filter", window, cx));
        cx.subscribe(&filter, |this, _, FilterEvent::Changed, cx| {
            this.refresh(cx);
        })
        .detach();
        Self {
            namespaces: Vec::new(),
            sentence: String::new(),
            table,
            filter,
        }
    }

    pub fn filter_bar(&self) -> &Entity<FilterBar> {
        &self.filter
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (query, chip) = {
            let bar = self.filter.read(cx);
            (bar.query().clone(), bar.chip())
        };
        let base: Vec<&Namespace> = self
            .namespaces
            .iter()
            .filter(|n| n.matches(&query))
            .collect();
        let chips = StatusFilter::counts(base.iter().map(|n| n.status))
            .into_iter()
            .map(|(filter, n)| chip_label(filter.label(), n))
            .collect();
        let chip = StatusFilter::at(chip);
        let rows: Vec<Namespace> = base
            .into_iter()
            .filter(|n| chip.keeps(n.status))
            .cloned()
            .collect();
        let shown = rows.len();
        let total = self.namespaces.len();
        self.filter.update(cx, |bar, cx| {
            bar.set_chips(chips, cx);
            bar.set_matches(shown, total, cx);
        });
        let words = query.words;
        self.table.update(cx, |table, cx| {
            table.set_highlight(words, cx);
            table.set_rows(rows, cx);
        });
        cx.notify();
    }

    pub fn set(&mut self, cluster: &ClusterId, detail: &ClusterDetail, cx: &mut Context<Self>) {
        let scope = cluster.as_str().to_owned();
        self.table
            .update(cx, |table, cx| table.set_scope(scope, cx));
        if self.namespaces == detail.namespaces {
            return;
        }
        self.namespaces.clone_from(&detail.namespaces);
        self.sentence = namespace_sentence(&self.namespaces);
        self.refresh(cx);
    }

    pub fn header_note(&self) -> (Counts, String) {
        let counts = Counts::of(self.namespaces.iter().map(|n| n.status));
        let text = format!("{} with problems", counts.flagged());
        (counts, text)
    }
}

impl Render for NamespacesScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let pods: u32 = self.namespaces.iter().map(|n| n.pods).sum();
        let meta = format!("{} namespaces · {pods} pods", self.namespaces.len());
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(Lede::new(self.sentence.clone()).meta(meta))
            .child(self.filter.clone())
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
            .child(status_symbols(format!(
                "{} {}",
                self.status.glyph(),
                self.n
            )))
    }
}

fn name_cell(namespace: &Namespace, palette: &Palette) -> AnyElement {
    let phase = namespace.phase_note().map(|phase| {
        div()
            .flex_none()
            .text_color(palette.warn)
            .child(phase.to_owned())
    });
    div()
        .flex()
        .items_center()
        .gap(SPACE_4)
        .min_w_0()
        .child(StatusGlyph::new(namespace.status))
        .child(div().truncate().child(namespace.name.clone()))
        .children(phase)
        .into_any_element()
}

fn columns() -> Vec<Column<Namespace>> {
    vec![
        Column::new("Name", name_cell)
            .fit_by(|n: &Namespace| n.name.clone().into(), GLYPH_SLOT + SPACE_4)
            .mono()
            .flex(NAME_WEIGHT, NAME_MIN)
            .sort_by_key(|n: &Namespace| n.name.clone()),
        Column::new("Problems", |n: &Namespace, _| problems(n))
            .flex(PROBLEMS_WEIGHT, PROBLEMS_MIN)
            .sort_by_key(|n: &Namespace| std::cmp::Reverse(n.status)),
        Column::text("Pods", |n: &Namespace| n.pods.to_string().into())
            .mono()
            .end()
            .fixed(PODS_WIDTH)
            .sort_by_key(|n: &Namespace| n.pods),
        Column::colored_text(
            "CPU",
            |n: &Namespace| format!("{} {CPU_UNIT}", fixed(n.cpu)).into(),
            |_, p| p.fg2,
        )
        .mono()
        .end()
        .fixed(CPU_WIDTH)
        .sortable(|a: &Namespace, b: &Namespace| a.cpu.total_cmp(&b.cpu)),
        Column::colored_text(
            "Memory",
            |n: &Namespace| format!("{} {MEMORY_UNIT}", fixed(n.memory)).into(),
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
