use fleet_core::settings::ClusterAction;
use fleet_core::switcher::{self, ADD_CLUSTER, ALL_CLUSTERS, Entry, Row};
use fleet_core::{ClusterId, ClusterSummary, Command};
use gpui::{
    AnyElement, App, AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle, FontWeight,
    Hsla, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, relative, svg,
};

use super::Shell;
use crate::assets::{CHEVRON_LEFT, CHEVRON_UP};
use crate::kit::list_keys::Nav;
use crate::kit::{
    ConfirmDialog, Dismissal, Highlighted, InputEvent, Step, TextField, TextFieldDown,
    TextFieldEnd, TextFieldEscape, TextFieldHome, TextFieldPageDown, TextFieldPageUp,
    TextFieldState, TextFieldUp, Tooltip, above_left, appear, confirm, status_symbols, step,
    surface,
};
use crate::screens::settings::{DISCONNECT, DISCONNECT_DESCRIPTION};
use crate::theme::{
    ActiveTheme, CONTROL, FONT_MONO, FONT_STATUS, GLYPH_SLOT, ICON, ICON_BUTTON, Palette, RADIUS,
    ROW_CLUSTER, ROW_MENU_PAD_Y, RULE, SPACE_1, SPACE_2, SPACE_3, SPACE_4, SPACE_5, SPACE_6,
    SWITCHER_LIST, SWITCHER_WIDTH, TEXT_BODY, TEXT_GLYPH, TEXT_LABEL, TEXT_SMALL, TEXT_UI,
};

pub const SWITCH_KEY: &str = "⌘⇧C";
const ID: &str = "cluster-switcher";
const PLACEHOLDER: &str = "Switch cluster";
const TOOLTIP: &str = "Switch cluster";
const ALL_CLUSTERS_GLYPH: &str = "◎";
const ADD_GLYPH: &str = "+";
const MORE: &str = "⋯";
const MORE_TOOLTIP: &str = "Cluster actions";
const ACTIONS_KEY: &str = ".";
const NO_MATCHES: &str = "No clusters match.";
const FOOTER: &[&str] = &["↑↓ move", "↵ switch", "⌘. actions", "esc"];
const DESCRIPTION_LINE: f32 = 1.35;
const SETTINGS: &str = "Settings…";
const SETTINGS_DESCRIPTION: &str =
    "Connection details, default namespace, metrics source and labels.";
const CONNECT_DESCRIPTION: &str = "Connect again and resume watching this cluster.";
const REMOVE: &str = "Remove from Fleet…";
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

fn starts_group(rows: &[Row], ix: usize) -> bool {
    let Some(row) = rows.get(ix) else {
        return false;
    };
    ix.checked_sub(1)
        .and_then(|before| rows.get(before))
        .is_some_and(|previous| {
            std::mem::discriminant(&previous.entry) != std::mem::discriminant(&row.entry)
        })
}

fn child_index(rows: &[Row], ix: usize) -> usize {
    let rules = (0..=ix).filter(|&at| starts_group(rows, at)).count();
    ix + rules
}

pub struct ClusterSwitcher {
    clusters: Vec<ClusterSummary>,
    selected: Option<ClusterId>,
    query: Entity<TextFieldState>,
    rows: Vec<Row>,
    highlighted: Option<usize>,
    actions_for: Option<ClusterId>,
    open: bool,
    generation: u64,
    dismissal: Dismissal,
    restore: Option<FocusHandle>,
    scroll: ScrollHandle,
    _query_events: Subscription,
}

impl EventEmitter<SwitcherEvent> for ClusterSwitcher {}

impl ClusterSwitcher {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| TextFieldState::new(window, cx).placeholder(PLACEHOLDER));
        let query_events = cx.subscribe_in(
            &query,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.actions_for = None;
                    this.refresh(cx);
                }
                InputEvent::PressEnter { .. } => this.activate_highlighted(window, cx),
                InputEvent::Focus => {}
                InputEvent::Blur => {}
            },
        );
        Self {
            clusters: Vec::new(),
            selected: None,
            query,
            rows: Vec::new(),
            highlighted: None,
            actions_for: None,
            open: false,
            generation: 0,
            dismissal: Dismissal::default(),
            restore: None,
            scroll: ScrollHandle::new(),
            _query_events: query_events,
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
            .filter(|_| !moved)
            .and_then(|ix| self.rows.get(ix))
            .map(|row| row.entry.clone());
        let query = self.query.read(cx).value();
        self.rows = switcher::rows(&self.clusters, &query);
        if self.actions_for.is_none() {
            let position = kept.and_then(|entry| self.rows.iter().position(|r| r.entry == entry));
            self.highlighted = position.or_else(|| self.initial(&query));
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
        self.actions_for = None;
        self.query.update(cx, |query, cx| {
            query.set_value(String::new(), window, cx);
            query.focus(window, cx);
        });
        self.refresh(cx);
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

    fn initial(&self, query: &str) -> Option<usize> {
        switcher::initial_highlight(&self.rows, self.selected.as_ref(), query)
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value();
        self.rows = switcher::rows(&self.clusters, &query);
        self.highlighted = self.initial(&query);
        self.scroll_to_highlight();
        cx.notify();
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

    fn move_highlight(&mut self, direction: Step, cx: &mut Context<Self>) {
        let len = match self.actions() {
            Some((_, actions)) => actions.len(),
            None => self.rows.len(),
        };
        self.highlighted = step(self.highlighted, len, |_| true, direction);
        self.scroll_to_highlight();
        cx.notify();
    }

    fn jump(&mut self, nav: Nav, cx: &mut Context<Self>) {
        let len = match self.actions() {
            Some((_, actions)) => actions.len(),
            None => self.rows.len(),
        };
        self.highlighted = nav.target(self.highlighted, len);
        self.scroll_to_highlight();
        cx.notify();
    }

    fn jump_if_empty(&mut self, nav: Nav, cx: &mut Context<Self>) {
        if self.query.read(cx).value().is_empty() {
            self.jump(nav, cx);
            cx.stop_propagation();
        }
    }

    fn activate_highlighted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.highlighted {
            self.activate(ix, window, cx);
        }
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let event = match self.actions() {
            Some((id, actions)) => actions.get(ix).map(|action| action.event(&id)),
            None => self.rows.get(ix).map(|row| match &row.entry {
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

    fn show_actions(&mut self, id: ClusterId, cx: &mut Context<Self>) {
        self.actions_for = Some(id);
        self.highlighted = Some(0);
        cx.notify();
    }

    fn show_actions_for_highlighted(&mut self, cx: &mut Context<Self>) {
        let entry = self
            .highlighted
            .and_then(|ix| self.rows.get(ix))
            .map(|row| row.entry.clone());
        if let Some(Entry::Cluster(id)) = entry {
            self.show_actions(id, cx);
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.actions_for.take() else {
            return;
        };
        let entry = Entry::Cluster(id);
        self.highlighted = self.rows.iter().position(|row| row.entry == entry);
        cx.notify();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions_for.is_some() {
            self.back(cx);
        } else {
            self.close(window, cx);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform && keystroke.key == ACTIONS_KEY {
            self.show_actions_for_highlighted(cx);
            cx.stop_propagation();
        }
    }

    fn render_trigger(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.selected.as_ref().and_then(|id| self.cluster(id));
        let (glyph, color, name): (SharedString, Hsla, SharedString) = match selected {
            Some(cluster) => (
                cluster.status.glyph().into(),
                palette.status(cluster.status),
                cluster.name.clone().into(),
            ),
            None => (ALL_CLUSTERS_GLYPH.into(), palette.fg2, ALL_CLUSTERS.into()),
        };
        let hover = palette.hover;
        div()
            .debug_selector(|| ID.to_owned())
            .id(ID)
            .flex()
            .items_center()
            .gap(SPACE_3)
            .h_full()
            .px(SPACE_4)
            .cursor_pointer()
            .text_color(palette.fg2)
            .when(self.open, |trigger| {
                trigger.bg(palette.sel).text_color(palette.selfg)
            })
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
            .child(glyph_cell(glyph, color))
            .child(name)
            .child(svg().path(CHEVRON_UP).size(ICON).text_color(palette.mute))
            .into_any_element()
    }

    fn render_query(&self, palette: &Palette) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(CONTROL + SPACE_4)
            .px(SPACE_5)
            .border_b(RULE)
            .border_color(palette.line)
            .child(TextField::bare(&self.query, TEXT_UI))
    }

    fn render_rows(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut children: Vec<AnyElement> = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            if starts_group(&self.rows, ix) {
                children.push(rule(palette).into_any_element());
            }
            children.push(self.render_row(ix, row, palette, cx));
        }
        if children.is_empty() {
            children.push(
                div()
                    .px(SPACE_4)
                    .py(SPACE_3)
                    .text_size(TEXT_BODY)
                    .text_color(palette.mute)
                    .child(NO_MATCHES)
                    .into_any_element(),
            );
        }
        div()
            .id("switcher-list")
            .flex()
            .flex_col()
            .p(SPACE_2)
            .max_h(SWITCHER_LIST)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(children)
            .into_any_element()
    }

    fn render_row(
        &self,
        ix: usize,
        row: &Row,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = self.highlighted == Some(ix);
        let current = match &row.entry {
            Entry::AllClusters => self.selected.is_none(),
            Entry::Cluster(id) => self.selected.as_ref() == Some(id),
            Entry::AddCluster => false,
        };
        let group_name: SharedString = format!("switcher-row-{ix}").into();
        let base = div()
            .debug_selector(|| format!("switcher-row-{ix}"))
            .id(ElementId::NamedInteger("switcher-row".into(), ix as u64))
            .group(group_name.clone())
            .flex()
            .flex_none()
            .items_center()
            .gap(SPACE_4)
            .h(ROW_CLUSTER)
            .px(SPACE_4)
            .rounded(RADIUS)
            .cursor_pointer()
            .text_size(TEXT_UI)
            .text_color(palette.fg2)
            .when(highlighted, |row| row.bg(palette.hover))
            .when(current, |row| {
                row.text_color(palette.fg).font_weight(FontWeight::MEDIUM)
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.actions_for.is_none() {
                    this.highlighted = Some(ix);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.activate(ix, window, cx)));
        let label = |text: SharedString| {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(Highlighted::new(text, row.highlights.clone()))
        };
        let element = match &row.entry {
            Entry::AllClusters => base
                .child(glyph_cell(ALL_CLUSTERS_GLYPH.into(), palette.fg2))
                .child(label(ALL_CLUSTERS.into())),
            Entry::AddCluster => base
                .text_color(palette.mute)
                .child(glyph_cell(ADD_GLYPH.into(), palette.mute))
                .child(label(ADD_CLUSTER.into())),
            Entry::Cluster(id) => match self.cluster(id) {
                Some(cluster) => {
                    let more = more_button(ix, id.clone(), highlighted, group_name, palette, cx);
                    base.when(!cluster.is_connected() && !current, |row| {
                        row.text_color(palette.mute)
                    })
                    .child(glyph_cell(
                        cluster.status.glyph().into(),
                        palette.status(cluster.status),
                    ))
                    .child(label(cluster.name.clone().into()))
                    .child(cluster_meta(cluster, palette))
                    .child(more)
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
            .p(SPACE_2)
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
            .debug_selector(|| format!("switcher-action-{ix}"))
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

fn cluster_meta(cluster: &ClusterSummary, palette: &Palette) -> impl IntoElement {
    let badges = cluster.problems.badges().into_iter().map(|(status, n)| {
        div()
            .text_color(palette.status(status))
            .child(status_symbols(format!("{} {n}", status.glyph())))
    });
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(SPACE_4)
        .font_family(FONT_MONO)
        .text_size(TEXT_SMALL)
        .font_weight(FontWeight::NORMAL)
        .child(div().text_color(palette.mute).child(cluster.state.clone()))
        .children(badges)
}

fn more_button(
    ix: usize,
    id: ClusterId,
    highlighted: bool,
    group_name: SharedString,
    palette: &Palette,
    cx: &mut Context<ClusterSwitcher>,
) -> impl IntoElement {
    let hover = palette.line;
    div()
        .debug_selector(|| format!("switcher-more-{ix}"))
        .id(ElementId::NamedInteger("switcher-more".into(), ix as u64))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(ICON_BUTTON)
        .rounded(RADIUS)
        .text_color(palette.fg2)
        .when(!highlighted, |button| {
            button.invisible().group_hover(group_name, Styled::visible)
        })
        .hover(move |style| style.bg(hover))
        .tooltip(Tooltip::text(MORE_TOOLTIP))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.show_actions(id.clone(), cx);
        }))
        .child(MORE)
}

fn footer(palette: &Palette) -> impl IntoElement {
    div()
        .flex()
        .gap(SPACE_6)
        .px(SPACE_5)
        .py(SPACE_3)
        .border_t(RULE)
        .border_color(palette.line)
        .bg(palette.chrome)
        .font_family(FONT_MONO)
        .text_size(TEXT_LABEL)
        .text_color(palette.mute)
        .children(FOOTER.iter().map(|hint| div().child(*hint)))
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
                .p_0()
                .w(SWITCHER_WIDTH)
                .overflow_hidden()
                .font_family(crate::theme::FONT_SANS)
                .capture_action(cx.listener(|this, _: &TextFieldUp, _, cx| {
                    this.move_highlight(Step::Previous, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|this, _: &TextFieldDown, _, cx| {
                    this.move_highlight(Step::Next, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|this, _: &TextFieldPageUp, _, cx| {
                    this.jump(Nav::PageUp, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|this, _: &TextFieldPageDown, _, cx| {
                    this.jump(Nav::PageDown, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|this, _: &TextFieldHome, _, cx| {
                    this.jump_if_empty(Nav::Home, cx);
                }))
                .capture_action(cx.listener(|this, _: &TextFieldEnd, _, cx| {
                    this.jump_if_empty(Nav::End, cx);
                }))
                .on_action(cx.listener(|this, _: &TextFieldEscape, window, cx| {
                    this.escape(window, cx);
                }))
                .capture_key_down(cx.listener(Self::key_down))
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.dismissal.clicked_outside();
                    this.close(window, cx);
                }))
                .child(self.render_query(&palette))
                .child(body)
                .child(footer(&palette));
            let key = ElementId::NamedInteger(ID.into(), self.generation);
            above_left(appear(panel, key))
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
            SwitcherEvent::Select(target) => {
                self.dispatch(Command::SelectCluster(target.clone()), cx);
            }
            SwitcherEvent::AddCluster => {
                self.open_add_cluster(fleet_core::AddClusterStep::Pick, window, cx);
            }
            SwitcherEvent::Run(command) => self.dispatch(command.clone(), cx),
            SwitcherEvent::Remove(id) => Self::confirm_remove(id.clone(), window, cx),
        }
    }

    fn confirm_remove(id: ClusterId, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let title = format!("Remove {id}");
        let body = format!(
            "Fleet disconnects from {id} and stops watching it. Your kubeconfig file is not \
             changed, so you can add the context again from Add cluster."
        );
        let removed = id;
        let dialog = ConfirmDialog::new(title, body, REMOVE_CONFIRM, move |_, cx: &mut App| {
            let command = Command::RemoveCluster(removed.clone());
            shell
                .update(cx, |shell, cx| shell.dispatch(command, cx))
                .ok();
        });
        confirm(dialog, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fleet_core::{Counts, Status};

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
        let rows = switcher::rows(&clusters, "");
        let starts: Vec<bool> = (0..rows.len()).map(|ix| starts_group(&rows, ix)).collect();
        assert_eq!(starts, [false, true, false, true]);
        assert_eq!(child_index(&rows, 0), 0);
        assert_eq!(child_index(&rows, 1), 2);
        assert_eq!(child_index(&rows, 3), 5);
    }
}
