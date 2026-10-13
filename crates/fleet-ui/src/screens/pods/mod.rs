use fleet_core::pods::{
    Container, Pod, PodFilter, PodQuery, filter_counts, namespaces, nav_counts,
};
use fleet_core::resources::{POD_LIST, namespace_choices};
use fleet_core::settings::NamespaceSelection;
use fleet_core::{ClusterId, Command, Counts, Preferences, ResourceRef, Seen, Snapshot, Status};
use gpui::{
    AnyElement, AppContext as _, Context, Entity, EventEmitter, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};

mod streams;

use streams::{LogSession, SEARCH_PLACEHOLDER, ShellSession, TERMINAL_ID};

use crate::kit::{
    ActionMenu, Button, Column, ConfirmDialog, DockState, FilterBar, FilterEvent, InputEvent,
    InspectorPane, KeyValue, Link, LogView, MenuItem, SectionLabel, Select, SelectEvent,
    StatusGlyph, StatusText, Table, TableEvent, Terminal, TextFieldState, chip_label, confirm,
    swap_in,
};
use crate::theme::{
    ActiveTheme, CONTAINER_SQUARE, CONTAINER_SQUARE_GAP, FONT_MONO, LINE_PROSE, Palette,
    RADIUS_SMALL, RULE, SPACE_1, SPACE_2, SPACE_3, SPACE_4, SPACE_5, SPACE_6, SPACE_7, TEXT_BODY,
    TEXT_MONO, TEXT_SMALL, TEXT_UI,
};

const INSPECTOR_ID: &str = "pod-inspector";
const NAME_MIN: Pixels = px(220.);
const NAME_WEIGHT: f32 = 2.;
const NAMESPACE: Pixels = px(100.);
const CONTAINERS: Pixels = px(72.);
const RESTARTS: Pixels = px(64.);
const AGE: Pixels = px(48.);
const STATUS: Pixels = px(150.);
const NAMESPACE_SELECT: Pixels = px(180.);
const CONTAINER_SELECT: Pixels = px(160.);
const INSPECTOR_LABEL: Pixels = px(96.);

const ALL_NAMESPACES: &str = "All namespaces";
const MISSING: &str = "—";
const TRUE: &str = "True";
const FALSE: &str = "False";
const NO_PODS: &str = "No pods match the filter.";
const NO_EVENTS: &str = "No recent events.";
const NO_SELECTION: &str = "Select a pod to see its containers, conditions and events.";
pub struct PodsScreen {
    pods: Vec<Pod>,
    cluster: Option<ClusterId>,
    query: PodQuery,
    selected: Option<ResourceRef>,
    container: Option<String>,
    log: Option<LogSession>,
    shell: Option<ShellSession>,
    table: Entity<Table<Pod>>,
    filter: Entity<FilterBar>,
    namespace: Entity<Select>,
    preferences: Preferences,
    containers: Entity<Select>,
    search: Entity<TextFieldState>,
    logs: Entity<LogView>,
    terminal: Entity<Terminal>,
    dock: Entity<DockState>,
    menu: Entity<ActionMenu>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Command> for PodsScreen {}

impl PodsScreen {
    pub fn list_focus(&self, cx: &gpui::App) -> gpui::FocusHandle {
        gpui::Focusable::focus_handle(&self.table, cx)
    }

    pub fn dock(&self) -> &Entity<DockState> {
        &self.dock
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            Table::new("pods", columns(), cx)
                .row_key(|pod: &Pod| pod.name.clone().into())
                .column_gap(SPACE_7)
                .empty(NO_PODS)
        });
        let search = cx.new(|cx| TextFieldState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
        let namespace = cx.new(|cx| {
            Select::new("pod-namespace", vec![ALL_NAMESPACES.into()], cx)
                .mono()
                .width(NAMESPACE_SELECT)
        });
        let filter = cx.new(|cx| FilterBar::new("pod-filter", window, cx).scope(namespace.clone()));
        let containers = cx.new(|cx| {
            Select::new("pod-container", Vec::new(), cx)
                .mono()
                .width(CONTAINER_SELECT)
        });
        let logs = cx.new(|_| LogView::new("pod-logs"));
        let terminal = cx.new(|cx| Terminal::new(TERMINAL_ID, cx));
        let dock = cx.new(|_| DockState::new());
        let menu = cx.new(|cx| ActionMenu::new("pod-actions", cx));
        let subscriptions = vec![
            cx.subscribe(&table, |this, table, event: &TableEvent, cx| {
                let ix = match *event {
                    TableEvent::Selected(ix) => ix,
                    TableEvent::Opened(ix) => ix,
                };
                let pod = table.read(cx).row(ix).map(Pod::resource);
                if let Some(pod) = pod {
                    this.select(Some(pod), cx);
                }
            }),
            cx.subscribe(&filter, |this, bar, FilterEvent::Changed, cx| {
                let (text, chip) = {
                    let bar = bar.read(cx);
                    (bar.query().clone(), bar.chip())
                };
                this.query.text = text;
                this.query.filter = PodFilter::ALL.get(chip).copied().unwrap_or_default();
                this.refresh(cx);
            }),
            cx.subscribe(&search, |this, state, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value();
                    this.logs.update(cx, |logs, cx| logs.set_query(text, cx));
                }
            }),
            cx.subscribe(&namespace, |this, _, SelectEvent::Changed(ix), cx| {
                let chosen = ix
                    .checked_sub(1)
                    .and_then(|ix| this.namespaces().get(ix).cloned());
                this.choose_namespace(chosen, cx);
            }),
            cx.subscribe(&containers, |this, select, SelectEvent::Changed(ix), cx| {
                let name = select.read(cx).selected_label().map(ToString::to_string);
                let name = name.or_else(|| {
                    this.selected_pod()
                        .and_then(|pod| pod.containers.get(*ix))
                        .map(|c| c.name.clone())
                });
                this.pick_container(name);
                this.sync_streams(cx);
            }),
            cx.observe(&dock, |this, _, cx| this.sync_streams(cx)),
        ];
        Self {
            pods: Vec::new(),
            cluster: None,
            query: PodQuery::default(),
            selected: None,
            container: None,
            log: None,
            shell: None,
            table,
            filter,
            namespace,
            preferences: Preferences::default(),
            containers,
            search,
            logs,
            terminal,
            dock,
            menu,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_snapshot(&mut self, snapshot: &Snapshot, cx: &mut Context<Self>) {
        self.pods.clone_from(&snapshot.pods);
        self.refresh(cx);
    }

    pub fn set_context(
        &mut self,
        cluster: Option<&ClusterId>,
        resource: Option<&ResourceRef>,
        preferences: &Preferences,
        cx: &mut Context<Self>,
    ) {
        if self.cluster.as_ref() != cluster {
            self.cluster = cluster.cloned();
            self.selected = None;
        }
        self.preferences = preferences.clone();
        self.query.namespace = preferences
            .namespace_selection(POD_LIST)
            .namespace()
            .map(str::to_owned);
        let wanted = resource.filter(|r| self.pods.iter().any(|pod| pod.is(r)));
        if let Some(pod) = wanted {
            let elsewhere = pod
                .namespace
                .as_ref()
                .filter(|ns| {
                    self.query
                        .namespace
                        .as_ref()
                        .is_some_and(|chosen| chosen != *ns)
                })
                .cloned();
            if let Some(namespace) = elsewhere {
                self.choose_namespace(Some(namespace), cx);
            }
            self.selected = Some(pod.clone());
            self.pick_container(None);
        }
        self.refresh(cx);
    }

    fn choose_namespace(&mut self, choice: Option<String>, cx: &mut Context<Self>) {
        let selection = NamespaceSelection::of(choice.as_deref());
        self.query.namespace = choice;
        self.refresh(cx);
        let Some(cluster) = self.cluster.clone() else {
            return;
        };
        self.preferences.select_namespace(POD_LIST, selection);
        cx.emit(Command::SetPreferences(cluster, self.preferences.clone()));
    }

    pub fn filter_bar(&self) -> &Entity<FilterBar> {
        &self.filter
    }

    pub fn header_note(&self) -> (Counts, String) {
        let total = self
            .cluster
            .as_ref()
            .and_then(|cluster| nav_counts(&self.pods, cluster).total)
            .unwrap_or_default();
        (Counts::default(), total.to_string())
    }

    fn cluster_pods(&self) -> Vec<&Pod> {
        match &self.cluster {
            Some(cluster) => self.query.base(&self.pods, cluster),
            None => Vec::new(),
        }
    }

    fn namespaces(&self) -> Vec<String> {
        let known = self
            .cluster
            .as_ref()
            .map(|cluster| namespaces(&self.pods, cluster))
            .unwrap_or_default();
        namespace_choices(known, self.query.namespace.as_deref())
    }

    fn selected_pod(&self) -> Option<&Pod> {
        let selected = self.selected.as_ref()?;
        self.pods.iter().find(|pod| pod.is(selected))
    }

    fn selected_container(&self) -> Option<&Container> {
        let pod = self.selected_pod()?;
        match &self.container {
            Some(name) => pod.containers.iter().find(|c| &c.name == name),
            None => pod.containers.first(),
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let rows: Vec<Pod> = match &self.cluster {
            Some(cluster) => self
                .query
                .apply(&self.pods, cluster)
                .into_iter()
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        let mut options: Vec<SharedString> = vec![ALL_NAMESPACES.into()];
        options.extend(self.namespaces().into_iter().map(SharedString::from));
        let selected = self
            .query
            .namespace
            .as_ref()
            .and_then(|ns| options.iter().position(|o| o.as_ref() == ns))
            .unwrap_or_default();
        self.namespace.update(cx, |select, cx| {
            select.set_options(options, cx);
            select.set_selected(selected, cx);
        });
        let chips = filter_counts(&self.cluster_pods())
            .into_iter()
            .map(|(filter, n)| chip_label(filter.label(), n))
            .collect();
        let shown = rows.len();
        let total = self.cluster_total();
        self.filter.update(cx, |bar, cx| {
            bar.set_chips(chips, cx);
            bar.set_matches(shown, total, cx);
        });
        let still_there = self
            .selected
            .as_ref()
            .is_some_and(|selected| rows.iter().any(|pod| pod.is(selected)));
        if !still_there {
            self.selected = rows.first().map(Pod::resource);
            self.pick_container(None);
        }
        let position = self
            .selected
            .as_ref()
            .and_then(|selected| rows.iter().position(|pod| pod.is(selected)));
        let scope = self.cluster.as_ref().map(|c| c.as_str().to_owned());
        let words = self.query.text.words.clone();
        self.table.update(cx, |table, cx| {
            if let Some(scope) = scope {
                table.set_scope(scope, cx);
            }
            table.set_highlight(words, cx);
            table.set_rows(rows, cx);
            table.select(position, cx);
        });
        self.sync_selection(cx);
    }

    fn cluster_total(&self) -> usize {
        self.cluster.as_ref().map_or(0, |cluster| {
            self.pods
                .iter()
                .filter(|pod| &pod.cluster == cluster)
                .count()
        })
    }

    fn select(&mut self, pod: Option<ResourceRef>, cx: &mut Context<Self>) {
        if self.selected == pod {
            return;
        }
        self.selected = pod;
        self.pick_container(None);
        self.sync_selection(cx);
    }

    fn pick_container(&mut self, name: Option<String>) {
        self.container = name;
    }

    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        let names: Vec<SharedString> = self
            .selected_pod()
            .map(|pod| {
                pod.containers
                    .iter()
                    .map(|c| SharedString::from(c.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let current = self.selected_container().map(|c| c.name.clone());
        let index = current
            .as_ref()
            .and_then(|name| names.iter().position(|n| n.as_ref() == name))
            .unwrap_or_default();
        self.container = current;
        self.containers.update(cx, |select, cx| {
            select.set_options(names, cx);
            select.set_selected(index, cx);
        });
        self.rebuild_menu(cx);
        self.sync_streams(cx);
        cx.notify();
    }

    fn rebuild_menu(&mut self, cx: &mut Context<Self>) {
        let items = self.selected_pod().map(|pod| menu_items(pod, cx));
        self.menu
            .update(cx, |menu, cx| menu.set_items(items.unwrap_or_default(), cx));
    }

    fn inspector(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        let selection = self
            .selected_pod()
            .map(|pod| format!("{}/{}", pod.namespace, pod.name))
            .unwrap_or_default();
        let (header, body) = match self.selected_pod() {
            Some(pod) => (
                Some(self.pod_header(pod, &palette, cx)),
                Self::pod_details(pod, &palette, cx).into_any_element(),
            ),
            None => (
                None,
                div()
                    .p(SPACE_6)
                    .text_color(palette.mute)
                    .child(NO_SELECTION)
                    .into_any_element(),
            ),
        };
        let panel = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.panel)
            .text_size(TEXT_BODY)
            .children(header)
            .child(
                div()
                    .id(INSPECTOR_ID)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            );
        div()
            .size_full()
            .bg(palette.panel)
            .child(swap_in(panel, INSPECTOR_ID, &selection))
            .into_any_element()
    }

    fn pod_header(&self, pod: &Pod, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let shell = Button::new("pod-shell", "Shell")
            .primary()
            .on_click(cx.listener(|this, _, window, cx| this.show_terminal(window, cx)));
        let header = section(palette)
            .gap(SPACE_3)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(SPACE_4)
                    .child(SectionLabel::new("Pod"))
                    .child(div().flex_1())
                    .child(shell)
                    .child(self.menu.clone()),
            )
            .child(
                div()
                    .font_family(FONT_MONO)
                    .text_size(TEXT_UI)
                    .font_weight(FontWeight::MEDIUM)
                    .child(pod.name.clone()),
            )
            .child(StatusText::new(pod.status, pod.reason.clone()))
            .child(
                div()
                    .text_color(palette.fg2)
                    .line_height(gpui::relative(LINE_PROSE))
                    .child(pod.sentence()),
            );
        header.flex_none().into_any_element()
    }

    fn pod_details(pod: &Pod, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(Self::facts(pod, palette, cx))
            .child(labels(pod, palette))
            .child(Self::containers_section(pod, palette, cx))
            .child(conditions(pod, palette))
            .child(volumes(pod, palette))
            .child(events(pod, palette))
    }

    fn facts(pod: &Pod, palette: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = match &pod.owner {
            Some(owner) => {
                let target = owner.clone();
                let link = Link::new("pod-owner", format!("{}/{}", owner.kind, owner.name))
                    .mono()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(Command::OpenResource(target.clone()));
                    }));
                div().min_w_0().truncate().child(link).into_any_element()
            }
            None => missing(palette),
        };
        let created = format!("{} ago", fleet_core::time::age(pod.age));
        let ready = div()
            .text_color(palette.status(pod.status))
            .child(format!("{} containers", pod.ready_label()));
        section(palette).child(
            KeyValue::new()
                .label_width(INSPECTOR_LABEL)
                .row("Namespace", pod.namespace.clone())
                .row("Node", or_missing(pod.node.clone()))
                .row("Pod IP", or_missing(pod.ip.clone()))
                .row("Ready", ready)
                .row("Controlled by", owner)
                .row("QoS", pod.qos.label())
                .row("Created", created),
        )
    }

    fn containers_section(
        pod: &Pod,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let rows: Vec<AnyElement> = pod
            .containers
            .iter()
            .enumerate()
            .map(|(ix, container)| Self::container_row(pod, ix, container, palette, cx))
            .collect();
        section(palette)
            .gap(SPACE_5)
            .child(SectionLabel::new("Containers"))
            .children(rows)
    }

    fn container_row(
        pod: &Pod,
        ix: usize,
        container: &Container,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ports: Vec<AnyElement> = container
            .ports
            .iter()
            .enumerate()
            .map(|(port_ix, port)| {
                let target = pod.resource();
                let remote = port.port;
                let id =
                    gpui::ElementId::NamedInteger(format!("forward-{ix}").into(), port_ix as u64);
                div()
                    .flex()
                    .gap(SPACE_4)
                    .font_family(FONT_MONO)
                    .text_size(TEXT_SMALL)
                    .child(div().text_color(palette.fg2).child(port.label()))
                    .child(
                        Link::new(id, "Forward").on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(Command::PortForward {
                                service: target.clone(),
                                local: remote,
                                remote,
                            });
                        })),
                    )
                    .into_any_element()
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(SPACE_1)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(SPACE_3)
                    .child(StatusGlyph::new(container.status))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM)
                            .child(container.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(TEXT_SMALL)
                            .text_color(palette.status(container.status))
                            .child(container.reason.clone()),
                    ),
            )
            .child(mono_line(container.image.clone(), palette.fg2))
            .child(mono_line(container.meta(), palette.mute))
            .children(ports)
            .into_any_element()
    }
}

fn section(palette: &Palette) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(SPACE_4)
        .px(SPACE_6)
        .py(SPACE_5)
        .border_b(RULE)
        .border_color(palette.line)
}

fn mono_line(text: impl Into<SharedString>, color: gpui::Hsla) -> impl IntoElement {
    div()
        .font_family(FONT_MONO)
        .text_size(TEXT_SMALL)
        .text_color(color)
        .child(text.into())
}

fn missing(palette: &Palette) -> AnyElement {
    div()
        .text_color(palette.mute)
        .child(MISSING)
        .into_any_element()
}

fn or_missing(value: Option<String>) -> SharedString {
    value.map_or_else(|| MISSING.into(), SharedString::from)
}

fn labels(pod: &Pod, palette: &Palette) -> impl IntoElement {
    let items = pod.labels.iter().map(|(key, value)| {
        div()
            .font_family(FONT_MONO)
            .text_size(TEXT_SMALL)
            .text_color(palette.fg2)
            .child(format!("{key}={value}"))
    });
    section(palette).child(SectionLabel::new("Labels")).child(
        div()
            .flex()
            .flex_wrap()
            .gap_x(SPACE_5)
            .gap_y(SPACE_2)
            .children(items),
    )
}

fn conditions(pod: &Pod, palette: &Palette) -> impl IntoElement {
    let rows = pod.conditions.iter().fold(
        KeyValue::new().label_width(INSPECTOR_LABEL),
        |grid, condition| {
            let (status, word) = if condition.ok {
                (Status::Ok, TRUE)
            } else {
                (Status::Err, FALSE)
            };
            let text = match &condition.reason {
                Some(reason) => format!("{word} · {reason}"),
                None => word.to_owned(),
            };
            grid.row(condition.kind.clone(), StatusText::new(status, text))
        },
    );
    section(palette)
        .child(SectionLabel::new("Conditions"))
        .child(rows)
}

fn volumes(pod: &Pod, palette: &Palette) -> impl IntoElement {
    let rows = pod.volumes.iter().fold(
        KeyValue::new().label_width(INSPECTOR_LABEL),
        |grid, volume| grid.row(volume.name.clone(), volume.source.clone()),
    );
    section(palette)
        .child(SectionLabel::new("Volumes"))
        .child(rows)
}

fn events(pod: &Pod, palette: &Palette) -> impl IntoElement {
    let rows: Vec<AnyElement> = pod
        .events
        .iter()
        .map(|event| {
            div()
                .flex()
                .flex_col()
                .gap(SPACE_1)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(SPACE_3)
                        .font_family(FONT_MONO)
                        .text_size(TEXT_MONO)
                        .child(StatusGlyph::new(event.status))
                        .child(div().flex_1().child(event.reason.clone()))
                        .child(div().text_color(palette.mute).child(format!(
                            "×{} · {}",
                            event.count,
                            fleet_core::time::age(event.age)
                        ))),
                )
                .child(div().text_color(palette.fg2).child(event.message.clone()))
                .into_any_element()
        })
        .collect();
    let empty = rows.is_empty();
    section(palette)
        .child(SectionLabel::new("Events"))
        .children(rows)
        .when(empty, |s| {
            s.child(div().text_color(palette.mute).child(NO_EVENTS))
        })
}

fn menu_items(pod: &Pod, cx: &mut Context<PodsScreen>) -> Vec<MenuItem> {
    let screen = cx.entity().downgrade();
    let logs_screen = screen.clone();
    let mut items = vec![
        MenuItem::new("Logs", move |_, cx| {
            let _ = logs_screen.update(cx, PodsScreen::show_logs);
        })
        .description("Follow the selected container's output."),
    ];
    for (container, port) in pod.ports() {
        let target = pod.resource();
        let forward_screen = screen.clone();
        let remote = port.port;
        items.push(
            MenuItem::new(format!("Port-forward :{remote}"), move |_, cx| {
                let command = Command::PortForward {
                    service: target.clone(),
                    local: remote,
                    remote,
                };
                let _ = forward_screen.update(cx, |_, cx| cx.emit(command));
            })
            .description(format!(
                "localhost:{remote} → {} {}",
                container.name,
                port.label()
            )),
        );
    }
    let target = pod.resource();
    let seen = Seen::new(target.clone(), pod.uid.clone());
    let replaced = pod.owner.is_some();
    let consequence = if replaced {
        "Its controller creates a replacement within seconds; logs from its containers are lost."
    } else {
        "Nothing recreates it; logs from its containers are lost."
    };
    let detail = format!("{} · {}", target.cluster, target);
    items.push(
        MenuItem::new("Delete pod…", move |window, cx| {
            let seen = seen.clone();
            let screen = screen.clone();
            let dialog = ConfirmDialog::new(
                "Delete pod",
                format!("The pod is deleted now. {consequence}"),
                "Delete pod",
                move |_, cx| {
                    let command = Command::Delete(seen.clone());
                    let _ = screen.update(cx, |_, cx| cx.emit(command));
                },
            )
            .detail(detail.clone());
            confirm(dialog, window, cx);
        })
        .destructive()
        .description(consequence),
    );
    items
}

fn text(value: impl Into<SharedString>, color: gpui::Hsla) -> AnyElement {
    div()
        .truncate()
        .text_color(color)
        .child(value.into())
        .into_any_element()
}

fn container_squares(pod: &Pod, palette: &Palette) -> AnyElement {
    let squares = pod.containers.iter().map(|container| {
        div()
            .size(CONTAINER_SQUARE)
            .flex_none()
            .rounded(RADIUS_SMALL)
            .bg(palette.status(container.status))
    });
    div()
        .flex()
        .gap(CONTAINER_SQUARE_GAP)
        .children(squares)
        .into_any_element()
}

fn columns() -> Vec<Column<Pod>> {
    vec![
        Column::text("Name", |p: &Pod| p.name.clone().into())
            .highlight()
            .mono()
            .flex(NAME_WEIGHT, NAME_MIN)
            .sort_by_key(|p: &Pod| p.name.clone()),
        Column::colored_text(
            "Namespace",
            |p: &Pod| p.namespace.clone().into(),
            |_, palette| palette.fg2,
        )
        .highlight()
        .fixed(NAMESPACE)
        .sort_by_key(|p: &Pod| p.namespace.clone()),
        Column::new("Ctrs", container_squares).fixed(CONTAINERS),
        Column::new("Restarts", |p: &Pod, palette| {
            let color = p
                .restart_status()
                .map_or(palette.mute, |status| palette.status(status));
            text(p.restarts().to_string(), color)
        })
        .mono()
        .end()
        .fixed(RESTARTS)
        .sort_by_key(|p: &Pod| std::cmp::Reverse(p.restarts())),
        Column::new("Age", |p: &Pod, palette| {
            text(fleet_core::time::age(p.age), palette.mute)
        })
        .mono()
        .fixed(AGE)
        .sort_by_key(|p: &Pod| p.age),
        Column::new("Status", |p: &Pod, _| {
            StatusText::new(p.status, p.reason.clone()).into_any_element()
        })
        .fixed(STATUS)
        .sort_by_key(|p: &Pod| std::cmp::Reverse(p.status)),
    ]
}

impl Render for PodsScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inspector = self.inspector(cx);
        let dock = self.dock_view(cx).into_any_element();
        let table = div().flex_1().min_w_0().h_full().child(self.table.clone());
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.filter.clone())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(table)
                    .child(InspectorPane::new(INSPECTOR_ID, inspector)),
            )
            .child(dock)
    }
}
