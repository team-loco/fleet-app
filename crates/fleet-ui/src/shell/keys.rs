use std::time::Instant;

use fleet_core::palette::COMMAND_PREFIX;
use fleet_core::tabs::Cycle;
use fleet_core::{Command, Page};
use gpui::{
    App, Context, Div, Entity, FocusHandle, InteractiveElement, KeyDownEvent, Stateful, Window,
};

use super::Shell;
use crate::actions::{
    ActivateTab, CloseInspector, CloseTab, CloseWindow, FocusContent, FocusSidebar, GoBack,
    GoForward, NewTab, NextTab, PreviousTab, QuickOpen, ShowCommands, ToggleDock,
};
use crate::kit::DockState;
use crate::kit::list_keys::{Nav, TypeAhead, find, typed};
use crate::screens::workload::WorkloadScreen;

pub(super) const SIDEBAR_CONTEXT: &str = "FleetSidebar";

pub(super) struct Keyboard {
    pub(super) refocus: bool,
    pub(super) sidebar_focus: FocusHandle,
    pub(super) nav_cursor: Option<Page>,
    pub(super) nav_type_ahead: TypeAhead,
}

impl Keyboard {
    pub(super) fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            refocus: false,
            sidebar_focus: cx.focus_handle().tab_stop(true),
            nav_cursor: None,
            nav_type_ahead: TypeAhead::default(),
        }
    }
}

impl Shell {
    pub(super) fn key_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|_, _: &CloseWindow, window, _| {
            window.remove_window();
        }))
        .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
            let active = this.state.panes().active().id();
            this.close_tab(active, cx);
        }))
        .on_action(cx.listener(|this, _: &NewTab, _, cx| {
            this.state.new_tab();
            this.show_active(cx);
        }))
        .on_action(cx.listener(|this, _: &NextTab, _, cx| {
            if this.state.panes_mut().cycle(Cycle::Next) {
                this.show_active(cx);
            }
        }))
        .on_action(cx.listener(|this, _: &PreviousTab, _, cx| {
            if this.state.panes_mut().cycle(Cycle::Previous) {
                this.show_active(cx);
            }
        }))
        .on_action(cx.listener(|this, action: &ActivateTab, _, cx| {
            if this.state.panes_mut().activate_nth(action.0) {
                this.show_active(cx);
            }
        }))
        .on_action(cx.listener(|this, _: &CloseInspector, _, cx| {
            this.close_inspector(cx);
        }))
        .on_action(cx.listener(|this, _: &QuickOpen, window, cx| {
            this.open_palette("", window, cx);
        }))
        .on_action(cx.listener(|this, _: &ShowCommands, window, cx| {
            this.open_palette(COMMAND_PREFIX, window, cx);
        }))
        .on_action(cx.listener(|this, _: &ToggleDock, _, cx| {
            if let Some(dock) = this.dock(cx) {
                dock.update(cx, DockState::toggle);
            }
        }))
        .on_action(cx.listener(|this, _: &GoBack, _, cx| {
            if this.state.back() {
                this.show_active(cx);
            }
        }))
        .on_action(cx.listener(|this, _: &GoForward, _, cx| {
            if this.state.forward() {
                this.show_active(cx);
            }
        }))
        .on_action(cx.listener(|this, _: &FocusSidebar, window, cx| {
            this.focus_sidebar(window, cx);
        }))
        .on_action(cx.listener(|this, _: &FocusContent, window, cx| {
            this.focus_content(window, cx);
        }))
    }

    pub(super) fn keep_keyboard_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.keys.refocus) || self.keys.sidebar_focus.is_focused(window) {
            return;
        }
        let focus = self.focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
    }

    fn close_inspector(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some((screen, _)) = &self.workload {
            return screen.update(cx, WorkloadScreen::close_inspector);
        }
        self.pages.close_inspector(self.state.location().page, cx)
    }

    pub fn dock_open(&self, cx: &App) -> Option<bool> {
        self.dock(cx).map(|dock| dock.read(cx).is_open())
    }

    fn dock(&self, cx: &App) -> Option<Entity<DockState>> {
        if let Some((screen, _)) = &self.workload {
            return Some(screen.read(cx).dock().clone());
        }
        let pods = self.state.location().page == Page::Pods && self.has_detail();
        pods.then(|| self.pods.read(cx).dock().clone())
    }

    fn list_focus(&self, cx: &mut Context<Self>) -> Option<FocusHandle> {
        if let Some((screen, _)) = &self.workload {
            return Some(screen.read(cx).list_focus(cx));
        }
        let page = self.state.location().page;
        match page {
            Page::Overview => Some(self.fleet.read(cx).list_focus(page, cx)),
            Page::Problems => Some(self.fleet.read(cx).list_focus(page, cx)),
            Page::Changes => Some(self.changes.read(cx).list_focus(cx)),
            Page::ClusterSettings => Some(self.settings.read(cx).list_focus(cx)),
            Page::Pods => Some(self.pods.read(cx).list_focus(cx)),
            page => self.pages.list_focus(page, cx),
        }
    }

    fn focus_content(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.list_focus(cx) {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.keys.sidebar_focus, cx);
            window.focus_next(cx);
        }
        cx.notify();
    }

    fn focus_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.sidebar_open {
            self.dispatch(Command::ToggleSidebar, cx);
        }
        self.keys.nav_cursor = Some(self.state.location().page);
        window.focus(&self.keys.sidebar_focus, cx);
        cx.notify();
    }

    pub(super) fn nav_pages(&self) -> Vec<Page> {
        self.state
            .snapshot
            .nav_sections(self.state.location().cluster.as_ref())
            .iter()
            .flat_map(|section| section.pages.iter().copied())
            .collect()
    }

    pub(super) fn sidebar_key(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let pages = self.nav_pages();
        let current = self
            .keys
            .nav_cursor
            .or(Some(self.state.location().page))
            .and_then(|page| pages.iter().position(|p| *p == page));
        let opens =
            matches!(keystroke.key.as_str(), "enter" | "space") && !keystroke.modifiers.modified();
        if opens {
            if let Some(page) = current.and_then(|ix| pages.get(ix).copied()) {
                self.dispatch(Command::SelectPage(page), cx);
            }
            cx.stop_propagation();
            return;
        }
        let target = match (Nav::parse(keystroke), typed(keystroke)) {
            (Some(nav), _) => nav.target(current, pages.len()),
            (None, Some(text)) => {
                let query = self
                    .keys
                    .nav_type_ahead
                    .push(text, Instant::now())
                    .to_owned();
                find(&query, pages.iter().map(|page| page.label()))
            }
            (None, None) => return,
        };
        cx.stop_propagation();
        if let Some(page) = target.and_then(|ix| pages.get(ix).copied()) {
            self.keys.nav_cursor = Some(page);
            cx.notify();
        }
    }
}
