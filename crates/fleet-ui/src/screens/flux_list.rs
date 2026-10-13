use fleet_core::filter::StatusFilter;
use fleet_core::{ClusterId, Command, Counts, FluxObject, Page};
use gpui::{
    AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels, Render, Styled,
    Window, div, px,
};

use super::ScreenEvent;
use crate::kit::{Column, FilterBar, FilterEvent, Lede, Table, TableEvent, chip_label};

const NAME_WEIGHT: f32 = 1.;
const NAME_MIN: Pixels = px(220.);
const NAMESPACE_WIDTH: Pixels = px(130.);
const KIND_WIDTH: Pixels = px(130.);
const STATUS_WEIGHT: f32 = 2.;
const STATUS_MIN: Pixels = px(300.);

pub struct FluxListScreen {
    objects: Vec<FluxObject>,
    table: Entity<Table<FluxObject>>,
    filter: Entity<FilterBar>,
}

impl EventEmitter<ScreenEvent> for FluxListScreen {}

impl FluxListScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            Table::new("flux-list", columns(), cx)
                .row_key(|o: &FluxObject| format!("{}/{}", o.key.namespace, o.key.name).into())
                .type_ahead(|o: &FluxObject| o.key.name.clone().into())
                .empty("No Flux objects match the filter.")
        });
        let filter = cx.new(|cx| FilterBar::new("flux-filter", window, cx));
        cx.subscribe(&filter, |this, _, FilterEvent::Changed, cx| {
            this.refresh(cx);
        })
        .detach();
        cx.subscribe(&table, |_, table, event: &TableEvent, cx| {
            if let TableEvent::Opened(ix) = *event
                && let Some(object) = table.read(cx).row(ix)
            {
                let open = Command::OpenFluxObject(object.key.clone());
                cx.emit(ScreenEvent::Command(open));
            }
        })
        .detach();
        Self {
            objects: Vec::new(),
            table,
            filter,
        }
    }

    pub fn list_focus(&self, cx: &gpui::App) -> gpui::FocusHandle {
        gpui::Focusable::focus_handle(&self.table, cx)
    }

    pub fn filter_bar(&self) -> &Entity<FilterBar> {
        &self.filter
    }

    pub fn set(
        &mut self,
        cluster: &ClusterId,
        page: Page,
        flux: &[FluxObject],
        cx: &mut Context<Self>,
    ) {
        let scope = format!("{}/{}", cluster.as_str(), page.label());
        self.table
            .update(cx, |table, cx| table.set_scope(scope, cx));
        let objects: Vec<FluxObject> = flux
            .iter()
            .filter(|o| &o.key.cluster == cluster && o.key.kind.page() == page)
            .cloned()
            .collect();
        if self.objects == objects {
            return;
        }
        self.objects = objects;
        self.refresh(cx);
    }

    pub fn header_note(&self) -> (Counts, String) {
        let counts = Counts::of(self.objects.iter().map(FluxObject::status));
        (counts, format!("{} objects", self.objects.len()))
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (query, chip) = {
            let bar = self.filter.read(cx);
            (bar.query().clone(), bar.chip())
        };
        let base: Vec<&FluxObject> = self
            .objects
            .iter()
            .filter(|o| {
                let fields = [o.key.name.as_str(), o.key.kind.label()];
                query.matches(Some(&o.key.namespace), &fields)
            })
            .collect();
        let chips = StatusFilter::counts(base.iter().map(|o| o.status()))
            .into_iter()
            .map(|(filter, n)| chip_label(filter.label(), n))
            .collect();
        let chip = StatusFilter::at(chip);
        let rows: Vec<FluxObject> = base
            .into_iter()
            .filter(|o| chip.keeps(o.status()))
            .cloned()
            .collect();
        let shown = rows.len();
        let total = self.objects.len();
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
}

impl Render for FluxListScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let counts = Counts::of(self.objects.iter().map(FluxObject::status));
        let sentence = match counts.flagged() {
            0 => format!("All {} objects are healthy.", self.objects.len()),
            flagged => format!(
                "{flagged} of {} objects need attention.",
                self.objects.len()
            ),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(Lede::new(sentence))
            .child(self.filter.clone())
            .child(div().flex_1().min_h_0().child(self.table.clone()))
    }
}

fn columns() -> Vec<Column<FluxObject>> {
    vec![
        Column::status_text("Name", FluxObject::status, |o: &FluxObject| {
            o.key.name.clone().into()
        })
        .mono()
        .highlight()
        .flex(NAME_WEIGHT, NAME_MIN)
        .sort_by_key(|o: &FluxObject| o.key.name.clone()),
        Column::text("Namespace", |o: &FluxObject| o.key.namespace.clone().into())
            .mono()
            .fixed(NAMESPACE_WIDTH)
            .sort_by_key(|o: &FluxObject| o.key.namespace.clone()),
        Column::text("Kind", |o: &FluxObject| o.key.kind.label().into())
            .fixed(KIND_WIDTH)
            .sort_by_key(|o: &FluxObject| o.key.kind.label()),
        Column::text("Status", |o: &FluxObject| o.sentence.plain().into())
            .flex(STATUS_WEIGHT, STATUS_MIN),
    ]
}
