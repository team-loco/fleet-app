use fleet_core::flux::{
    FluxDetail, HelmDetail, HelmRevision, HistoryEntry, InventoryFilter, ManagedObject, Span,
    SpecRow,
};
use fleet_core::{Command, FluxAction, FluxKind, FluxObject, FluxPage, Page, Status, time};
use gpui::{
    AnyElement, App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, div,
    prelude::FluentBuilder, px, relative,
};

use crate::kit::{
    ActionMenu, Button, CodeView, Column, ConfirmDialog, DiffView, KeyValue, MenuItem, PageHeader,
    Prose, SectionHeader, SectionLabel, Segmented, StatusGlyph, Table, TableEvent, Toast, confirm,
    parse_unified, toast,
};
use crate::theme::{
    ASIDE_WIDTH, ActiveTheme, FONT_MONO, FONT_SANS, LINE_PROSE, Palette, ROW_DOCUMENT,
    ROW_TABLE_MIN, RULE, SPACE_2, SPACE_3, SPACE_6, SPACE_7, SPACE_8, TEXT_BODY, TEXT_MONO,
    TEXT_SMALL,
};

const KEY_CONTEXT: &str = "FluxObject";
const GLYPH_COLUMN: Pixels = px(14.);
const KIND_COLUMN: Pixels = px(170.);
const NAME_MIN: Pixels = px(200.);
const DETAIL_MIN: Pixels = px(160.);
const AGE_COLUMN: Pixels = px(40.);
const REVISION_COLUMN: Pixels = px(70.);
const COUNT_COLUMN: Pixels = px(34.);
const PEER_GLYPH: Pixels = px(12.);
const PEER_CLUSTER: Pixels = px(96.);
const PEER_REVISION: Pixels = px(64.);
const SPEC_LABEL: Pixels = px(64.);
const VALUES_LABEL: Pixels = px(140.);

const RECONCILE: &str = "Reconcile";
const RECONCILE_KEYS: &str = "⌘R";
const WITH_SOURCE_KEYS: &str = "⇧⌘R";
const DIFF_KEYS: &str = "⌘D";
const YAML_KEYS: &str = "⌘Y";
const COMPARE: &str = "Compare rendered output →";

gpui::actions!(
    flux_object,
    [Reconcile, ReconcileWithSource, DiffAgainstGit, ViewYaml]
);

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-r", Reconcile, context),
        KeyBinding::new("cmd-shift-r", ReconcileWithSource, context),
        KeyBinding::new("cmd-d", DiffAgainstGit, context),
        KeyBinding::new("cmd-y", ViewYaml, context),
    ]);
}

pub struct FluxObjectPage {
    page: Option<FluxPage>,
    filter: InventoryFilter,
    yaml_open: bool,
    release: Option<u32>,
    focus: FocusHandle,
    wants_focus: bool,
    menu: Entity<ActionMenu>,
    inventory: Entity<Table<ManagedObject>>,
    history: Entity<Table<HistoryEntry>>,
    releases: Entity<Table<HelmRevision>>,
    dependents: Entity<Table<FluxObject>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Command> for FluxObjectPage {}

impl FluxObjectPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let menu = cx.new(|cx| ActionMenu::new("flux-actions", cx));
        let inventory = cx.new(|cx| {
            Table::new("flux-inventory", inventory_columns(), cx)
                .headerless()
                .row_height(ROW_DOCUMENT)
                .row_key(|o: &ManagedObject| format!("{}/{}", o.kind, o.path()).into())
                .empty("Nothing matches this filter.")
                .fit_content()
        });
        let history = cx.new(|cx| {
            Table::new("flux-history", history_columns(), cx)
                .headerless()
                .row_height(ROW_DOCUMENT)
                .empty("No reconciles recorded yet.")
                .fit_content()
        });
        let releases = cx.new(|cx| {
            Table::new("flux-releases", release_columns(), cx)
                .headerless()
                .row_height(ROW_DOCUMENT)
                .row_key(|r: &HelmRevision| r.revision.to_string().into())
                .empty("Helm has no releases stored for this object.")
                .fit_content()
        });
        let dependents = cx.new(|cx| {
            Table::new("flux-dependents", dependent_columns(), cx)
                .headerless()
                .row_height(ROW_DOCUMENT)
                .empty("Nothing on this cluster builds from this source.")
                .fit_content()
        });
        let subscriptions = vec![
            cx.subscribe(&inventory, |page, table, event, cx| {
                page.inventory_event(&table, event, cx);
            }),
            cx.subscribe(&releases, |page, table, event, cx| {
                page.release_event(&table, event, cx);
            }),
            cx.subscribe(&dependents, |_, table, event, cx| {
                Self::dependent_event(&table, event, cx);
            }),
        ];
        Self {
            page: None,
            filter: InventoryFilter::All,
            yaml_open: false,
            release: None,
            focus: cx.focus_handle(),
            wants_focus: false,
            menu,
            inventory,
            history,
            releases,
            dependents,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_page(&mut self, page: Option<FluxPage>, cx: &mut Context<Self>) {
        let previous = self.page.as_ref().map(|p| p.object.key.clone());
        let next = page.as_ref().map(|p| p.object.key.clone());
        let opened = previous != next;
        if opened {
            self.filter = InventoryFilter::All;
            self.yaml_open = false;
            self.release = None;
            self.wants_focus = next.is_some();
        }
        self.page = page;
        let Some(page) = self.page.clone() else {
            cx.notify();
            return;
        };
        if let FluxDetail::HelmRelease(detail) = &page.object.detail {
            let known = self
                .release
                .filter(|r| detail.releases.iter().any(|x| x.revision == *r));
            self.release = known.or_else(|| detail.first_change());
            let rows = detail.releases.clone();
            let selected = self
                .release
                .and_then(|r| rows.iter().position(|x| x.revision == r));
            self.releases.update(cx, |table, cx| {
                table.set_rows(rows, cx);
                table.select(selected, cx);
            });
        }
        self.refresh_inventory(cx);
        let history = page.object.history.clone();
        self.history
            .update(cx, |table, cx| table.set_rows(history, cx));
        let dependents = page.dependents.clone();
        self.dependents
            .update(cx, |table, cx| table.set_rows(dependents, cx));
        let items = Self::menu_items(&page, cx);
        self.menu.update(cx, |menu, cx| menu.set_items(items, cx));
        cx.notify();
    }

    fn object(&self) -> Option<&FluxObject> {
        self.page.as_ref().map(|page| &page.object)
    }

    fn refresh_inventory(&mut self, cx: &mut Context<Self>) {
        let rows = self
            .object()
            .map(|object| object.inventory(self.filter))
            .unwrap_or_default();
        self.inventory.update(cx, |table, cx| {
            table.set_rows(rows, cx);
            table.select(None, cx);
        });
    }

    fn set_filter(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(filter) = InventoryFilter::ALL.get(ix).copied() else {
            return;
        };
        self.filter = filter;
        self.refresh_inventory(cx);
        cx.notify();
    }

    fn act(&mut self, action: FluxAction, cx: &mut Context<Self>) {
        let Some(key) = self.object().map(|object| object.key.clone()) else {
            return;
        };
        cx.emit(Command::Flux(key, action));
    }

    fn toggle_yaml(&mut self, cx: &mut Context<Self>) {
        self.yaml_open = !self.yaml_open;
        cx.notify();
    }

    fn copy_yaml(&mut self, cx: &mut Context<Self>) {
        let Some(object) = self.object() else {
            return;
        };
        let item = ClipboardItem::new_string(object.yaml.clone());
        let detail = object_detail(object);
        cx.write_to_clipboard(item);
        toast(Toast::new(Status::Ok, "Copied YAML").detail(detail), cx);
    }

    fn inventory_event(
        &mut self,
        table: &Entity<Table<ManagedObject>>,
        event: &TableEvent,
        cx: &mut Context<Self>,
    ) {
        let TableEvent::Selected(ix) = event else {
            return;
        };
        let Some(cluster) = self.object().map(|object| object.key.cluster.clone()) else {
            return;
        };
        let target = table.read(cx).row(*ix).map(|row| row.resource(&cluster));
        if let Some(resource) = target {
            cx.emit(Command::OpenResource(resource));
        }
    }

    fn release_event(
        &mut self,
        table: &Entity<Table<HelmRevision>>,
        event: &TableEvent,
        cx: &mut Context<Self>,
    ) {
        let TableEvent::Selected(ix) = event else {
            return;
        };
        self.release = table.read(cx).row(*ix).map(|r| r.revision);
        cx.notify();
    }

    fn dependent_event(
        table: &Entity<Table<FluxObject>>,
        event: &TableEvent,
        cx: &mut Context<Self>,
    ) {
        let TableEvent::Selected(ix) = event else {
            return;
        };
        let target = table.read(cx).row(*ix).map(|row| row.key.clone());
        if let Some(key) = target {
            cx.emit(Command::OpenFluxObject(key));
        }
    }

    fn menu_items(page: &FluxPage, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let this = cx.weak_entity();
        let object = &page.object;
        let source = object.source.clone();
        let source_name = source.as_ref().map_or_else(String::new, |s| s.name.clone());
        let mut items = Vec::new();
        if let Some(label) = fetch_label(object.key.kind) {
            items.push(
                item(&this, label, |page, _, cx| {
                    page.act(FluxAction::ReconcileWithSource, cx);
                })
                .shortcut(WITH_SOURCE_KEYS)
                .description(format!(
                    "Pull {source_name} now instead of waiting for the next poll."
                )),
            );
        }
        items.push(suspend_item(&this, object));
        if object.key.kind == FluxKind::Kustomization {
            items.push(
                item(&this, "Diff cluster against git", |_, _, cx| {
                    cx.emit(Command::SelectPage(Page::Changes));
                })
                .shortcut(DIFF_KEYS)
                .description("What the next reconcile would change."),
            );
        }
        items.push(
            item(&this, "View YAML", |page, _, cx| page.toggle_yaml(cx))
                .shortcut(YAML_KEYS)
                .description(yaml_description(object)),
        );
        items.push(
            item(&this, "Copy YAML", |page, _, cx| page.copy_yaml(cx))
                .description("Copy the live object, status included."),
        );
        if let Some(source) = source {
            let description = format!(
                "{} {}: URL, ref and last fetch.",
                source.kind.label(),
                source.name
            );
            items.push(
                item(&this, "Open source", move |_, _, cx| {
                    cx.emit(Command::OpenFluxObject(source.clone()));
                })
                .description(description),
            );
        }
        if let Some(force) = force_item(&this, object) {
            items.push(force);
        }
        items
    }

    fn header(&self, object: &FluxObject, cx: &mut Context<Self>) -> AnyElement {
        let suspended = object.phase.is_suspended();
        let this = cx.weak_entity();
        let reconcile = Button::new("flux-reconcile", RECONCILE)
            .primary()
            .shortcut(RECONCILE_KEYS)
            .disabled(suspended)
            .when(suspended, |button| button.tooltip("Resume it first."))
            .on_click(move |_, _, cx| {
                this.update(cx, |page, cx| page.act(FluxAction::Reconcile, cx))
                    .ok();
            });
        PageHeader::new(object.key.name.clone())
            .mono()
            .status(object.status(), object.phase.label())
            .action(reconcile)
            .action(self.menu.clone())
            .sentence(Self::sentence(object, cx))
            .meta(Self::meta(object, cx))
            .into_any_element()
    }

    fn sentence(object: &FluxObject, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let cluster = object.key.cluster.clone();
        let prose = object
            .sentence
            .0
            .iter()
            .fold(Prose::new("flux-sentence"), |prose, span| match span {
                Span::Text(text) => prose.text(text.clone()),
                Span::Object(name) => {
                    let target = object
                        .inventory
                        .iter()
                        .find(|o| &o.name == name)
                        .map(|o| o.resource(&cluster));
                    match target {
                        Some(resource) => {
                            let this = this.clone();
                            prose.link(name.clone(), move |_, cx| {
                                this.update(cx, |_, cx| {
                                    cx.emit(Command::OpenResource(resource.clone()));
                                })
                                .ok();
                            })
                        }
                        None => prose.inert_link(name.clone()),
                    }
                }
            });
        prose.into_any_element()
    }

    fn meta(object: &FluxObject, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let open_commit = move |sha: String| {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                let command = Command::OpenCommit(sha.clone());
                this.update(cx, |_, cx| cx.emit(command)).ok();
            }
        };
        let mut prose = Prose::new("flux-meta");
        match &object.detail {
            FluxDetail::Kustomization => {}
            FluxDetail::HelmRelease(detail) => {
                prose = prose.text(format!("chart {} · ", detail.chart));
            }
            FluxDetail::Source(source) => {
                let fetched = time::age(source.fetched);
                if let Some(artifact) = &object.applied {
                    let git = object.key.kind == FluxKind::GitRepository;
                    let handler = open_commit(artifact.id.clone());
                    prose = revision_link(prose.text("artifact "), &artifact.id, git, handler)
                        .text(format!(" {} ago · ", time::age(artifact.age)));
                }
                return prose
                    .text(format!("last fetch {fetched} ago"))
                    .into_any_element();
            }
        }
        let commits = object.key.kind == FluxKind::Kustomization;
        if let Some(applied) = &object.applied {
            prose = prose.text("applied ");
            prose = revision_link(prose, &applied.id, commits, open_commit(applied.id.clone()));
            prose = prose.text(format!(" {} ago", time::age(applied.age)));
        }
        if let Some(healthy) = object.shown_last_healthy() {
            prose = prose.text(" · last healthy ");
            prose = revision_link(prose, &healthy.id, commits, open_commit(healthy.id.clone()));
            prose = prose.text(format!(" {} ago", time::age(healthy.age)));
        }
        prose.into_any_element()
    }

    fn inventory_section(
        &self,
        object: &FluxObject,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let applied = object.applied.as_ref();
        let selected = InventoryFilter::ALL
            .iter()
            .position(|f| *f == self.filter)
            .unwrap_or_default();
        let filters = InventoryFilter::ALL
            .iter()
            .fold(Segmented::new("inventory-filter"), |group, filter| {
                group.option(filter.label(applied))
            });
        let filters = filters
            .small()
            .selected(selected)
            .on_select(cx.listener(|page, ix: &usize, _, cx| page.set_filter(*ix, cx)));
        div()
            .child(
                SectionHeader::new("Managed objects")
                    .count(object.inventory.len().to_string())
                    .end(filters),
            )
            .child(ruled(self.inventory.clone(), palette))
            .into_any_element()
    }

    fn history_section(&self, palette: &Palette) -> AnyElement {
        div()
            .child(SectionHeader::new("History"))
            .child(ruled(self.history.clone(), palette))
            .into_any_element()
    }

    fn dependents_section(&self, page: &FluxPage, palette: &Palette) -> AnyElement {
        div()
            .child(SectionHeader::new("Used by").count(page.dependents.len().to_string()))
            .child(ruled(self.dependents.clone(), palette))
            .into_any_element()
    }

    fn releases_section(&self, detail: &HelmDetail, palette: &Palette) -> AnyElement {
        let count = format!("{} · select one to see what changed", detail.releases.len());
        let selected = self
            .release
            .and_then(|r| detail.releases.iter().find(|x| x.revision == r));
        div()
            .child(SectionHeader::new("Helm releases").count(count))
            .child(ruled(self.releases.clone(), palette))
            .children(selected.map(|release| revision_diff(detail, release, palette)))
            .into_any_element()
    }

    fn yaml_section(object: &FluxObject) -> AnyElement {
        div()
            .child(SectionHeader::new("YAML").count("read-only"))
            .child(
                div()
                    .px(SPACE_8)
                    .child(CodeView::new("flux-yaml", object.yaml.clone()).yaml()),
            )
            .into_any_element()
    }

    fn main(&self, page: &FluxPage, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let object = &page.object;
        let body: Vec<AnyElement> = match &object.detail {
            FluxDetail::Kustomization => vec![
                self.inventory_section(object, palette, cx),
                self.history_section(palette),
            ],
            FluxDetail::HelmRelease(detail) => vec![
                self.inventory_section(object, palette, cx),
                self.releases_section(detail, palette),
            ],
            FluxDetail::Source(_) => vec![
                self.dependents_section(page, palette),
                self.history_section(palette),
            ],
        };
        div()
            .id("flux-main")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .child(self.header(object, cx))
            .children(body)
            .when(self.yaml_open, |main| {
                main.child(Self::yaml_section(object))
            })
            .child(div().h(SPACE_8))
            .into_any_element()
    }

    fn aside(page: &FluxPage, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let object = &page.object;
        let this = cx.weak_entity();
        let peers: Vec<AnyElement> = page
            .peers
            .iter()
            .enumerate()
            .map(|(ix, peer)| peer_row(ix, peer, &this, palette))
            .collect();
        let verdict = page.verdict.map(|verdict| {
            let this = this.clone();
            div()
                .px(SPACE_6)
                .pt(SPACE_3)
                .line_height(relative(LINE_PROSE))
                .text_color(palette.fg2)
                .child(
                    Prose::new("flux-verdict")
                        .text(format!("{} ", verdict.sentence()))
                        .link(COMPARE, move |_, cx| {
                            this.update(cx, |_, cx| {
                                cx.emit(Command::SelectPage(Page::Changes));
                            })
                            .ok();
                        }),
                )
        });
        let dependencies: Vec<AnyElement> = page
            .dependencies
            .iter()
            .enumerate()
            .map(|(ix, dependency)| {
                let key = dependency.key.clone();
                let this = this.clone();
                div()
                    .id(("flux-dependency", ix))
                    .flex()
                    .items_center()
                    .gap(SPACE_2)
                    .cursor_pointer()
                    .hover(|style| style.text_color(palette.acc))
                    .on_click(move |_, _, cx| {
                        this.update(cx, |_, cx| cx.emit(Command::OpenFluxObject(key.clone())))
                            .ok();
                    })
                    .child(StatusGlyph::new(dependency.status))
                    .child(dependency.key.name.clone())
                    .into_any_element()
            })
            .collect();
        let values = match &object.detail {
            FluxDetail::HelmRelease(detail) => Some(spec_list(&detail.values, VALUES_LABEL)),
            FluxDetail::Kustomization => None,
            FluxDetail::Source(_) => None,
        };
        div()
            .id("flux-aside")
            .w(ASIDE_WIDTH)
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .border_l(RULE)
            .border_color(palette.line)
            .bg(palette.panel)
            .text_size(TEXT_BODY)
            .when(!peers.is_empty(), |aside| {
                aside.child(
                    aside_block(palette)
                        .child(aside_label("Other clusters"))
                        .children(peers)
                        .children(verdict),
                )
            })
            .when(!dependencies.is_empty(), |aside| {
                aside.child(
                    aside_block(palette).child(aside_label("Depends on")).child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(SPACE_2)
                            .px(SPACE_6)
                            .font_family(FONT_MONO)
                            .text_size(TEXT_MONO)
                            .children(dependencies),
                    ),
                )
            })
            .when_some(values, |aside, values| {
                aside.child(
                    aside_block(palette)
                        .child(aside_label("Values"))
                        .child(values),
                )
            })
            .child(
                aside_block(palette)
                    .child(aside_label("Spec"))
                    .child(spec_list(&object.spec, SPEC_LABEL)),
            )
            .into_any_element()
    }
}

impl Render for FluxObjectPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        if std::mem::take(&mut self.wants_focus) {
            window.focus(&self.focus, cx);
        }
        let content = self.page.clone().map(|page| {
            div()
                .flex()
                .size_full()
                .child(self.main(&page, &palette, cx))
                .child(Self::aside(&page, &palette, cx))
        });
        div()
            .id("flux-object")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|page, _: &Reconcile, _, cx| {
                let suspended = page.object().is_some_and(|o| o.phase.is_suspended());
                if !suspended {
                    page.act(FluxAction::Reconcile, cx);
                }
            }))
            .on_action(cx.listener(|page, _: &ReconcileWithSource, _, cx| {
                page.act(FluxAction::ReconcileWithSource, cx);
            }))
            .on_action(cx.listener(|_, _: &DiffAgainstGit, _, cx| {
                cx.emit(Command::SelectPage(Page::Changes));
            }))
            .on_action(cx.listener(|page, _: &ViewYaml, _, cx| page.toggle_yaml(cx)))
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(palette.editor)
            .font_family(FONT_SANS)
            .children(content)
    }
}

fn item(
    this: &WeakEntity<FluxObjectPage>,
    label: &str,
    run: impl Fn(&mut FluxObjectPage, &mut Window, &mut Context<FluxObjectPage>) + 'static,
) -> MenuItem {
    let this = this.clone();
    MenuItem::new(label.to_owned(), move |window, cx| {
        this.update(cx, |page, cx| run(page, window, cx)).ok();
    })
}

fn suspend_item(this: &WeakEntity<FluxObjectPage>, object: &FluxObject) -> MenuItem {
    if object.phase.is_suspended() {
        let source = object
            .source
            .as_ref()
            .map_or_else(|| "the latest revision".to_owned(), |s| s.name.clone());
        item(this, "Resume", |page, _, cx| {
            page.act(FluxAction::Resume, cx);
        })
        .description(format!(
            "Apply {source} here again, starting with a reconcile now."
        ))
    } else {
        item(this, "Suspend", |page, _, cx| {
            page.act(FluxAction::Suspend, cx);
        })
        .description("Stop Flux applying changes here until resumed.")
    }
}

fn force_item(this: &WeakEntity<FluxObjectPage>, object: &FluxObject) -> Option<MenuItem> {
    let (label, body, confirm_label, description) = match object.key.kind {
        FluxKind::Kustomization => (
            "Force apply…",
            "Flux deletes and recreates every object whose immutable fields changed. Pods behind them restart.",
            "Force apply",
            "Recreate objects whose immutable fields changed.",
        ),
        FluxKind::HelmRelease => (
            "Force upgrade…",
            "Helm upgrades the release now even if nothing changed, and the retry count starts over. Pods may restart.",
            "Force upgrade",
            "Run helm upgrade now, even with retries used up.",
        ),
        FluxKind::GitRepository => return None,
        FluxKind::HelmRepository => return None,
        FluxKind::OciRepository => return None,
    };
    let title = format!("{} {}", confirm_label, object.key.name);
    let detail = object_detail(object);
    let weak = this.clone();
    let open =
        move |_: &mut FluxObjectPage, window: &mut Window, cx: &mut Context<FluxObjectPage>| {
            let weak = weak.clone();
            let dialog = ConfirmDialog::new(title.clone(), body, confirm_label, move |_, cx| {
                weak.update(cx, |page, cx| page.act(FluxAction::Force, cx))
                    .ok();
            })
            .detail(detail.clone());
            confirm(dialog, window, cx);
        };
    Some(
        item(this, label, open)
            .destructive()
            .disabled(object.phase.is_suspended())
            .description(description),
    )
}

fn fetch_label(kind: FluxKind) -> Option<&'static str> {
    match kind {
        FluxKind::Kustomization => Some("Fetch git, then reconcile"),
        FluxKind::HelmRelease => Some("Fetch chart, then reconcile"),
        FluxKind::GitRepository => None,
        FluxKind::HelmRepository => None,
        FluxKind::OciRepository => None,
    }
}

fn yaml_description(object: &FluxObject) -> String {
    match object.key.kind {
        FluxKind::Kustomization => object.source.as_ref().map_or_else(
            || "Read-only. Edits belong in git.".to_owned(),
            |s| format!("Read-only. Edits belong in {}.", s.name),
        ),
        FluxKind::HelmRelease => "Read-only. Edits belong in git.".to_owned(),
        FluxKind::GitRepository => "Read-only. Edits belong in git.".to_owned(),
        FluxKind::HelmRepository => "Read-only. Edits belong in git.".to_owned(),
        FluxKind::OciRepository => "Read-only. Edits belong in git.".to_owned(),
    }
}

fn object_detail(object: &FluxObject) -> String {
    format!(
        "{} · {}/{}",
        object.key.cluster, object.key.namespace, object.key.name
    )
}

fn revision_link(
    prose: Prose,
    id: &str,
    linked: bool,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> Prose {
    if linked {
        prose.link(id.to_owned(), handler)
    } else {
        prose.text(id.to_owned())
    }
}

fn ruled(table: impl IntoElement, palette: &Palette) -> impl IntoElement {
    div().border_t(RULE).border_color(palette.line).child(table)
}

fn aside_block(palette: &Palette) -> gpui::Div {
    div().pb(SPACE_7).border_b(RULE).border_color(palette.line)
}

fn aside_label(text: &'static str) -> impl IntoElement {
    div()
        .px(SPACE_6)
        .pt(SPACE_7)
        .pb(SPACE_3)
        .child(SectionLabel::new(text))
}

fn spec_list(rows: &[SpecRow], label_width: Pixels) -> impl IntoElement {
    let list = rows.iter().fold(KeyValue::new(), |list, row| {
        list.row(row.label.clone(), row.value.clone())
    });
    div()
        .px(SPACE_6)
        .font_family(FONT_MONO)
        .text_size(TEXT_SMALL)
        .child(list.label_width(label_width))
}

fn peer_row(
    ix: usize,
    peer: &FluxObject,
    this: &WeakEntity<FluxObjectPage>,
    palette: &Palette,
) -> AnyElement {
    let key = peer.key.clone();
    let this = this.clone();
    let revision = peer
        .applied
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_default();
    let hover = palette.hover;
    div()
        .id(("flux-peer", ix))
        .flex()
        .items_center()
        .gap(SPACE_3)
        .h(ROW_TABLE_MIN)
        .px(SPACE_6)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(move |_, _, cx| {
            this.update(cx, |_, cx| cx.emit(Command::OpenFluxObject(key.clone())))
                .ok();
        })
        .font_family(FONT_MONO)
        .text_size(TEXT_MONO)
        .child(
            div()
                .w(PEER_GLYPH)
                .flex_none()
                .child(StatusGlyph::new(peer.status())),
        )
        .child(
            div()
                .w(PEER_CLUSTER)
                .flex_none()
                .truncate()
                .font_family(FONT_SANS)
                .text_size(TEXT_BODY)
                .child(peer.key.cluster.to_string()),
        )
        .child(
            div()
                .w(PEER_REVISION)
                .flex_none()
                .truncate()
                .text_color(palette.acc)
                .child(revision),
        )
        .child(
            div()
                .flex_1()
                .whitespace_nowrap()
                .text_color(palette.mute)
                .child(peer.phase.note()),
        )
        .into_any_element()
}

fn revision_diff(detail: &HelmDetail, release: &HelmRevision, palette: &Palette) -> AnyElement {
    let previous = detail.previous(release.revision);
    let title = match previous {
        Some(previous) => format!(
            "Revision {} against {}",
            release.revision, previous.revision
        ),
        None => format!("Revision {}", release.revision),
    };
    let body: AnyElement = if release.diff.is_empty() {
        let text = match previous {
            Some(previous) => format!(
                "Same chart and values as revision {}; Helm ran it again.",
                previous.revision
            ),
            None => "The first release; there is nothing earlier to compare with.".to_owned(),
        };
        div()
            .px(SPACE_8)
            .text_color(palette.fg2)
            .child(text)
            .into_any_element()
    } else {
        diff_view(release, previous)
    };
    div()
        .child(
            SectionHeader::new(title)
                .count(format!("{} · {}", release.chart_version, release.state)),
        )
        .child(body)
        .into_any_element()
}

fn diff_view(release: &HelmRevision, previous: Option<&HelmRevision>) -> AnyElement {
    let files = parse_unified(&release.diff);
    let old = previous.map_or_else(String::new, |p| format!("revision {}", p.revision));
    let new = format!("revision {}", release.revision);
    div()
        .px(SPACE_8)
        .child(DiffView::new("flux-release-diff", files).sides(old, new))
        .into_any_element()
}

fn glyph_column<R: 'static>(status: impl Fn(&R) -> Status + 'static) -> Column<R> {
    Column::new("", move |row: &R, _| {
        StatusGlyph::new(status(row)).into_any_element()
    })
    .fixed(GLYPH_COLUMN)
}

fn inventory_columns() -> Vec<Column<ManagedObject>> {
    vec![
        glyph_column(|o: &ManagedObject| o.status),
        Column::colored_text(
            "Kind",
            |o: &ManagedObject| o.kind.clone().into(),
            |_, p| p.mute,
        )
        .fixed(KIND_COLUMN),
        Column::text("Name", |o: &ManagedObject| o.path().into())
            .mono()
            .flex(1., NAME_MIN),
        Column::new("Detail", |o: &ManagedObject, palette| {
            let color = if o.status == Status::Ok {
                palette.mute
            } else {
                palette.status(o.status)
            };
            div()
                .truncate()
                .font_family(FONT_MONO)
                .text_size(TEXT_SMALL)
                .text_color(color)
                .child(SharedString::from(o.detail.clone()))
                .into_any_element()
        })
        .flex(1., DETAIL_MIN),
    ]
}

fn history_columns() -> Vec<Column<HistoryEntry>> {
    vec![
        Column::colored_text(
            "Age",
            |h: &HistoryEntry| h.age_label().into(),
            |_, p| p.mute,
        )
        .mono()
        .fixed(AGE_COLUMN),
        glyph_column(|h: &HistoryEntry| h.status),
        Column::colored_text(
            "Revision",
            |h: &HistoryEntry| h.revision.clone().into(),
            |_, p| p.acc,
        )
        .mono()
        .fixed(REVISION_COLUMN),
        Column::colored_text(
            "Message",
            |h: &HistoryEntry| h.message.clone().into(),
            |_, p| p.fg2,
        )
        .mono()
        .flex(1., Pixels::ZERO),
        Column::colored_text(
            "Count",
            |h: &HistoryEntry| h.count_label().unwrap_or_default().into(),
            |_, p| p.mute,
        )
        .mono()
        .end()
        .fixed(COUNT_COLUMN),
    ]
}

fn release_columns() -> Vec<Column<HelmRevision>> {
    vec![
        Column::colored_text(
            "Age",
            |r: &HelmRevision| time::age(r.age).into(),
            |_, p| p.mute,
        )
        .mono()
        .fixed(AGE_COLUMN),
        glyph_column(|r: &HelmRevision| r.status),
        Column::colored_text(
            "Chart",
            |r: &HelmRevision| r.chart_version.clone().into(),
            |_, p| p.acc,
        )
        .mono()
        .fixed(REVISION_COLUMN),
        Column::colored_text(
            "Description",
            |r: &HelmRevision| format!("{} · {}", r.state, r.description).into(),
            |_, p| p.fg2,
        )
        .mono()
        .flex(1., Pixels::ZERO),
        Column::colored_text(
            "Revision",
            |r: &HelmRevision| format!("#{}", r.revision).into(),
            |_, p| p.mute,
        )
        .mono()
        .end()
        .fixed(COUNT_COLUMN),
    ]
}

fn dependent_columns() -> Vec<Column<FluxObject>> {
    vec![
        glyph_column(FluxObject::status),
        Column::colored_text(
            "Kind",
            |o: &FluxObject| o.key.kind.label().into(),
            |_, p| p.mute,
        )
        .fixed(KIND_COLUMN),
        Column::text("Name", |o: &FluxObject| {
            format!("{}/{}", o.key.namespace, o.key.name).into()
        })
        .mono()
        .flex(1., NAME_MIN),
        Column::colored_text(
            "State",
            |o: &FluxObject| o.phase.note().into(),
            |o, p| {
                if o.status() == Status::Ok {
                    p.mute
                } else {
                    p.status(o.status())
                }
            },
        )
        .mono()
        .flex(1., DETAIL_MIN),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_deployers_fetch_before_reconciling() {
        assert!(fetch_label(FluxKind::Kustomization).is_some());
        assert!(fetch_label(FluxKind::HelmRelease).is_some());
        assert!(fetch_label(FluxKind::GitRepository).is_none());
    }
}
