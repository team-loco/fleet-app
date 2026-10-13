use fleet_core::events::{self, Event, EventFilter, filter_counts, filter_events};
use fleet_core::time::age;
use fleet_core::{ClusterDetail, ClusterId, Counts, Status};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels,
    Render, Styled, Window, div, px,
};

use super::ScreenEvent;
use crate::kit::{
    Column, FilterBar, FilterEvent, Lede, StatusGlyph, StatusText, Table, chip_label,
};
use crate::theme::{GLYPH_SLOT, SPACE_3, SPACE_4};

const REASON_WIDTH: Pixels = px(190.);
const OBJECT_WEIGHT: f32 = 1.2;
const OBJECT_MIN: Pixels = px(290.);
const NAMESPACE_WIDTH: Pixels = px(110.);
const MESSAGE_WEIGHT: f32 = 2.;
const MESSAGE_MIN: Pixels = px(260.);
const COUNT_WIDTH: Pixels = px(40.);
const AGE_WIDTH: Pixels = px(44.);
const NO_NAMESPACE: &str = "—";

pub struct EventsScreen {
    events: Vec<Event>,
    sentence: String,
    filter: Entity<FilterBar>,
    table: Entity<Table<Event>>,
}

impl EventEmitter<ScreenEvent> for EventsScreen {}

impl EventsScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            Table::new("events", columns(), cx)
                .row_status(|e: &Event| (e.status == Status::Err).then_some(Status::Err))
                .empty("No events match the filter.")
        });
        let filter = cx.new(|cx| FilterBar::new("event-filter", window, cx));
        cx.subscribe(&filter, |this, _, FilterEvent::Changed, cx| {
            this.refresh(cx);
        })
        .detach();
        Self {
            events: Vec::new(),
            sentence: String::new(),
            filter,
            table,
        }
    }

    pub fn set(&mut self, cluster: &ClusterId, detail: &ClusterDetail, cx: &mut Context<Self>) {
        let scope = cluster.as_str().to_owned();
        self.table
            .update(cx, |table, cx| table.set_scope(scope, cx));
        if self.events == detail.events {
            return;
        }
        self.events.clone_from(&detail.events);
        self.sentence = events::sentence(&self.events);
        self.refresh(cx);
    }

    pub fn header_note(&self) -> (Counts, String) {
        let counts = events::counts(&self.events);
        (counts, format!("{} events", self.events.len()))
    }

    pub fn filter_bar(&self) -> &Entity<FilterBar> {
        &self.filter
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (query, chip) = {
            let bar = self.filter.read(cx);
            (bar.query().clone(), bar.chip())
        };
        let chips = filter_counts(&self.events, &query)
            .into_iter()
            .map(|(filter, n)| chip_label(filter.label(), n))
            .collect();
        let filter = EventFilter::ALL.get(chip).copied().unwrap_or_default();
        let rows = filter_events(&self.events, filter, &query);
        let shown = rows.len();
        let total = self.events.len();
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

impl Render for EventsScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(Lede::new(self.sentence.clone()))
            .child(self.filter.clone())
            .child(div().flex_1().min_h_0().child(self.table.clone()))
    }
}

pub fn reason_cell(event: &Event) -> AnyElement {
    if event.is_warning() {
        return StatusText::new(event.status, event.reason.clone()).into_any_element();
    }
    div()
        .flex()
        .items_center()
        .gap(SPACE_3)
        .min_w_0()
        .child(StatusGlyph::new(Status::Ok))
        .child(div().truncate().child(event.reason.clone()))
        .into_any_element()
}

fn columns() -> Vec<Column<Event>> {
    vec![
        Column::new("Reason", |e: &Event, _| reason_cell(e))
            .fit_by(|e: &Event| e.reason.clone().into(), GLYPH_SLOT + SPACE_4)
            .fixed(REASON_WIDTH)
            .sort_by_key(|e: &Event| e.reason.clone()),
        Column::text("Object", |e: &Event| e.object.clone().into())
            .highlight()
            .mono()
            .flex(OBJECT_WEIGHT, OBJECT_MIN)
            .sort_by_key(|e: &Event| e.object.clone()),
        Column::colored_text(
            "Namespace",
            |e: &Event| {
                e.namespace
                    .clone()
                    .unwrap_or_else(|| NO_NAMESPACE.into())
                    .into()
            },
            |_, p| p.fg2,
        )
        .fixed(NAMESPACE_WIDTH)
        .sort_by_key(|e: &Event| e.namespace.clone()),
        Column::colored_text(
            "Message",
            |e: &Event| e.message.clone().into(),
            |_, p| p.fg2,
        )
        .highlight()
        .flex(MESSAGE_WEIGHT, MESSAGE_MIN),
        Column::colored_text("Age", |e: &Event| age(e.age).into(), |_, p| p.mute)
            .mono()
            .fixed(AGE_WIDTH)
            .sort_by_key(|e: &Event| e.age),
        Column::colored_text("×", |e: &Event| e.count.to_string().into(), |_, p| p.mute)
            .mono()
            .end()
            .fixed(COUNT_WIDTH)
            .sort_by_key(|e: &Event| e.count),
    ]
}
