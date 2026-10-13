mod cluster_menu;
mod page;
mod panels;
mod status_bar;

use std::collections::BTreeMap;

use fleet_core::resource::short_kind;
use fleet_core::{AddClusterStep, AppState, ClusterId, Command, Receiver, Sender, Status, Update};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, Styled, Subscription, Task, Window, div,
};

use crate::actions::{ToggleClustersPanel, ToggleCommandPalette, ToggleResourcesPanel};
use crate::kit::{ActionMenu, Toast, toast};
use crate::screens::add_cluster::{AddClusterDialog, AddClusterEvent};
use crate::screens::changes::ChangesScreen;
use crate::screens::command::{CommandPalette, PaletteEvent};
use crate::screens::compare::CompareScreen;
use crate::screens::fleet::FleetScreen;
use crate::screens::flux_object::FluxObjectPage;
use crate::screens::gitops::GitOpsScreen;
use crate::screens::pods::PodsScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::workload::WorkloadScreen;
use crate::screens::{ClusterPages, ScreenEvent};
use crate::theme::{self, ActiveTheme, NAV_MIN_WIDTH, RULE, STRIP, TEXT_UI};

pub struct Shell {
    state: AppState,
    commands: Sender<Command>,
    pending: Vec<Command>,
    clusters_generation: usize,
    resources_generation: usize,
    focus: FocusHandle,
    fleet: Entity<FleetScreen>,
    palette: Entity<CommandPalette>,
    changes: Entity<ChangesScreen>,
    pages: ClusterPages,
    loaded: bool,
    add_cluster: Option<(Entity<AddClusterDialog>, Subscription)>,
    cluster_menus: BTreeMap<ClusterId, Entity<ActionMenu>>,
    workload: Option<(Entity<WorkloadScreen>, Subscription)>,
    flux_object: Entity<FluxObjectPage>,
    #[cfg(feature = "loco")]
    loco: Entity<crate::screens::loco::LocoScreens>,
    #[cfg(feature = "loco")]
    _loco_commands: Subscription,
    compare: Entity<CompareScreen>,
    settings: Entity<SettingsScreen>,
    _updates: Task<()>,
    _appearance: Subscription,
    _fleet_commands: Subscription,
    _palette_events: Subscription,
    gitops: Entity<GitOpsScreen>,
    _gitops_commands: Subscription,
    pods: Entity<PodsScreen>,
    _pod_commands: Subscription,
    _flux_object: Subscription,
    _screen_commands: [Subscription; 2],
}

impl Shell {
    pub fn new(
        updates: Receiver<Update>,
        commands: Sender<Command>,
        initial: Vec<Command>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
        let gitops = cx.new(GitOpsScreen::new);
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
        let screen_commands = [
            cx.subscribe(&compare, |this, _, command: &Command, cx| {
                this.dispatch(command.clone(), cx);
            }),
            cx.subscribe(&settings, |this, _, command: &Command, cx| {
                this.dispatch(command.clone(), cx);
            }),
        ];
        let pods = cx.new(|cx| PodsScreen::new(window, cx));
        let pod_commands = cx.subscribe(&pods, |this, _, command: &Command, cx| {
            this.dispatch(command.clone(), cx);
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        theme::follow_window(window, cx);
        let palette = cx.new(|cx| CommandPalette::new(window, cx));
        let palette_events = cx.subscribe_in(
            &palette,
            window,
            |this, palette, PaletteEvent::Run(command), window, cx| {
                palette.update(cx, |palette, cx| palette.close(window, cx));
                this.dispatch(command.clone(), cx);
            },
        );
        let pages = ClusterPages::new(window, cx, Self::screen_event);
        let flux_object = cx.new(FluxObjectPage::new);
        let flux_commands = cx.subscribe(&flux_object, |shell, _, command: &Command, cx| {
            shell.dispatch(command.clone(), cx);
        });
        Self {
            state: AppState::default(),
            commands,
            pending: initial,
            clusters_generation: 0,
            resources_generation: 0,
            focus,
            fleet,
            palette,
            changes,
            pages,
            loaded: false,
            add_cluster: None,
            cluster_menus: BTreeMap::new(),
            workload: None,
            flux_object,
            #[cfg(feature = "loco")]
            loco,
            #[cfg(feature = "loco")]
            _loco_commands: loco_commands,
            compare,
            settings,
            _updates: listen,
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

    fn receive(&mut self, update: Update, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((dialog, _)) = &self.add_cluster {
            dialog.update(cx, |dialog, cx| dialog.receive(&update, window, cx));
        }
        if let Update::Snapshot(_) = update {
            self.loaded = true;
        }
        if let Update::Notice(notice) = &update {
            let shown = Toast::new(notice.status, notice.title.clone());
            let shown = match &notice.detail {
                Some(detail) => shown.detail(detail.clone()),
                None => shown,
            };
            toast(shown, cx);
        }
        self.state.update(update);
        self.sync_cluster_menus(cx);
        let pending = std::mem::take(&mut self.pending);
        for command in pending {
            if let Command::ShowClusterSettings(cluster) = &command {
                self.settings
                    .update(cx, |settings, cx| settings.open(cluster, cx));
            }
            self.state.apply(&command);
            if let Command::OpenAddCluster(step) = command {
                self.open_add_cluster(step, window, cx);
            } else {
                self.send(command);
            }
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
        self.sync_palette(cx);
        cx.notify();
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
        let current = self.state.cluster.as_ref();
        self.palette.update(cx, |palette, cx| {
            palette.set_context(snapshot, current, cx);
        });
    }

    fn sync_pages(&self, cx: &mut Context<Self>) {
        let cluster = self.selected_cluster();
        let detail = cluster.and_then(|c| self.state.snapshot.detail(&c.id));
        self.pages.sync(self.state.page, cluster, detail, cx);
    }

    fn screen_event(&mut self, event: &ScreenEvent, cx: &mut Context<Self>) {
        match event {
            ScreenEvent::Command(command) => self.dispatch(command.clone(), cx),
            ScreenEvent::Navigate(page) => self.dispatch(Command::SelectPage(*page), cx),
        }
    }

    fn send(&self, command: Command) {
        if let Err(error) = self.commands.try_send(command) {
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
        match command {
            Command::ToggleClustersPanel => self.clusters_generation += 1,
            Command::ToggleResourcesPanel => self.resources_generation += 1,
            Command::SelectCluster(_) => {}
            Command::SelectPage(_) => {}
            Command::OpenResource(_) => {}
            Command::InspectResource(_) => {}
            Command::FollowLogs(_) => {}
            Command::Compare(_) => {}
            #[cfg(feature = "loco")]
            Command::Loco(_) => {}
            Command::CopySpec { .. } => {}
            Command::ShowClusterSettings(ref cluster) => {
                self.settings
                    .update(cx, |settings, cx| settings.open(cluster, cx));
            }
            Command::SetPreferences(..) => {}
            Command::Connect(_) => {}
            Command::Reauthenticate(_) => {}
            Command::Disconnect(_) => {}
            Command::PortForward {
                ref service,
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
            Command::OpenCommit(ref sha) => {
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
        self.state.apply(&command);
        if let Command::OpenResource(target) = &command {
            self.pages.reveal(target, cx);
        }
        self.sync_page(cx);
        self.send(command);
        self.sync_palette(cx);
        cx.notify();
    }

    fn sync_page(&self, cx: &mut Context<Self>) {
        let page = self.state.page;
        self.fleet.update(cx, |fleet, cx| fleet.set_page(page, cx));
        self.sync_pages(cx);
        let flux = self.state.flux_object.as_ref();
        let object = flux.and_then(|key| self.state.snapshot.flux_page(key));
        self.flux_object
            .update(cx, |screen, cx| screen.set_page(object, cx));
        #[cfg(feature = "loco")]
        self.loco
            .update(cx, |loco, cx| loco.set_context(&self.state, cx));
        let snapshot = &self.state.snapshot;
        let resource = self.state.resource.as_ref();
        self.compare.update(cx, |compare, cx| {
            compare.set_snapshot(snapshot, resource, cx);
        });
        let cluster = self.state.cluster.as_ref();
        let resource = self.state.resource.as_ref();
        self.pods
            .update(cx, |pods, cx| pods.set_context(cluster, resource, cx));
    }

    pub fn show_pod_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pods
            .update(cx, |pods, cx| pods.show_terminal(window, cx));
    }

    fn strip(cx: &App) -> impl IntoElement {
        let palette = cx.palette();
        div()
            .h(STRIP)
            .flex_none()
            .bg(palette.chrome)
            .border_b(RULE)
            .border_color(palette.line)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
    }

    fn nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .min_w(NAV_MIN_WIDTH)
            .h_full()
            .bg(palette.panel)
            .border_r(RULE)
            .border_color(palette.line)
            .child(Self::strip(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.clusters_panel(cx))
                    .child(self.resources_panel(cx)),
            )
            .into_any_element()
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let page = self.page(window, cx);
        div()
            .id("shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &ToggleClustersPanel, _, cx| {
                this.dispatch(Command::ToggleClustersPanel, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleResourcesPanel, _, cx| {
                this.dispatch(Command::ToggleResourcesPanel, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                this.toggle_palette(window, cx);
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
                    .child(self.nav(cx))
                    .child(page),
            )
            .child(self.status_bar(cx))
            .child(self.palette.clone())
            .children(self.add_cluster.as_ref().map(|(dialog, _)| dialog.clone()))
            .child(crate::kit::overlay_layer(cx))
    }
}
