use std::collections::HashMap;

use fleet_core::resources::{Resource, filter_resources, namespaces_of};
use fleet_core::time::age;
use fleet_core::{
    ClusterDetail, ClusterId, ClusterSummary, Command, ResourceKind, ResourceRef, Status,
};
use gpui::{
    AnyElement, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Pixels,
    Render, SharedString, Styled, Subscription, WeakEntity, Window, div, px,
};

use super::ScreenEvent;
use crate::kit::{
    ActionMenu, CodeView, Column, ConfirmDialog, IconButton, InputEvent, Inspector, MenuItem,
    SectionHeader, Select, SelectEvent, StatusGlyph, Table, TableEvent, TextField, TextFieldState,
    confirm,
};
use crate::theme::{SPACE_3, SPACE_4};

const ALL_NAMESPACES: &str = "All namespaces";
const PLACEHOLDER: &str = "filter by name";
const NAME_WEIGHT: f32 = 1.4;
const NAME_MIN: Pixels = px(200.);
const NAMESPACE_WIDTH: Pixels = px(130.);
const AGE_WIDTH: Pixels = px(44.);
const SELECT_WIDTH: Pixels = px(180.);
const YAML_INSPECTOR: Pixels = px(480.);
const STATUS_COLUMNS: [&str; 2] = ["Ready", "Status"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cell {
    Fixed(Pixels),
    End(Pixels),
    Flex(f32, Pixels),
}

const IMAGE: Cell = Cell::Flex(1.4, px(220.));
const DEPLOYMENT_CELLS: [Cell; 4] = [
    Cell::Fixed(px(56.)),
    Cell::End(px(76.)),
    Cell::End(px(72.)),
    IMAGE,
];
const STATEFULSET_CELLS: [Cell; 3] = [Cell::Fixed(px(56.)), Cell::Fixed(px(140.)), IMAGE];
const DAEMONSET_CELLS: [Cell; 4] = [
    Cell::End(px(60.)),
    Cell::End(px(52.)),
    Cell::Fixed(px(230.)),
    IMAGE,
];
const JOB_CELLS: [Cell; 3] = [
    Cell::Fixed(px(84.)),
    Cell::Fixed(px(70.)),
    Cell::Fixed(px(90.)),
];
const CRONJOB_CELLS: [Cell; 4] = [
    Cell::Fixed(px(110.)),
    Cell::Fixed(px(64.)),
    Cell::End(px(52.)),
    Cell::Fixed(px(96.)),
];
const SERVICE_CELLS: [Cell; 3] = [
    Cell::Fixed(px(96.)),
    Cell::Fixed(px(110.)),
    Cell::Fixed(px(90.)),
];
const INGRESS_CELLS: [Cell; 3] = [
    Cell::Fixed(px(60.)),
    Cell::Fixed(px(200.)),
    Cell::Flex(1., px(200.)),
];
const CONFIGMAP_CELLS: [Cell; 1] = [Cell::End(px(48.))];
const SECRET_CELLS: [Cell; 2] = [Cell::Fixed(px(220.)), Cell::End(px(48.))];
const PVC_CELLS: [Cell; 4] = [
    Cell::Fixed(px(64.)),
    Cell::Fixed(px(70.)),
    Cell::Fixed(px(96.)),
    Cell::Flex(1., px(200.)),
];

fn cell_widths(kind: ResourceKind) -> &'static [Cell] {
    match kind {
        ResourceKind::Deployment => &DEPLOYMENT_CELLS,
        ResourceKind::StatefulSet => &STATEFULSET_CELLS,
        ResourceKind::DaemonSet => &DAEMONSET_CELLS,
        ResourceKind::Job => &JOB_CELLS,
        ResourceKind::CronJob => &CRONJOB_CELLS,
        ResourceKind::Service => &SERVICE_CELLS,
        ResourceKind::Ingress => &INGRESS_CELLS,
        ResourceKind::ConfigMap => &CONFIGMAP_CELLS,
        ResourceKind::Secret => &SECRET_CELLS,
        ResourceKind::PersistentVolumeClaim => &PVC_CELLS,
    }
}

pub struct ResourcesScreen {
    cluster: Option<ClusterId>,
    kind: ResourceKind,
    rows: Vec<Resource>,
    namespace: Option<String>,
    query: String,
    selected: Option<(String, String)>,
    revealed: Option<(ResourceKind, String, String)>,
    tables: HashMap<ResourceKind, Entity<Table<Resource>>>,
    namespaces: Entity<Select>,
    search: Entity<TextFieldState>,
    menu: Entity<ActionMenu>,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<ScreenEvent> for ResourcesScreen {}

impl ResourcesScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let namespaces = cx.new(|cx| {
            Select::new("resource-namespace", vec![ALL_NAMESPACES.into()], cx)
                .mono()
                .width(SELECT_WIDTH)
        });
        let search = cx.new(|cx| TextFieldState::new(window, cx).placeholder(PLACEHOLDER));
        let subscriptions = vec![
            cx.subscribe(&namespaces, |this, select, _: &SelectEvent, cx| {
                let label = select.read(cx).selected_label().cloned();
                this.namespace = label
                    .filter(|l| l.as_ref() != ALL_NAMESPACES)
                    .map(|l| l.to_string());
                this.refresh(cx);
            }),
            cx.subscribe(&search, |this, state, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.query = state.read(cx).value().to_string();
                    this.refresh(cx);
                }
            }),
        ];
        let menu = cx.new(|cx| ActionMenu::new("resource-actions", cx));
        Self {
            cluster: None,
            kind: ResourceKind::Deployment,
            rows: Vec::new(),
            namespace: None,
            query: String::new(),
            selected: None,
            revealed: None,
            tables: HashMap::new(),
            namespaces,
            search,
            menu,
            subscriptions,
        }
    }

    pub fn set(
        &mut self,
        cluster: &ClusterSummary,
        kind: ResourceKind,
        detail: &ClusterDetail,
        cx: &mut Context<Self>,
    ) {
        let rows = detail.resources(kind);
        let switched = self.kind != kind || self.cluster.as_ref() != Some(&cluster.id);
        if !switched && self.rows == rows {
            return;
        }
        if switched {
            self.selected = None;
            self.namespace = None;
        }
        if let Some((_, namespace, name)) = self.revealed.take_if(|(k, _, _)| *k == kind) {
            self.selected = Some((namespace, name));
        }
        self.cluster = Some(cluster.id.clone());
        self.kind = kind;
        self.rows = rows.to_vec();
        let mut options: Vec<SharedString> = vec![ALL_NAMESPACES.into()];
        options.extend(
            namespaces_of(&self.rows)
                .into_iter()
                .map(SharedString::from),
        );
        let current = self.namespace.clone();
        self.namespaces.update(cx, |select, cx| {
            select.set_options(options.clone(), cx);
            let ix = current
                .and_then(|ns| options.iter().position(|o| o.as_ref() == ns))
                .unwrap_or_default();
            select.set_selected(ix, cx);
        });
        self.refresh(cx);
    }

    pub fn reveal(
        &mut self,
        kind: ResourceKind,
        namespace: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        if kind == self.kind && self.cluster.is_some() {
            self.selected = Some((namespace, name));
            self.refresh(cx);
        } else {
            self.revealed = Some((kind, namespace, name));
        }
    }

    pub fn header_note(&self) -> (Status, String) {
        let flagged = self.rows.iter().filter(|r| r.status != Status::Ok).count();
        let worst = Status::worst(self.rows.iter().map(|r| r.status));
        let text = if flagged > 0 {
            format!("{flagged} of {} need attention", self.rows.len())
        } else {
            self.rows.len().to_string()
        };
        (worst, text)
    }

    fn table(&mut self, cx: &mut Context<Self>) -> Entity<Table<Resource>> {
        let kind = self.kind;
        if let Some(table) = self.tables.get(&kind) {
            return table.clone();
        }
        let table = cx.new(|cx| {
            Table::new(format!("resources-{}", kind.name()), columns(kind), cx)
                .row_key(|r: &Resource| format!("{}/{}", r.namespace, r.name).into())
                .row_status(|r: &Resource| (r.status != Status::Ok).then_some(r.status))
                .empty("Nothing matches the filter.")
        });
        let subscription = cx.subscribe(&table, |this, table, event: &TableEvent, cx| {
            let ix = match *event {
                TableEvent::Selected(ix) => ix,
                TableEvent::Opened(ix) => ix,
            };
            let key = table
                .read(cx)
                .row(ix)
                .map(|r| (r.namespace.clone(), r.name.clone()));
            this.select(key, cx);
            if let TableEvent::Opened(_) = event {
                this.open(cx);
            }
        });
        self.subscriptions.push(subscription);
        self.tables.insert(kind, table.clone());
        table
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let rows = filter_resources(&self.rows, self.namespace.as_deref(), &self.query);
        if self
            .selected
            .as_ref()
            .is_some_and(|(ns, name)| !rows.iter().any(|r| &r.namespace == ns && &r.name == name))
        {
            self.selected = None;
        }
        let position = self.selected.as_ref().and_then(|(ns, name)| {
            rows.iter()
                .position(|r| &r.namespace == ns && &r.name == name)
        });
        let table = self.table(cx);
        table.update(cx, |table, cx| {
            table.set_rows(rows, cx);
            if position.is_some() {
                table.select(position, cx);
            }
        });
        self.refresh_menu(cx);
        cx.notify();
    }

    fn select(&mut self, key: Option<(String, String)>, cx: &mut Context<Self>) {
        self.selected = key;
        let target = self.selected_row().and_then(|row| self.target(row));
        self.refresh_menu(cx);
        if let Some(target) = target {
            cx.emit(ScreenEvent::Command(Command::InspectResource(target)));
        }
        cx.notify();
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        let target = self.selected_row().and_then(|row| self.target(row));
        if let Some(target) = target {
            cx.emit(ScreenEvent::Command(Command::OpenResource(target)));
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        let table = self.table(cx);
        table.update(cx, |table, cx| table.select(None, cx));
        self.select(None, cx);
    }

    fn selected_row(&self) -> Option<&Resource> {
        let (ns, name) = self.selected.as_ref()?;
        self.rows
            .iter()
            .find(|r| &r.namespace == ns && &r.name == name)
    }

    fn target(&self, row: &Resource) -> Option<ResourceRef> {
        Some(ResourceRef {
            cluster: self.cluster.clone()?,
            kind: self.kind.name().into(),
            namespace: Some(row.namespace.clone()),
            name: row.name.clone(),
        })
    }

    fn refresh_menu(&mut self, cx: &mut Context<Self>) {
        let target = self.selected_row().and_then(|r| self.target(r));
        let items = target
            .map(|target| delete_items(self.kind, target, cx.weak_entity()))
            .unwrap_or_default();
        self.menu.update(cx, |menu, cx| menu.set_items(items, cx));
    }

    fn inspector(&self, row: &Resource, cx: &mut Context<Self>) -> AnyElement {
        let close = IconButton::glyph("close-resource", "×", "Close")
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
        let actions = div()
            .flex()
            .gap(SPACE_3)
            .child(self.menu.clone())
            .child(close);
        let subtitle = format!("{} · {}", self.kind.name(), row.namespace);
        Inspector::new("resource-inspector", row.name.clone())
            .mono()
            .width(YAML_INSPECTOR)
            .status(row.status, subtitle)
            .actions(actions)
            .section(
                None,
                CodeView::new("resource-yaml", row.yaml.clone()).yaml(),
            )
            .into_any_element()
    }
}

impl Render for ResourcesScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let flagged = self.rows.iter().filter(|r| r.status != Status::Ok).count();
        let count = if flagged > 0 {
            format!("{} · {flagged} need attention", self.rows.len())
        } else {
            self.rows.len().to_string()
        };
        let controls = div()
            .flex()
            .items_center()
            .gap(SPACE_4)
            .child(self.namespaces.clone())
            .child(TextField::search(&self.search));
        let table = self.table(cx);
        let inspector = self
            .selected_row()
            .cloned()
            .map(|row| self.inspector(&row, cx));
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        SectionHeader::new(self.kind.page().label())
                            .count(count)
                            .end(controls),
                    )
                    .child(div().flex_1().min_h_0().child(table)),
            )
            .children(inspector)
    }
}

fn delete_items(
    kind: ResourceKind,
    target: ResourceRef,
    screen: WeakEntity<ResourcesScreen>,
) -> Vec<MenuItem> {
    let title = format!("Delete {}", kind.name());
    let detail = format!("{} · {}", target.cluster, target);
    vec![
        MenuItem::new("Delete…", move |window, cx| {
            let screen = screen.clone();
            let target = target.clone();
            let dialog = ConfirmDialog::new(
                title.clone(),
                "It is removed from the cluster now. If Flux or another controller manages it, it comes back on the next reconcile.",
                title.clone(),
                move |_, cx| {
                    let command = Command::Delete(target.clone());
                    let _ = screen.update(cx, |_, cx| cx.emit(ScreenEvent::Command(command)));
                },
            )
            .detail(detail.clone());
            confirm(dialog, window, cx);
        })
        .destructive()
        .description("Remove it from the cluster. Managed objects come back on reconcile."),
    ]
}

fn columns(kind: ResourceKind) -> Vec<Column<Resource>> {
    let name = Column::new("Name", |r: &Resource, _| {
        div()
            .flex()
            .items_center()
            .gap(SPACE_4)
            .min_w_0()
            .child(StatusGlyph::new(r.status))
            .child(div().truncate().child(r.name.clone()))
            .into_any_element()
    })
    .mono()
    .flex(NAME_WEIGHT, NAME_MIN)
    .sort_by_key(|r: &Resource| r.name.clone());
    let namespace = Column::colored_text(
        "Namespace",
        |r: &Resource| r.namespace.clone().into(),
        |_, p| p.fg2,
    )
    .fixed(NAMESPACE_WIDTH)
    .sort_by_key(|r: &Resource| r.namespace.clone());
    let cells = kind
        .columns()
        .iter()
        .zip(cell_widths(kind))
        .enumerate()
        .map(|(ix, (title, width))| cell_column(ix, title, *width));
    let age_column = Column::colored_text("Age", |r: &Resource| age(r.age).into(), |_, p| p.mute)
        .mono()
        .fixed(AGE_WIDTH)
        .sort_by_key(|r: &Resource| r.age);
    std::iter::once(name)
        .chain(std::iter::once(namespace))
        .chain(cells)
        .chain(std::iter::once(age_column))
        .collect()
}

fn cell_column(ix: usize, title: &'static str, width: Cell) -> Column<Resource> {
    let tinted = STATUS_COLUMNS.contains(&title);
    let column = Column::colored_text(
        title,
        move |r: &Resource| r.cells.get(ix).cloned().unwrap_or_default().into(),
        move |r, p| {
            if tinted && r.status != Status::Ok {
                p.status(r.status)
            } else {
                p.fg2
            }
        },
    )
    .mono()
    .sort_by_key(move |r: &Resource| r.cells.get(ix).cloned());
    match width {
        Cell::Fixed(w) => column.fixed(w),
        Cell::End(w) => column.end().fixed(w),
        Cell::Flex(weight, min) => column.flex(weight, min),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_width_per_column() {
        for kind in ResourceKind::ALL {
            assert_eq!(cell_widths(kind).len(), kind.columns().len(), "{kind:?}");
        }
    }
}
