use std::rc::Rc;
use std::time::Instant;

use flock_core::settings::ClusterAction;
use flock_core::switcher::{self, ADD_CLUSTER, ALL_CLUSTERS, Entry};
use flock_core::{ClusterId, ClusterSummary, Command, Status};
use gpui::{
    AnyElement, App, Context, ElementId, EventEmitter, FocusHandle, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, relative, svg,
};

use super::Shell;
use crate::assets::{CHEVRON_LEFT, CHEVRON_UP, GEAR};
use crate::kit::list_keys::{Nav, TypeAhead, find, typed};
use crate::kit::{
    ConfirmDialog, Dismissal, MenuItem, Tooltip, above_right, appear, confirm, status_symbols,
    surface,
};
use crate::screens::settings::{DISCONNECT, DISCONNECT_DESCRIPTION};
use crate::theme::{
    ActiveTheme, FONT_MONO, FONT_SANS, FONT_STATUS, GLYPH_SLOT, ICON, Palette, RADIUS, ROW_CLUSTER,
    ROW_MENU_PAD_Y, RULE, SPACE_1, SPACE_2, SPACE_4, SWITCHER_GEAR, SWITCHER_LIST, SWITCHER_WIDTH,
    TEXT_BODY, TEXT_GLYPH, TEXT_LABEL, TEXT_SMALL, TEXT_UI,
};

pub const SWITCH_KEY: &str = "⌘⇧C";
pub const ALL_CLUSTERS_KEY: &str = "⌘0";
const ID: &str = "cluster-switcher";
const TOOLTIP: &str = "Switch cluster";
const ADD_GLYPH: &str = "+";
const GEAR_TOOLTIP: &str = "Cluster settings";
const ACTIONS_KEY: &str = ".";
const ENTER: &str = "enter";
const ESCAPE: &str = "escape";
const DESCRIPTION_LINE: f32 = 1.35;
const SETTINGS: &str = "Settings…";
const SETTINGS_DESCRIPTION: &str =
    "Connection details, default namespace, metrics source and labels.";
const CONNECT_DESCRIPTION: &str = "Connect again and resume watching this cluster.";
const REMOVE: &str = "Remove from Flock…";
const REMOVE_DESCRIPTION: &str =
    "Disconnect and forget this cluster. Your kubeconfig is not changed.";
const REMOVE_CONFIRM: &str = "Remove";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitcherEvent {
    Select(Option<ClusterId>),
    AddCluster,
    Run(Command),
    Remove(ClusterId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Settings,
    Connect,
    Disconnect,
    Remove,
}

impl Action {
    pub(super) fn for_cluster(cluster: &ClusterSummary) -> [Self; 3] {
        let connection = if cluster.is_connected() {
            Self::Disconnect
        } else {
            Self::Connect
        };
        [Self::Settings, connection, Self::Remove]
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Settings => SETTINGS,
            Self::Connect => ClusterAction::Connect.label(),
            Self::Disconnect => DISCONNECT,
            Self::Remove => REMOVE,
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Settings => SETTINGS_DESCRIPTION,
            Self::Connect => CONNECT_DESCRIPTION,
            Self::Disconnect => DISCONNECT_DESCRIPTION,
            Self::Remove => REMOVE_DESCRIPTION,
        }
    }

    pub(super) fn event(self, id: &ClusterId) -> SwitcherEvent {
        match self {
            Self::Settings => SwitcherEvent::Run(Command::ShowClusterSettings(id.clone())),
            Self::Connect => SwitcherEvent::Run(Command::Connect(id.clone())),
            Self::Disconnect => SwitcherEvent::Run(Command::Disconnect(id.clone())),
            Self::Remove => SwitcherEvent::Remove(id.clone()),
        }
    }
}

pub(crate) type Run = Rc<dyn Fn(Command, &mut App)>;

fn remove_dialog(id: ClusterId, run: Run) -> ConfirmDialog {
    let title = format!("Remove {id}");
    let body = format!(
        "Flock disconnects from {id} and stops watching it. Your kubeconfig file is not \
         changed, so you can add the context again from Add cluster."
    );
    ConfirmDialog::new(title, body, REMOVE_CONFIRM, move |_, cx: &mut App| {
        run(Command::RemoveCluster(id.clone()), cx);
    })
}

pub(crate) fn cluster_items(cluster: &ClusterSummary, run: &Run) -> Vec<MenuItem> {
    Action::for_cluster(cluster)
        .into_iter()
        .map(|action| {
            let id = cluster.id.clone();
            let run = run.clone();
            let item = MenuItem::new(action.label(), move |window, cx| match action.event(&id) {
                SwitcherEvent::Run(command) => run(command, cx),
                SwitcherEvent::Remove(id) => confirm(remove_dialog(id, run.clone()), window, cx),
                SwitcherEvent::Select(_) => {}
                SwitcherEvent::AddCluster => {}
            })
            .description(action.description());
            match action {
                Action::Remove => item.destructive(),
                Action::Settings => item,
                Action::Connect => item,
                Action::Disconnect => item,
            }
        })
        .collect()
}

fn starts_group(rows: &[Entry], ix: usize) -> bool {
    let Some(row) = rows.get(ix) else {
        return false;
    };
    ix.checked_sub(1)
        .and_then(|before| rows.get(before))
        .is_some_and(|previous| std::mem::discriminant(previous) != std::mem::discriminant(row))
}

fn child_index(rows: &[Entry], ix: usize) -> usize {
    let rules = (0..=ix).filter(|&at| starts_group(rows, at)).count();
    ix + rules
}

pub struct ClusterSwitcher {
    clusters: Vec<ClusterSummary>,
    selected: Option<ClusterId>,
    rows: Vec<Entry>,
    highlighted: Option<usize>,
    steered: bool,
    actions_for: Option<ClusterId>,
    open: bool,
    generation: u64,
    dismissal: Dismissal,
    restore: Option<FocusHandle>,
    focus: FocusHandle,
    type_ahead: TypeAhead,
    scroll: ScrollHandle,
}

impl EventEmitter<SwitcherEvent> for ClusterSwitcher {}

impl ClusterSwitcher {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            clusters: Vec::new(),
            selected: None,
            rows: Vec::new(),
            highlighted: None,
            steered: false,
            actions_for: None,
            open: false,
            generation: 0,
            dismissal: Dismissal::default(),
            restore: None,
            focus: cx.focus_handle(),
            type_ahead: TypeAhead::default(),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_context(
        &mut self,
        clusters: &[ClusterSummary],
        selected: Option<&ClusterId>,
        cx: &mut Context<Self>,
    ) {
        let moved = self.selected.as_ref() != selected;
        self.clusters = clusters.to_vec();
        self.selected = selected.cloned();
        let gone = self
            .actions_for
            .as_ref()
            .is_some_and(|id| self.cluster(id).is_none());
        if gone {
            self.actions_for = None;
        }
        let kept = self
            .highlighted
            .filter(|_| !moved && self.steered)
            .and_then(|ix| self.rows.get(ix))
            .cloned();
        self.rows = switcher::rows(&self.clusters);
        if self.actions_for.is_none() {
            let position = kept.and_then(|entry| self.rows.iter().position(|r| *r == entry));
            self.highlighted = position.or_else(|| self.initial());
        }
        cx.notify();
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
        } else {
            self.show(window, cx);
        }
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.restore = window.focused(cx);
            self.generation += 1;
        }
        self.open = true;
        self.steered = false;
        self.actions_for = None;
        self.rows = switcher::rows(&self.clusters);
        self.highlighted = self.initial();
        self.scroll_to_highlight();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.actions_for = None;
        if let Some(previous) = self.restore.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    fn cluster(&self, id: &ClusterId) -> Option<&ClusterSummary> {
        self.clusters.iter().find(|cluster| &cluster.id == id)
    }

    fn initial(&self) -> Option<usize> {
        switcher::initial_highlight(&self.rows, self.selected.as_ref())
    }

    fn actions(&self) -> Option<(ClusterId, [Action; 3])> {
        let id = self.actions_for.as_ref()?;
        let cluster = self.cluster(id)?;
        Some((id.clone(), Action::for_cluster(cluster)))
    }

    fn scroll_to_highlight(&self) {
        if self.actions_for.is_some() {
            return;
        }
        if let Some(ix) = self.highlighted {
            self.scroll.scroll_to_item(child_index(&self.rows, ix));
        }
    }

    fn jump(&mut self, nav: Nav, cx: &mut Context<Self>) {
        let len = match self.actions() {
            Some((_, actions)) => actions.len(),
            None => self.rows.len(),
        };
        self.highlighted = nav.target(self.highlighted, len);
        self.steered = true;
        self.scroll_to_highlight();
        cx.notify();
    }

    fn type_to(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.actions_for.is_some() {
            return;
        }
        let query = self.type_ahead.push(text, Instant::now()).to_owned();
        let labels = self
            .rows
            .iter()
            .map(|row| switcher::label(row, &self.clusters));
        if let Some(ix) = find(&query, labels) {
            self.highlighted = Some(ix);
            self.steered = true;
            self.scroll_to_highlight();
            cx.notify();
        }
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let event = match self.actions() {
            Some((id, actions)) => actions.get(ix).map(|action| action.event(&id)),
            None => self.rows.get(ix).map(|row| match row {
                Entry::AllClusters => SwitcherEvent::Select(None),
                Entry::Cluster(id) => SwitcherEvent::Select(Some(id.clone())),
                Entry::AddCluster => SwitcherEvent::AddCluster,
            }),
        };
        let Some(event) = event else {
            return;
        };
        self.close(window, cx);
        cx.emit(event);
    }

    fn open_settings(&mut self, id: ClusterId, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        cx.emit(SwitcherEvent::Run(Command::ShowClusterSettings(id)));
    }

    fn show_actions(&mut self, id: ClusterId, cx: &mut Context<Self>) {
        self.actions_for = Some(id);
        self.highlighted = Some(0);
        cx.notify();
    }

    fn show_actions_for_highlighted(&mut self, cx: &mut Context<Self>) {
        let entry = self.highlighted.and_then(|ix| self.rows.get(ix)).cloned();
        if let Some(Entry::Cluster(id)) = entry {
            self.show_actions(id, cx);
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.actions_for.take() else {
            return;
        };
        let entry = Entry::Cluster(id);
        self.highlighted = self.rows.iter().position(|row| *row == entry);
        cx.notify();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_for.is_some() {
            self.back(cx);
        } else {
            self.close(window, cx);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let plain = !keystroke.modifiers.modified();
        if keystroke.modifiers.platform && keystroke.key == ACTIONS_KEY {
            self.show_actions_for_highlighted(cx);
        } else if plain && keystroke.key == ENTER {
            if let Some(ix) = self.highlighted {
                self.activate(ix, window, cx);
            }
        } else if plain && keystroke.key == ESCAPE {
            self.escape(window, cx);
        } else if let Some(nav) = Nav::parse(keystroke) {
            self.jump(nav, cx);
        } else if let Some(text) = typed(keystroke) {
            let text = text.to_owned();
            self.type_to(&text, cx);
        } else {
            return;
        }
        cx.stop_propagation();
    }

    fn render_trigger(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected.as_ref().and_then(|id| self.cluster(id));
        let (glyph, color, name): (SharedString, Hsla, SharedString) = match selected {
            Some(cluster) => (
                cluster.status.glyph().into(),
                palette.status(cluster.status),
                cluster.name.clone().into(),
            ),
            None => (SharedString::default(), palette.mute, ALL_CLUSTERS.into()),
        };
        let hover = palette.hover;
        div()
            .debug_selector(|| ID.to_owned())
            .id(ID)
            .flex()
            .items_center()
            .gap(SPACE_4)
            .h_full()
            .px(crate::theme::SPACE_6)
            .border_l(RULE)
            .border_color(palette.line)
            .cursor_pointer()
            .font_family(FONT_SANS)
            .text_size(TEXT_BODY)
            .font_weight(FontWeight::MEDIUM)
            .text_color(palette.fg)
            .when(self.open, |trigger| trigger.bg(palette.sel))
            .when(!self.open, |trigger| {
                trigger.hover(move |style| style.bg(hover))
            })
            .tooltip(Tooltip::text(format!("{TOOLTIP}  {SWITCH_KEY}")))
            .on_click(cx.listener(|this, _, window, cx| {
                if this.dismissal.take_suppressed() {
                    return;
                }
                this.toggle(window, cx);
            }))
            .when(!glyph.is_empty(), |trigger| {
                trigger.child(
                    div()
                        .font_family(FONT_STATUS)
                        .text_size(TEXT_GLYPH)
                        .text_color(color)
                        .child(glyph),
                )
            })
            .child(name)
            .child(svg().path(CHEVRON_UP).size(ICON).text_color(palette.mute))
            .into_any_element()
    }

    fn render_rows(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut children: Vec<AnyElement> = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            if starts_group(&self.rows, ix) {
                children.push(rule(palette).into_any_element());
            }
            children.push(self.render_row(ix, row, palette, cx));
        }
        div()
            .id("switcher-list")
            .flex()
            .flex_col()
            .max_h(SWITCHER_LIST)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(children)
            .into_any_element()
    }

    fn render_row(
        &self,
        ix: usize,
        row: &Entry,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = self.highlighted == Some(ix);
        let current = match row {
            Entry::AllClusters => self.selected.is_none(),
            Entry::Cluster(id) => self.selected.as_ref() == Some(id),
            Entry::AddCluster => false,
        };
        let base = div()
            .debug_selector(move || format!("switcher-row-{ix}"))
            .id(ElementId::NamedInteger("switcher-row".into(), ix as u64))
            .flex()
            .flex_none()
            .items_center()
            .gap(SPACE_4)
            .h(ROW_CLUSTER)
            .pl(SPACE_4)
            .rounded(RADIUS)
            .cursor_pointer()
            .text_size(TEXT_UI)
            .text_color(palette.fg)
            .when(highlighted, |row| row.bg(palette.hover))
            .when(current, |row| row.bg(palette.sel))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.actions_for.is_none() {
                    this.highlighted = Some(ix);
                    this.steered = true;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.activate(ix, window, cx)));
        let label = |text: SharedString| div().flex_1().min_w_0().truncate().child(text);
        let element = match row {
            Entry::AllClusters => base
                .pr(SPACE_4)
                .child(glyph_cell(SharedString::default(), palette.mute))
                .child(label(ALL_CLUSTERS.into()))
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(TEXT_LABEL)
                        .text_color(palette.mute)
                        .child(ALL_CLUSTERS_KEY),
                ),
            Entry::AddCluster => base
                .text_color(palette.fg2)
                .child(glyph_cell(ADD_GLYPH.into(), palette.mute))
                .child(label(ADD_CLUSTER.into())),
            Entry::Cluster(id) => match self.cluster(id) {
                Some(cluster) => {
                    let actions_id = id.clone();
                    let base = base.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, cx| {
                            this.show_actions(actions_id.clone(), cx);
                        }),
                    );
                    let gear = gear_button(ix, id.clone(), highlighted || current, palette, cx);
                    base.pr(SPACE_1)
                        .when(!cluster.is_connected(), |row| row.text_color(palette.mute))
                        .child(glyph_cell(
                            cluster.status.glyph().into(),
                            palette.status(cluster.status),
                        ))
                        .child(label(cluster.name.clone().into()))
                        .children(cluster_meta(cluster, palette))
                        .child(gear)
                }
                None => base,
            },
        };
        element.into_any_element()
    }

    fn render_actions(
        &self,
        id: &ClusterId,
        actions: [Action; 3],
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name: SharedString = self
            .cluster(id)
            .map_or_else(|| id.to_string(), |cluster| cluster.name.clone())
            .into();
        let hover = palette.hover;
        let back = div()
            .debug_selector(|| "switcher-back".to_owned())
            .id("switcher-back")
            .flex()
            .items_center()
            .gap(SPACE_4)
            .h(ROW_CLUSTER)
            .px(SPACE_4)
            .rounded(RADIUS)
            .cursor_pointer()
            .font_weight(FontWeight::MEDIUM)
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(|this, _, _, cx| this.back(cx)))
            .child(svg().path(CHEVRON_LEFT).size(ICON).text_color(palette.mute))
            .child(div().flex_1().truncate().child(name));
        let items: Vec<AnyElement> = actions
            .into_iter()
            .enumerate()
            .map(|(ix, action)| self.render_action(ix, action, palette, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .child(back)
            .child(rule(palette))
            .children(items)
            .into_any_element()
    }

    fn render_action(
        &self,
        ix: usize,
        action: Action,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = self.highlighted == Some(ix);
        let color = if action == Action::Remove {
            palette.err
        } else {
            palette.fg
        };
        div()
            .debug_selector(move || format!("switcher-action-{ix}"))
            .id(ElementId::NamedInteger("switcher-action".into(), ix as u64))
            .flex()
            .flex_col()
            .gap(SPACE_1 / 2.)
            .px(SPACE_4)
            .py(ROW_MENU_PAD_Y)
            .rounded(RADIUS)
            .cursor_pointer()
            .when(highlighted, |row| row.bg(palette.hover))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.highlighted = Some(ix);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.activate(ix, window, cx)))
            .child(
                div()
                    .text_size(TEXT_BODY)
                    .text_color(color)
                    .child(action.label()),
            )
            .child(
                div()
                    .text_size(TEXT_SMALL)
                    .line_height(relative(DESCRIPTION_LINE))
                    .text_color(palette.mute)
                    .child(action.description()),
            )
            .into_any_element()
    }
}

fn glyph_cell(glyph: SharedString, color: Hsla) -> impl IntoElement {
    div()
        .w(GLYPH_SLOT)
        .flex_none()
        .font_family(FONT_STATUS)
        .text_size(TEXT_GLYPH)
        .text_color(color)
        .child(glyph)
}

fn rule(palette: &Palette) -> impl IntoElement {
    div()
        .flex_none()
        .h(RULE)
        .mx(SPACE_1)
        .my(SPACE_2)
        .bg(palette.line)
}

fn cluster_meta(cluster: &ClusterSummary, palette: &Palette) -> Option<AnyElement> {
    let badges: Vec<AnyElement> = cluster
        .problems
        .badges()
        .into_iter()
        .map(|(status, n)| {
            div()
                .text_color(palette.status(status))
                .child(status_symbols(format!("{}{n}", status.glyph())))
                .into_any_element()
        })
        .collect();
    let meta = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(SPACE_2)
        .font_family(FONT_MONO)
        .text_size(TEXT_LABEL)
        .font_weight(FontWeight::NORMAL);
    if !badges.is_empty() {
        return Some(meta.children(badges).into_any_element());
    }
    if cluster.status == Status::Ok {
        return None;
    }
    let state = div()
        .text_color(palette.status(cluster.status))
        .child(cluster.state.clone());
    Some(meta.child(state).into_any_element())
}

fn gear_button(
    ix: usize,
    id: ClusterId,
    shown: bool,
    palette: &Palette,
    cx: &mut Context<ClusterSwitcher>,
) -> AnyElement {
    let hover = palette.line;
    div()
        .debug_selector(move || format!("switcher-gear-{ix}"))
        .id(ElementId::NamedInteger("switcher-gear".into(), ix as u64))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(SWITCHER_GEAR)
        .h(crate::theme::ICON_BUTTON)
        .rounded(RADIUS)
        .when(!shown, Styled::invisible)
        .hover(move |style| style.bg(hover))
        .tooltip(Tooltip::text(GEAR_TOOLTIP))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.open_settings(id.clone(), window, cx);
        }))
        .child(svg().path(GEAR).size(ICON).text_color(palette.mute))
        .into_any_element()
}

impl Render for ClusterSwitcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let trigger = self.render_trigger(&palette, cx);
        let popover = self.open.then(|| {
            let body = match self.actions() {
                Some((id, actions)) => self.render_actions(&id, actions, &palette, cx),
                None => self.render_rows(&palette, cx),
            };
            let panel = surface("switcher", &palette)
                .w(SWITCHER_WIDTH)
                .overflow_hidden()
                .font_family(FONT_SANS)
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key_down))
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.dismissal.clicked_outside();
                    this.close(window, cx);
                }))
                .child(body);
            let key = ElementId::NamedInteger(ID.into(), self.generation);
            above_right(appear(panel, key))
        });
        div()
            .id(ElementId::NamedInteger(ID.into(), 0))
            .relative()
            .flex_none()
            .h_full()
            .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                this.dismissal.hover(*hovered);
            }))
            .child(trigger)
            .children(popover)
    }
}

impl Shell {
    pub(super) fn switcher_event(
        &mut self,
        event: &SwitcherEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SwitcherEvent::Select(target) => self.retarget(target.as_ref(), cx),
            SwitcherEvent::AddCluster => {
                self.open_add_cluster(flock_core::AddClusterStep::Pick, window, cx);
            }
            SwitcherEvent::Run(command) => self.dispatch(command.clone(), cx),
            SwitcherEvent::Remove(id) => Self::confirm_remove(id.clone(), window, cx),
        }
    }

    fn confirm_remove(id: ClusterId, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let run: Run = Rc::new(move |command, cx| {
            shell
                .update(cx, |shell, cx| shell.dispatch(command, cx))
                .ok();
        });
        confirm(remove_dialog(id, run), window, cx);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use flock_core::{Counts, Status};

    use super::*;

    fn cluster(id: &str, status: Status) -> ClusterSummary {
        ClusterSummary {
            id: ClusterId::new(id),
            name: id.to_owned(),
            status,
            state: String::new(),
            distro: String::new(),
            region: String::new(),
            version: None,
            nodes: None,
            problems: Counts::default(),
            nav: BTreeMap::new(),
            metrics: None,
        }
    }

    #[test]
    fn connected_clusters_offer_disconnect_and_offline_ones_connect() {
        let live = Action::for_cluster(&cluster("a", Status::Warn));
        let offline = Action::for_cluster(&cluster("b", Status::Offline));
        assert_eq!(live, [Action::Settings, Action::Disconnect, Action::Remove]);
        assert_eq!(offline, [Action::Settings, Action::Connect, Action::Remove]);
    }

    #[test]
    fn actions_become_shell_events() {
        let id = ClusterId::new("a");
        assert_eq!(
            Action::Settings.event(&id),
            SwitcherEvent::Run(Command::ShowClusterSettings(id.clone()))
        );
        assert_eq!(
            Action::Disconnect.event(&id),
            SwitcherEvent::Run(Command::Disconnect(id.clone()))
        );
        assert_eq!(
            Action::Connect.event(&id),
            SwitcherEvent::Run(Command::Connect(id.clone()))
        );
        assert_eq!(Action::Remove.event(&id), SwitcherEvent::Remove(id));
    }

    #[test]
    fn rules_separate_all_clusters_the_clusters_and_add_cluster() {
        let clusters = [cluster("a", Status::Ok), cluster("b", Status::Ok)];
        let rows = switcher::rows(&clusters);
        let starts: Vec<bool> = (0..rows.len()).map(|ix| starts_group(&rows, ix)).collect();
        assert_eq!(starts, [false, true, false, true]);
        assert_eq!(child_index(&rows, 0), 0);
        assert_eq!(child_index(&rows, 1), 2);
        assert_eq!(child_index(&rows, 3), 5);
    }
}
