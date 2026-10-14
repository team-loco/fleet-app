mod keys;
mod load;
mod page;
mod sidebar;
mod status_bar;
pub(crate) mod switcher;
mod tabs;

use fleet_core::palette::{COMPARE_KEY, LOGS_KEY, OWNER_KEY, resource_commands};
use fleet_core::resource::short_kind;
use fleet_core::{AddClusterStep, AppState, Command, Page, Sender, Status, Update};
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Render, Styled, Subscription, Task,
    Window, div,
};

use crate::Store;
use crate::actions::{
    CompareAcrossClusters, FocusFilter, FollowLogs, OpenSettings, ShowFluxOwner,
    ToggleClusterSwitcher, ToggleCommandPalette, ToggleSidebar,
};
use crate::kit::{FilterBar, Toast, toast};
use crate::screens::add_cluster::{AddClusterDialog, AddClusterEvent};
use crate::screens::app_settings::AppSettingsScreen;
use crate::screens::changes::ChangesScreen;
use crate::screens::command::{CommandPalette, PaletteEvent};
use crate::screens::compare::CompareScreen;
use crate::screens::fleet::FleetScreen;
use crate::screens::flux_object::FluxObjectPage;
use crate::screens::gitops::GitOpsScreen;
use crate::screens::logs::LogsScreen;
use crate::screens::pods::PodsScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::workload::WorkloadScreen;
use crate::screens::{ClusterPages, ScreenEvent};
use crate::theme::{self, ActiveTheme, TEXT_UI};
use switcher::{ClusterSwitcher, SwitcherEvent};

pub(crate) const KEY_CONTEXT: &str = "Shell";

pub struct NewWindowRequest(pub Vec<Command>);

impl gpui::EventEmitter<NewWindowRequest> for Shell {}

pub struct Shell {
    state: AppState,
    keys: keys::Keyboard,
    window_id: u64,
    commands: Sender<Command>,
    pending: Vec<Command>,
    new_tab_gesture: bool,
    announce: bool,
    tab_strip: tabs::TabStrip,
    sidebar_generation: usize,
    focus: FocusHandle,
    fleet: Entity<FleetScreen>,
    palette: Entity<CommandPalette>,
    changes: Entity<ChangesScreen>,
    pages: ClusterPages,
    loaded: bool,
    load: load::LoadClock,
    add_cluster: Option<(Entity<AddClusterDialog>, Subscription)>,
    switcher: Entity<ClusterSwitcher>,
    _switcher_events: Subscription,
    workload: Option<(Entity<WorkloadScreen>, Subscription)>,
    logs: Option<(Entity<LogsScreen>, Subscription)>,
    flux_object: Entity<FluxObjectPage>,
    #[cfg(feature = "loco")]
    loco: Entity<crate::screens::loco::LocoScreens>,
    #[cfg(feature = "loco")]
    _loco_commands: Subscription,
    compare: Entity<CompareScreen>,
    settings: Entity<SettingsScreen>,
    app_settings: Entity<AppSettingsScreen>,
    _updates: Task<()>,
    _store: Entity<Store>,
    _window_closed: Subscription,
    _appearance: Subscription,
    _fleet_commands: Subscription,
    _palette_events: Subscription,
    gitops: Entity<GitOpsScreen>,
    _gitops_commands: Subscription,
    pods: Entity<PodsScreen>,
    _pod_commands: Subscription,
    _flux_object: Subscription,
    _screen_commands: [Subscription; 3],
}

impl Shell {
    pub fn new(
        store: Entity<Store>,
        initial: Vec<Command>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let commands = store.read(cx).commands();
        let updates = store.update(cx, |store, _| store.subscribe());
        let window_id = cx.entity_id().as_u64();
        let close_commands = commands.clone();
        let window_closed = Self::watch_close(window, window_id, close_commands, cx);
        let listen = cx.spawn_in(window, async move |this, cx| {
            while let Ok(update) = updates.recv().await {
                let applied = this.update_in(cx, |shell, window, cx| {
                    shell.receive(update, window, cx);
                });
                if applied.is_err() {
                    break;
                }
            }
        });
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            theme::follow_window(window, cx);
            cx.notify();
        });
        let fleet = cx.new(FleetScreen::new);
        let fleet_commands = cx.subscribe(&fleet, |this, _, command: &Command, cx| {
            this.dispatch(command.clone(), cx);
        });
        let changes = cx.new(|cx| ChangesScreen::new(window, cx));
        let gitops = cx.new(|cx| GitOpsScreen::new(window, cx));
        let gitops_commands = cx.subscribe(&gitops, |this, _, command: &Command, cx| {
            this.dispatch(command.clone(), cx);
        });
        #[cfg(feature = "loco")]
        let loco = cx.new(|cx| crate::screens::loco::LocoScreens::new(window, cx));
        #[cfg(feature = "loco")]
        let loco_commands = cx.subscribe(&loco, |this, _, command: &Command, cx| {
            this.dispatch(command.clone(), cx);
        });
        let compare = cx.new(CompareScreen::new);
        let settings = cx.new(|cx| SettingsScreen::new(window, cx));
        let app_settings = cx.new(AppSettingsScreen::new);
        let screen_commands = [
            Self::forward(&app_settings, cx),
            Self::forward(&compare, cx),
            Self::forward(&settings, cx),
        ];
        let pods = cx.new(|cx| PodsScreen::new(window, cx));
        let pod_commands = cx.subscribe(&pods, |this, _, command: &Command, cx| {
            this.dispatch(command.clone(), cx);
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        theme::follow_window(window, cx);
        let (palette, palette_events) = Self::new_palette(window, cx);
        let (switcher, switcher_events) = Self::new_switcher(window, cx);
        let pages = ClusterPages::new(window, cx, Self::screen_event);
        let flux_object = cx.new(FluxObjectPage::new);
        let flux_commands = cx.subscribe(&flux_object, |shell, _, command: &Command, cx| {
            shell.dispatch(command.clone(), cx);
        });
        Self {
            state: AppState::default(),
            keys: keys::Keyboard::new(cx),
            window_id,
            commands,
            pending: initial,
            new_tab_gesture: false,
            announce: false,
            tab_strip: tabs::TabStrip::default(),
            sidebar_generation: 0,
            focus,
            fleet,
            palette,
            changes,
            pages,
            loaded: false,
            load: load::LoadClock::default(),
            add_cluster: None,
            switcher,
            _switcher_events: switcher_events,
            workload: None,
            logs: None,
            flux_object,
            #[cfg(feature = "loco")]
            loco,
            #[cfg(feature = "loco")]
            _loco_commands: loco_commands,
            compare,
            settings,
            app_settings,
            _updates: listen,
            _store: store,
            _window_closed: window_closed,
            _appearance: appearance,
            _fleet_commands: fleet_commands,
            _palette_events: palette_events,
            gitops,
            _gitops_commands: gitops_commands,
            pods,
            _pod_commands: pod_commands,
            _flux_object: flux_commands,
            _screen_commands: screen_commands,
        }
    }

    fn forward<T: gpui::EventEmitter<Command>>(
        screen: &Entity<T>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe(screen, |this, _, command: &Command, cx| {
            this.dispatch(command.clone(), cx);
        })
    }

    fn new_palette(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<CommandPalette>, Subscription) {
        let palette = cx.new(|cx| CommandPalette::new(window, cx));
        let events = cx.subscribe_in(&palette, window, |this, palette, event, window, cx| {
            this.palette_event(palette, event, window, cx);
        });
        (palette, events)
    }

    fn new_switcher(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<ClusterSwitcher>, Subscription) {
        let switcher = cx.new(|cx| ClusterSwitcher::new(window, cx));
        let events = cx.subscribe_in(
            &switcher,
            window,
            |this, _, event: &SwitcherEvent, window, cx| this.switcher_event(event, window, cx),
        );
        (switcher, events)
    }

    fn watch_close(
        window: &Window,
        window_id: u64,
        commands: Sender<Command>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let handle = window.window_handle();
        cx.on_window_closed(move |_, closed| {
            if closed == handle.window_id() {
                let _ = commands.try_send(Command::WindowClosed(window_id));
            }
        })
    }

    fn palette_event(
        &mut self,
        palette: &Entity<CommandPalette>,
        event: &PaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PaletteEvent::Run(command) => {
                palette.update(cx, |palette, cx| palette.close(window, cx));
                self.dispatch(command.clone(), cx);
            }
            PaletteEvent::NewTab(command) => {
                if self.state.window_commands(command).is_some() {
                    palette.update(cx, |palette, cx| palette.close(window, cx));
                    self.dispatch(Command::NewTab(Box::new(command.clone())), cx);
                }
            }
            PaletteEvent::NewWindow(command) => {
                if let Some(commands) = self.state.window_commands(command) {
                    palette.update(cx, |palette, cx| palette.close(window, cx));
                    cx.emit(NewWindowRequest(commands));
                }
            }
        }
    }

    fn receive(&mut self, update: Update, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((dialog, _)) = &self.add_cluster {
            dialog.update(cx, |dialog, cx| dialog.receive(&update, window, cx));
        }
        if let Update::Snapshot(_) = update {
            self.loaded = true;
        }
        self.state.update(update);
        self.run_pending(window, cx);
        if self.loaded && std::mem::take(&mut self.announce) {
            self.show_active(cx);
        }
        let snapshot = &self.state.snapshot;
        self.fleet
            .update(cx, |fleet, cx| fleet.set_snapshot(snapshot, cx));
        self.changes
            .update(cx, |changes, cx| changes.set_snapshot(snapshot, cx));
        self.gitops
            .update(cx, |gitops, cx| gitops.set_snapshot(snapshot, cx));
        self.settings
            .update(cx, |settings, cx| settings.set_snapshot(snapshot, cx));
        self.pods
            .update(cx, |pods, cx| pods.set_snapshot(snapshot, cx));
        self.sync_page(cx);
        self.track_load(cx);
        self.sync_palette(cx);
        cx.notify();
    }

    fn run_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ready = self
            .pending
            .iter()
            .take_while(|command| self.state.ready_for(command))
            .count();
        let pending: Vec<Command> = self.pending.drain(..ready).collect();
        for command in pending {
            if let Command::OpenAddCluster(step) = command {
                self.open_add_cluster(step, window, cx);
            } else {
                self.dispatch(command, cx);
            }
        }
    }

    pub fn restore_tabs(&mut self, saved: &fleet_core::tabs::SavedWindow, cx: &mut Context<Self>) {
        self.state.restore(saved);
        self.announce = true;
        cx.notify();
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn settled(&self) -> bool {
        self.loaded && self.pending.is_empty()
    }

    pub fn open_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_palette(cx);
        self.palette
            .update(cx, |palette, cx| palette.open(query, window, cx));
    }

    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.read(cx).is_open() {
            self.palette
                .update(cx, |palette, cx| palette.close(window, cx));
        } else {
            self.open_palette("", window, cx);
        }
    }

    fn sync_palette(&self, cx: &mut Context<Self>) {
        let snapshot = &self.state.snapshot;
        let current = self.state.location().cluster.as_ref();
        self.palette.update(cx, |palette, cx| {
            palette.set_context(snapshot, current, cx);
        });
        self.switcher.update(cx, |switcher, cx| {
            switcher.set_context(&snapshot.clusters, current, cx);
        });
    }

    pub fn switcher_open(&self, cx: &App) -> bool {
        self.switcher.read(cx).is_open()
    }

    pub fn open_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.switcher
            .update(cx, |switcher, cx| switcher.show(window, cx));
    }

    fn cluster_preferences(&self) -> fleet_core::Preferences {
        self.selected_cluster()
            .and_then(|cluster| self.state.snapshot.catalog.get(&cluster.id))
            .map(|settings| settings.preferences.clone())
            .unwrap_or_default()
    }

    fn sync_pages(&self, cx: &mut Context<Self>) {
        let cluster = self.selected_cluster();
        let confirm_destructive = cluster
            .and_then(|c| self.state.snapshot.catalog.get(&c.id))
            .is_none_or(|settings| settings.preferences.confirm_destructive);
        let preferences = self.cluster_preferences();
        self.pages.sync(
            self.state.location().page,
            cluster,
            &self.state.snapshot,
            confirm_destructive,
            &preferences,
            cx,
        );
    }

    fn screen_event(&mut self, event: &ScreenEvent, cx: &mut Context<Self>) {
        match event {
            ScreenEvent::Command(command) => self.dispatch(command.clone(), cx),
            ScreenEvent::Navigate(page) => self.dispatch(Command::SelectPage(*page), cx),
        }
    }

    fn send(&self, command: Command) {
        if let Err(error) = self
            .commands
            .try_send(Command::ForWindow(self.window_id, Box::new(command)))
        {
            tracing::warn!(%error, "backend stopped accepting commands");
        }
    }

    pub(super) fn open_add_cluster(
        &mut self,
        step: AddClusterStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = self.state.snapshot.clusters.iter().map(|c| c.name.clone());
        let existing: Vec<String> = existing.collect();
        let dialog = cx.new(|cx| AddClusterDialog::new(step, existing, window, cx));
        let subscription = cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &AddClusterEvent, window, cx| match event {
                AddClusterEvent::Send(command) => this.send(command.clone()),
                AddClusterEvent::Close => {
                    this.add_cluster = None;
                    window.focus(&this.focus, cx);
                    cx.notify();
                }
            },
        );
        self.add_cluster = Some((dialog, subscription));
        self.send(Command::OpenAddCluster(step));
        cx.notify();
    }

    fn dispatch(&mut self, command: Command, cx: &mut Context<Self>) {
        let command = self.with_gesture(command);
        let before = self.state.location().clone();
        if let Command::NewTab(inner) = &command {
            self.ui_effects(inner, cx);
            self.state.apply(&command);
            self.show_active(cx);
            return;
        }
        self.ui_effects(&command, cx);
        self.state.apply(&command);
        if let Command::OpenResource(target) = &command {
            self.pages.reveal(target, cx);
        }
        self.keys.refocus |= self.state.location() != &before;
        self.sync_page(cx);
        self.track_load(cx);
        self.send(command);
        self.sync_palette(cx);
        cx.notify();
    }

    fn with_gesture(&mut self, command: Command) -> Command {
        let navigates = self.state.window_commands(&command).is_some();
        let fresh = !matches!(command, Command::NewTab(_));
        if navigates && fresh && std::mem::take(&mut self.new_tab_gesture) {
            return Command::NewTab(Box::new(command));
        }
        command
    }

    pub(super) fn show_active(&mut self, cx: &mut Context<Self>) {
        let location = self.state.location().clone();
        for command in location.commands() {
            self.send(command);
        }
        if let Some(resource) = &location.resource {
            self.pages.reveal(resource, cx);
        }
        self.keys.refocus = true;
        self.sync_page(cx);
        self.track_load(cx);
        self.sync_palette(cx);
        cx.notify();
    }

    pub(super) fn close_tab(&mut self, id: fleet_core::tabs::TabId, cx: &mut Context<Self>) {
        let active = self.state.panes().active().id();
        if self.state.panes_mut().close(id) {
            if active == id {
                self.show_active(cx);
            } else {
                cx.notify();
            }
        }
    }

    fn ui_effects(&mut self, command: &Command, cx: &mut Context<Self>) {
        match command {
            Command::ForWindow(_, _) => {}
            Command::WindowClosed(_) => {}
            Command::ToggleSidebar => self.sidebar_generation += 1,
            Command::SelectCluster(_) => {}
            Command::SelectPage(_) => {}
            Command::OpenResource(_) => {}
            Command::InspectResource(_) => {}
            Command::FollowLogs(_) => {}
            Command::Compare(_) => {}
            Command::CompareWith { .. } => {}
            #[cfg(feature = "loco")]
            Command::Loco(_) => {}
            Command::CopySpec { .. } => {}
            Command::NewTab(_) => {}
            Command::OpenLogs { .. } => {}
            Command::ShowClusterSettings(cluster) => {
                self.settings
                    .update(cx, |settings, cx| settings.open(cluster, cx));
            }
            Command::SetPreferences(..) => {}
            Command::SetRefresh(_) => {}
            Command::Connect(_) => {}
            Command::Reauthenticate(_) => {}
            Command::ConfirmContext(_) => {}
            Command::Disconnect(_) => {}
            Command::PortForward {
                service,
                local,
                remote,
            } => {
                let title = format!(
                    "Forwarding localhost:{local} → {}/{}:{remote}",
                    short_kind(&service.kind),
                    service.name
                );
                let detail = format!(
                    "{} · {}",
                    service.cluster,
                    service.namespace.as_deref().unwrap_or_default()
                );
                toast(Toast::new(Status::Ok, title).detail(detail), cx);
            }
            Command::StopPortForward { .. } => {}
            Command::OpenCommit(sha) => {
                self.changes.update(cx, |changes, cx| changes.open(sha, cx));
            }
            Command::Cordon(_) => {}
            Command::Uncordon(_) => {}
            Command::Drain(_) => {}
            Command::Delete(_) => {}
            Command::OpenAddCluster(_) => {}
            Command::CheckContexts(_) => {}
            Command::AddClusters(_) => {}
            Command::SaveKubeconfig { .. } => {}
            Command::WatchFolder(_) => {}
            Command::RemoveCluster(_) => {}
            Command::WatchWorkload { .. } => {}
            Command::StreamLogs(_) => {}
            Command::Exec(_) => {}
            Command::RestartRollout(_) => {}
            Command::Scale { .. } => {}
            Command::Rollback { .. } => {}
            Command::OpenFluxObject(_) => {}
            Command::Flux(..) => {}
        }
    }

    fn run_shortcut(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(resource) = self.state.location().resource.as_ref() else {
            return;
        };
        let command = resource_commands(&self.state.snapshot, resource)
            .into_iter()
            .find(|hit| hit.shortcut == Some(key))
            .map(|hit| hit.command);
        if let Some(command) = command {
            self.dispatch(command, cx);
        }
    }

    fn sync_page(&self, cx: &mut Context<Self>) {
        let page = self.state.location().page;
        self.fleet.update(cx, |fleet, cx| fleet.set_page(page, cx));
        self.sync_pages(cx);
        let flux = self.state.location().flux_object.as_ref();
        let object = flux.and_then(|key| self.state.snapshot.flux_page(key));
        self.flux_object
            .update(cx, |screen, cx| screen.set_page(object, cx));
        #[cfg(feature = "loco")]
        self.loco
            .update(cx, |loco, cx| loco.set_context(&self.state, cx));
        let snapshot = &self.state.snapshot;
        let resource = self.state.location().resource.as_ref();
        self.compare.update(cx, |compare, cx| {
            compare.set_snapshot(snapshot, resource, cx);
        });
        let cluster = self.state.location().cluster.as_ref();
        let resource = self.state.location().resource.as_ref();
        let preferences = self.cluster_preferences();
        self.pods.update(cx, |pods, cx| {
            pods.set_context(cluster, resource, &preferences, cx);
        });
        if let Some((screen, _)) = &self.workload {
            self.sync_workload_yaml(screen, cx);
        }
    }

    pub(super) fn sync_workload_yaml(&self, screen: &Entity<WorkloadScreen>, cx: &mut App) {
        let target = screen.read(cx).target().clone();
        let yaml = self.state.snapshot.yaml(&target);
        screen.update(cx, |screen, cx| screen.set_yaml(yaml, cx));
    }

    pub fn filter_bar(&self, cx: &App) -> Option<Entity<FilterBar>> {
        if self.state.workload().is_some() || self.state.location().flux_object.is_some() {
            return None;
        }
        #[cfg(feature = "loco")]
        if crate::screens::loco::owns(self.state.location().page) {
            return self
                .loco
                .read(cx)
                .filter_bar(self.state.location().page, cx);
        }
        match self.state.location().page {
            Page::Pods => Some(self.pods.read(cx).filter_bar().clone()),
            Page::Changes => Some(self.changes.read(cx).filter_bar().clone()),
            Page::GitOps => Some(self.gitops.read(cx).filter_bar().clone()),
            page => self.pages.filter_bar(page, cx),
        }
    }

    fn focus_filter(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(bar) = self.filter_bar(cx) {
            bar.update(cx, |bar, cx| bar.focus(window, cx));
        }
    }

    pub fn show_pod_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pods
            .update(cx, |pods, cx| pods.show_terminal(window, cx));
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        self.keep_keyboard_focus(window, cx);
        let page = self.page(window, cx);
        let root = div()
            .id("shell")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &FocusFilter, window, cx| {
                this.focus_filter(window, cx);
            }))
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, _| {
                this.new_tab_gesture =
                    event.modifiers.platform || event.button == MouseButton::Middle;
            }))
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, _| {
                this.new_tab_gesture = false;
            }));
        Self::key_actions(root, cx)
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.dispatch(Command::SelectPage(Page::Settings), cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.dispatch(Command::ToggleSidebar, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleClusterSwitcher, window, cx| {
                this.switcher
                    .update(cx, |switcher, cx| switcher.toggle(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                this.toggle_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FollowLogs, _, cx| {
                this.run_shortcut(LOGS_KEY, cx);
            }))
            .on_action(cx.listener(|this, _: &CompareAcrossClusters, _, cx| {
                this.run_shortcut(COMPARE_KEY, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowFluxOwner, _, cx| {
                this.run_shortcut(OWNER_KEY, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.editor)
            .text_color(palette.fg)
            .font_family(theme::FONT_SANS)
            .text_size(TEXT_UI)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(window, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(self.tab_strip(cx))
                            .child(div().flex().flex_1().min_h_0().child(page)),
                    ),
            )
            .child(self.status_bar(cx))
            .child(self.palette.clone())
            .children(self.add_cluster.as_ref().map(|(dialog, _)| dialog.clone()))
            .child(crate::kit::overlay_layer(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Root, Store};
    use fleet_core::ClusterId;
    use gpui::{AnyWindowHandle, KeyBinding, TestAppContext, WindowOptions};

    fn open_window(
        store: &Entity<Store>,
        initial: Vec<Command>,
        cx: &mut App,
    ) -> (Entity<Shell>, AnyWindowHandle) {
        let mut shell = None;
        let opened = cx.open_window(WindowOptions::default(), |window, cx| {
            let view = cx.new(|cx| Shell::new(store.clone(), initial, window, cx));
            shell = Some(view.clone());
            cx.new(|cx| Root::new(view, window, cx))
        });
        let handle = opened.unwrap_or_else(|error| panic!("test window: {error}"));
        let shell = shell.unwrap_or_else(|| panic!("test shell missing"));
        (shell, handle.into())
    }

    #[gpui::test]
    fn palette_new_window_keeps_source_selection_and_shares_snapshots(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(Some(theme::Appearance::Dark), cx)
                .unwrap_or_else(|error| panic!("UI init: {error}"));
            cx.bind_keys([
                KeyBinding::new(
                    "alt-enter",
                    crate::actions::OpenSelectedNewWindow,
                    Some("CommandPalette"),
                ),
                KeyBinding::new("cmd-shift-w", crate::actions::CloseWindow, None),
            ]);
        });
        let (backend, updates) = async_channel::unbounded();
        let (commands, received) = async_channel::unbounded();
        let store = cx.new(|cx| Store::new(updates, commands, cx));
        let (first, first_window) = cx.update(|cx| open_window(&store, Vec::new(), cx));
        let (requests, opened) = async_channel::unbounded();
        cx.update(|cx| {
            cx.subscribe(&first, move |_, request: &NewWindowRequest, _| {
                assert!(requests.try_send(request.0.clone()).is_ok());
            })
            .detach();
        });
        assert!(backend.try_send(Update::Snapshot(Box::default())).is_ok());
        cx.run_until_parked();
        let before = first.read_with(cx, |shell, _| shell.state.clone());
        assert!(
            first_window
                .update(cx, |_, window, cx| {
                    first.update(cx, |shell, cx| shell.open_palette("GitOps", window, cx));
                })
                .is_ok()
        );
        cx.simulate_keystrokes(first_window, "alt-enter");
        let initial = opened
            .try_recv()
            .unwrap_or_else(|error| panic!("new window request: {error}"));
        assert_eq!(initial, vec![Command::SelectPage(Page::GitOps)]);
        let (second, second_window) = cx.update(|cx| open_window(&store, initial, cx));
        cx.run_until_parked();
        assert_eq!(first.read_with(cx, |shell, _| shell.state.clone()), before);
        assert_eq!(
            second.read_with(cx, |shell, _| shell.state.location().page),
            Page::GitOps
        );
        let mut snapshot = fleet_core::Snapshot::default();
        snapshot.home.problems.push(fleet_core::home::Problem {
            target: fleet_core::ResourceRef {
                cluster: ClusterId::new("test"),
                kind: "Pod".into(),
                namespace: None,
                name: "unavailable".into(),
            },
            status: Status::Err,
            message: "unavailable".into(),
            age: std::time::Duration::ZERO,
        });
        assert!(
            backend
                .try_send(Update::Snapshot(Box::new(snapshot.clone())))
                .is_ok()
        );
        cx.run_until_parked();
        assert_eq!(
            first.read_with(cx, |shell, _| shell.state.snapshot.clone()),
            snapshot
        );
        assert_eq!(
            second.read_with(cx, |shell, _| shell.state.snapshot.clone()),
            snapshot
        );
        cx.simulate_keystrokes(second_window, "cmd-shift-w");
        assert_eq!(cx.read(|cx| cx.windows().len()), 1);
        drop(second);
        cx.run_until_parked();
        let sent: Vec<_> = std::iter::from_fn(|| received.try_recv().ok()).collect();
        assert!(
            sent.iter()
                .any(|command| matches!(command, Command::WindowClosed(_)))
        );
    }

    #[gpui::test]
    fn palette_new_window_does_not_run_panel_commands(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(Some(theme::Appearance::Light), cx)
                .unwrap_or_else(|error| panic!("UI init: {error}"));
            cx.bind_keys([KeyBinding::new(
                "alt-enter",
                crate::actions::OpenSelectedNewWindow,
                Some("CommandPalette"),
            )]);
        });
        let (_, updates) = async_channel::unbounded();
        let (commands, received) = async_channel::unbounded();
        let store = cx.new(|cx| Store::new(updates, commands, cx));
        let (shell, window) = cx.update(|cx| open_window(&store, Vec::new(), cx));
        assert!(
            window
                .update(cx, |_, window, cx| {
                    shell.update(cx, |shell, cx| {
                        shell.open_palette(">toggle sidebar", window, cx);
                    });
                })
                .is_ok()
        );
        cx.simulate_keystrokes(window, "alt-enter");
        assert!(received.is_empty());
        assert!(shell.read_with(cx, |shell, cx| shell.palette.read(cx).is_open()));
        assert!(shell.read_with(cx, |shell, _| shell.state.sidebar_open));
    }
}
