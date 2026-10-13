mod page;
mod panels;
mod status_bar;

use fleet_core::{AppState, Command, Receiver, Sender, Status, Update};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, Styled, Subscription, Task, Window, div,
};

use crate::actions::{ToggleClustersPanel, ToggleCommandPalette, ToggleResourcesPanel};
use crate::kit::{Toast, toast};
use crate::screens::command::{CommandPalette, PaletteEvent};
use crate::screens::fleet::FleetScreen;
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
    _updates: Task<()>,
    _appearance: Subscription,
    _fleet_commands: Subscription,
    _palette_events: Subscription,
}

impl Shell {
    pub fn new(
        updates: Receiver<Update>,
        commands: Sender<Command>,
        initial: Vec<Command>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let listen = cx.spawn(async move |this, cx| {
            while let Ok(update) = updates.recv().await {
                let applied = this.update(cx, |shell, cx| shell.receive(update, cx));
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
        Self {
            state: AppState::default(),
            commands,
            pending: initial,
            clusters_generation: 0,
            resources_generation: 0,
            focus,
            fleet,
            palette,
            _updates: listen,
            _appearance: appearance,
            _fleet_commands: fleet_commands,
            _palette_events: palette_events,
        }
    }

    fn receive(&mut self, update: Update, cx: &mut Context<Self>) {
        self.state.update(update);
        let pending = std::mem::take(&mut self.pending);
        for command in pending {
            self.state.apply(&command);
        }
        let snapshot = &self.state.snapshot;
        self.fleet
            .update(cx, |fleet, cx| fleet.set_snapshot(snapshot, cx));
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

    fn dispatch(&mut self, command: Command, cx: &mut Context<Self>) {
        match command {
            Command::ToggleClustersPanel => self.clusters_generation += 1,
            Command::ToggleResourcesPanel => self.resources_generation += 1,
            Command::SelectCluster(_) => {}
            Command::SelectPage(_) => {}
            Command::OpenResource(_) => {}
            Command::FollowLogs(_) => {}
            Command::Compare(_) => {}
            Command::PortForward {
                ref service,
                local,
                remote,
            } => {
                let title = format!(
                    "Forwarding localhost:{local} → svc/{}:{remote}",
                    service.name
                );
                let detail = format!(
                    "{} · {}",
                    service.cluster,
                    service.namespace.as_deref().unwrap_or_default()
                );
                toast(Toast::new(Status::Ok, title).detail(detail), cx);
            }
        }
        self.state.apply(&command);
        self.sync_page(cx);
        if let Err(error) = self.commands.try_send(command) {
            tracing::warn!(%error, "backend stopped accepting commands");
        }
        self.sync_palette(cx);
        cx.notify();
    }

    fn sync_page(&self, cx: &mut Context<Self>) {
        let page = self.state.page;
        self.fleet.update(cx, |fleet, cx| fleet.set_page(page, cx));
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
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
                    .child(self.page(cx)),
            )
            .child(self.status_bar(cx))
            .child(self.palette.clone())
            .child(crate::kit::overlay_layer(cx))
    }
}
