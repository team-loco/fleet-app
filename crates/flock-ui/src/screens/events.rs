use flock_core::events::{self, Event, EventFilter, filter_counts, filter_events};
use flock_core::resources::EVENT_LIST;
use flock_core::settings::NamespaceSelection;
use flock_core::time::age;
use flock_core::{ClusterDetail, ClusterId, ClusterSummary, Command, Counts, Preferences, Status};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div,
};

use super::ScreenEvent;
use crate::kit::{
    Column, FilterBar, FilterEvent, IconButton, Inspector, KeyValue, Link, Presence, Select,
    SelectEvent, StatusGlyph, StatusText, Table, TableEvent, chip_label, slide_in, slide_out,
};
use crate::theme::{
    EVENT_AGE_WIDTH, EVENT_COUNT_WIDTH, EVENT_MESSAGE_MIN, EVENT_MESSAGE_WEIGHT,
    EVENT_NAMESPACE_WIDTH, EVENT_OBJECT_MIN, EVENT_OBJECT_WEIGHT, EVENT_REASON_WIDTH, GLYPH_SLOT,
    INSPECTOR, SEARCH_WIDTH, SPACE_3, SPACE_4,
};

const INSPECTOR_ID: &str = "event-inspector";
const NO_NAMESPACE: &str = "—";
const ALL_NAMESPACES: &str = "All namespaces";

pub struct EventsScreen {
    cluster: Option<ClusterId>,
    preferences: Preferences,
    namespaces: Entity<Select>,
    selected: Option<String>,
    presence: Presence<Event>,
    events: Vec<Event>,
    filter: Entity<FilterBar>,
    table: Entity<Table<Event>>,
}

fn presence(screen: &mut EventsScreen) -> &mut Presence<Event> {
    &mut screen.presence
}

impl EventEmitter<ScreenEvent> for EventsScreen {}

impl EventsScreen {
    pub fn close_inspector(&mut self, cx: &mut Context<Self>) -> bool {
        if self.selected.is_none() {
            return false;
        }
        self.close(cx);
        true
    }

    pub fn list_focus(&self, cx: &gpui::App) -> gpui::FocusHandle {
        gpui::Focusable::focus_handle(&self.table, cx)
    }

    pub fn filter_bar(&self) -> &Entity<FilterBar> {
        &self.filter
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            Table::new("events", columns(), cx)
                .type_ahead(|event: &Event| event.object.clone().into())
                .row_key(|event: &Event| event.key().into())
                .click_opens()
                .row_status(|e: &Event| (e.status == Status::Err).then_some(Status::Err))
                .empty("No events match the filter.")
        });
        cx.subscribe(&table, |this, table, event: &TableEvent, cx| {
            if let TableEvent::Opened(ix) = event {
                this.selected = table.read(cx).row(*ix).map(Event::key);
                cx.notify();
            }
        })
        .detach();
        let namespaces = cx.new(|cx| {
            Select::new("events-namespace", vec![ALL_NAMESPACES.into()], cx)
                .mono()
                .width(SEARCH_WIDTH)
        });
        cx.subscribe(&namespaces, |this, select, _: &SelectEvent, cx| {
            let label = select.read(cx).selected_label().cloned();
            let Some(label) = label else {
                return;
            };
            let scope = if label.as_ref() == ALL_NAMESPACES {
                NamespaceSelection::All
            } else {
                NamespaceSelection::Named(label.to_string())
            };
            this.preferences.select_namespace(EVENT_LIST, scope);
            this.selected = None;
            this.refresh(cx);
            if let Some(cluster) = &this.cluster {
                cx.emit(ScreenEvent::Command(Command::SetPreferences(
                    cluster.clone(),
                    this.preferences.clone(),
                )));
            }
        })
        .detach();
        let filter =
            cx.new(|cx| FilterBar::new("event-filter", window, cx).scope(namespaces.clone()));
        cx.subscribe(&filter, |this, _, FilterEvent::Changed, cx| {
            this.refresh(cx);
        })
        .detach();
        Self {
            cluster: None,
            preferences: Preferences::default(),
            namespaces,
            selected: None,
            presence: Presence::default(),
            events: Vec::new(),
            filter,
            table,
        }
    }

    pub fn set(
        &mut self,
        cluster: &ClusterSummary,
        detail: &ClusterDetail,
        preferences: &Preferences,
        cx: &mut Context<Self>,
    ) {
        if self.cluster.as_ref() != Some(&cluster.id) {
            self.selected = None;
        }
        let scope = cluster.id.as_str().to_owned();
        self.table
            .update(cx, |table, cx| table.set_scope(scope, cx));
        self.cluster = Some(cluster.id.clone());
        self.preferences = preferences.clone();
        self.events.clone_from(&detail.events);
        let scope = self.preferences.namespace_selection(EVENT_LIST);
        let mut names = std::collections::BTreeSet::new();
        names.extend(detail.namespaces.iter().map(|ns| ns.name.clone()));
        names.extend(
            self.events
                .iter()
                .filter_map(|event| event.namespace.clone()),
        );
        if let Some(namespace) = scope.namespace() {
            names.insert(namespace.into());
        }
        let mut options: Vec<SharedString> = vec![ALL_NAMESPACES.into()];
        options.extend(names.into_iter().map(SharedString::from));
        let selected = scope
            .namespace()
            .and_then(|ns| options.iter().position(|option| option.as_ref() == ns))
            .unwrap_or_default();
        self.namespaces.update(cx, |select, cx| {
            select.set_options(options, cx);
            select.set_selected(selected, cx);
        });
        self.refresh(cx);
    }

    pub fn header_note(&self) -> (Counts, String) {
        let scope = self.preferences.namespace_selection(EVENT_LIST);
        let events = events::in_namespace(&self.events, scope.namespace());
        let counts = events::counts(&events);
        (counts, format!("{} events", events.len()))
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let scope = self.preferences.namespace_selection(EVENT_LIST);
        let scoped = events::in_namespace(&self.events, scope.namespace());
        let (query, chip) = {
            let bar = self.filter.read(cx);
            (bar.query().clone(), bar.chip())
        };
        let chips = filter_counts(&scoped, &query)
            .into_iter()
            .map(|(filter, count)| chip_label(filter.label(), count))
            .collect();
        let filter = EventFilter::ALL.get(chip).copied().unwrap_or_default();
        let rows = filter_events(&scoped, filter, &query);
        let shown = rows.len();
        let total = scoped.len();
        self.filter.update(cx, |bar, cx| {
            bar.set_chips(chips, cx);
            bar.set_matches(shown, total, cx);
        });
        if self
            .selected
            .as_ref()
            .is_some_and(|key| !rows.iter().any(|event| &event.key() == key))
        {
            self.selected = None;
        }
        let words = query.words;
        self.table.update(cx, |table, cx| {
            table.set_highlight(words, cx);
            table.set_rows(rows, cx);
        });
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.table.update(cx, |table, cx| table.select(None, cx));
        cx.notify();
    }

    fn inspector(&self, event: &Event, cx: &mut Context<Self>) -> AnyElement {
        let close = IconButton::glyph("close-event", "×", "Close")
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
        let details = &event.details;
        let first_seen = seen(details.first_age, details.first_seen.as_deref());
        let last_seen = seen(Some(event.age), details.last_seen.as_deref());
        let facts = KeyValue::new()
            .sans()
            .row("Reason", event.reason.clone())
            .row("Type", StatusText::new(event.status, event_type(event)))
            .row("Count", event.count.to_string())
            .row("First seen", first_seen)
            .row("Last seen", last_seen)
            .row("Component", value(&details.source_component))
            .row("Host", value(&details.source_host));
        let target = self
            .cluster
            .as_ref()
            .and_then(|cluster| event.target(cluster));
        let object = match target {
            Some(target) => {
                let weak = cx.weak_entity();
                Link::new("event-object", event.object.clone())
                    .mono()
                    .on_click(move |_, _, cx| {
                        let command = Command::OpenResource(target.clone());
                        let _ = weak.update(cx, |_, cx| cx.emit(ScreenEvent::Command(command)));
                    })
                    .into_any_element()
            }
            None => div().child(value(&event.object)).into_any_element(),
        };
        let related = self
            .events
            .iter()
            .filter(|other| event.same_object(other) && event.key() != other.key())
            .map(|other| {
                div()
                    .flex()
                    .flex_col()
                    .gap(SPACE_3)
                    .child(StatusText::new(
                        other.status,
                        format!("{} · {} ago", other.reason, age(other.age)),
                    ))
                    .child(other.message.clone())
            })
            .collect::<Vec<_>>();
        let related = if related.is_empty() {
            div().child("No other events for this object.")
        } else {
            div().flex().flex_col().gap(SPACE_4).children(related)
        };
        Inspector::new(INSPECTOR_ID, event.reason.clone())
            .status(event.status, event_type(event))
            .actions(close)
            .section(
                Some("Message"),
                div().min_w_0().child(event.message.clone()),
            )
            .section(Some("Details"), facts)
            .section(Some("Involved object"), object)
            .section(Some("Other events"), related)
            .into_any_element()
    }
}

impl Render for EventsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self
            .selected
            .as_ref()
            .and_then(|key| self.events.iter().find(|event| &event.key() == key))
            .cloned();
        self.presence.track(current.as_ref(), presence, cx);
        let inspector = match current {
            Some(event) => {
                let panel = self.inspector(&event, cx);
                Some(slide_in(panel, INSPECTOR_ID, &event.key(), INSPECTOR))
            }
            None => self.presence.leaving().cloned().map(|event| {
                let panel = self.inspector(&event, cx);
                slide_out(panel, INSPECTOR_ID, INSPECTOR)
            }),
        };
        div()
            .size_full()
            .flex()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.selected.is_some() {
                    this.close(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(self.filter.clone())
                    .child(div().flex_1().min_h_0().child(self.table.clone())),
            )
            .children(inspector)
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
            .fixed(EVENT_REASON_WIDTH)
            .sort_by_key(|e: &Event| e.reason.clone()),
        Column::text("Object", |e: &Event| e.object.clone().into())
            .highlight()
            .mono()
            .flex(EVENT_OBJECT_WEIGHT, EVENT_OBJECT_MIN)
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
        .fixed(EVENT_NAMESPACE_WIDTH)
        .sort_by_key(|e: &Event| e.namespace.clone()),
        Column::colored_text(
            "Message",
            |event: &Event| event.message.clone().into(),
            |_, palette| palette.fg2,
        )
        .highlight()
        .tooltip(|event: &Event| event.message.clone().into())
        .flex(EVENT_MESSAGE_WEIGHT, EVENT_MESSAGE_MIN),
        Column::colored_text("Age", |e: &Event| age(e.age).into(), |_, p| p.mute)
            .mono()
            .fixed(EVENT_AGE_WIDTH)
            .sort_by_key(|e: &Event| e.age),
        Column::colored_text("×", |e: &Event| e.count.to_string().into(), |_, p| p.mute)
            .mono()
            .end()
            .fixed(EVENT_COUNT_WIDTH)
            .sort_by_key(|e: &Event| e.count),
    ]
}

fn value(text: &str) -> String {
    if text.is_empty() {
        "Unknown".into()
    } else {
        text.into()
    }
}

fn event_type(event: &Event) -> String {
    if !event.details.event_type.is_empty() {
        return event.details.event_type.clone();
    }
    if event.is_warning() {
        "Warning".into()
    } else {
        "Normal".into()
    }
}

fn seen(duration: Option<std::time::Duration>, absolute: Option<&str>) -> String {
    match (duration, absolute) {
        (Some(duration), Some(absolute)) => format!("{} ago · {absolute}", age(duration)),
        _ => "Unknown".into(),
    }
}
