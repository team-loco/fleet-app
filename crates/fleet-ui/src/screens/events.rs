use fleet_core::events::{Event, EventFilter, filter_events};
use fleet_core::time::age;
use fleet_core::{ClusterDetail, Status};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels,
    Render, Styled, Window, div, px,
};

use super::ScreenEvent;
use crate::kit::{
    Column, InputEvent, SectionHeader, Segmented, StatusText, Table, TextField, TextFieldState,
};
use crate::theme::{GLYPH_SLOT, SPACE_3, SPACE_4};

const REASON_WIDTH: Pixels = px(190.);
const OBJECT_WEIGHT: f32 = 1.2;
const OBJECT_MIN: Pixels = px(230.);
const NAMESPACE_WIDTH: Pixels = px(110.);
const MESSAGE_WEIGHT: f32 = 2.;
const MESSAGE_MIN: Pixels = px(260.);
const COUNT_WIDTH: Pixels = px(40.);
const AGE_WIDTH: Pixels = px(44.);
const NO_NAMESPACE: &str = "—";
const PLACEHOLDER: &str = "filter events";

pub struct EventsScreen {
    events: Vec<Event>,
    filter: EventFilter,
    query: String,
    search: Entity<TextFieldState>,
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
        let search = cx.new(|cx| TextFieldState::new(window, cx).placeholder(PLACEHOLDER));
        cx.subscribe(&search, |this, state, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                this.query = state.read(cx).value().to_string();
                this.refresh(cx);
            }
        })
        .detach();
        Self {
            events: Vec::new(),
            filter: EventFilter::default(),
            query: String::new(),
            search,
            table,
        }
    }

    pub fn set(&mut self, detail: &ClusterDetail, cx: &mut Context<Self>) {
        if self.events == detail.events {
            return;
        }
        self.events.clone_from(&detail.events);
        self.refresh(cx);
    }

    pub fn header_note(&self) -> (Status, String) {
        let warnings = self.events.iter().filter(|e| e.is_warning()).count();
        let status = if warnings > 0 {
            Status::Warn
        } else {
            Status::Ok
        };
        (status, format!("{warnings} warnings"))
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let rows = filter_events(&self.events, self.filter, &self.query);
        self.table.update(cx, |table, cx| table.set_rows(rows, cx));
        cx.notify();
    }

    fn select_filter(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(filter) = EventFilter::ALL.get(ix) {
            self.filter = *filter;
            self.refresh(cx);
        }
    }
}

impl Render for EventsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let warnings = self.events.iter().filter(|e| e.is_warning()).count();
        let count = format!("{warnings} warnings · {} total", self.events.len());
        let selected = EventFilter::ALL
            .iter()
            .position(|f| *f == self.filter)
            .unwrap_or_default();
        let filters = EventFilter::ALL
            .iter()
            .fold(Segmented::new("event-filter").small(), |seg, f| {
                seg.option(f.label())
            })
            .selected(selected)
            .on_select(cx.listener(|this, ix: &usize, _, cx| this.select_filter(*ix, cx)));
        let controls = div()
            .flex()
            .items_center()
            .gap(SPACE_4)
            .child(TextField::search(&self.search))
            .child(filters);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(SectionHeader::new("Events").count(count).end(controls))
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
        .child(div().w(GLYPH_SLOT).flex_none())
        .child(div().truncate().child(event.reason.clone()))
        .into_any_element()
}

fn columns() -> Vec<Column<Event>> {
    vec![
        Column::new("Reason", |e: &Event, _| reason_cell(e))
            .fixed(REASON_WIDTH)
            .sort_by_key(|e: &Event| e.reason.clone()),
        Column::text("Object", |e: &Event| e.object.clone().into())
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
        .flex(MESSAGE_WEIGHT, MESSAGE_MIN),
        Column::colored_text("×", |e: &Event| e.count.to_string().into(), |_, p| p.mute)
            .mono()
            .end()
            .fixed(COUNT_WIDTH)
            .sort_by_key(|e: &Event| e.count),
        Column::colored_text("Age", |e: &Event| age(e.age).into(), |_, p| p.mute)
            .mono()
            .fixed(AGE_WIDTH)
            .sort_by_key(|e: &Event| e.age),
    ]
}
