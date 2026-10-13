use std::time::Instant;

use fleet_core::history::{Cycle, History, Location};
use fleet_core::palette::COMMAND_PREFIX;
use fleet_core::{Command, Page};
use gpui::{
    App, Context, Div, Entity, FocusHandle, InteractiveElement, KeyDownEvent, Stateful, Window,
};

use super::Shell;
use crate::actions::{
    CloseInspector, CloseWindow, FocusContent, FocusSidebar, GoBack, GoForward, NextRecent,
    PreviousRecent, QuickOpen, ShowCommands, ToggleDock,
};
use crate::kit::DockState;
use crate::kit::list_keys::{Nav, TypeAhead, find, typed};
use crate::screens::workload::WorkloadScreen;

pub(super) const SIDEBAR_CONTEXT: &str = "FleetSidebar";

pub(super) struct Keyboard {
    pub(super) history: History,
    pub(super) restoring: bool,
    pub(super) refocus: bool,
    pub(super) sidebar_focus: FocusHandle,
    pub(super) nav_cursor: Option<Page>,
    pub(super) nav_type_ahead: TypeAhead,
}

impl Keyboard {
    pub(super) fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            history: History::default(),
            restoring: false,
            refocus: false,
            sidebar_focus: cx.focus_handle().tab_stop(true),
            nav_cursor: None,
            nav_type_ahead: TypeAhead::default(),
        }
    }
}

impl Shell {
    pub(super) fn key_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
            if !this.close_inspector(cx) && !this.close_dock(cx) {
                window.remove_window();
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
            let target = this.keys.history.back();
            this.go(target, cx);
        }))
        .on_action(cx.listener(|this, _: &GoForward, _, cx| {
            let target = this.keys.history.forward();
            this.go(target, cx);
        }))
        .on_action(cx.listener(|this, _: &NextRecent, _, cx| {
            let target = this.keys.history.cycle(Cycle::Older);
            this.go(target, cx);
        }))
        .on_action(cx.listener(|this, _: &PreviousRecent, _, cx| {
            let target = this.keys.history.cycle(Cycle::Newer);
            this.go(target, cx);
        }))
        .on_action(cx.listener(|this, _: &FocusSidebar, window, cx| {
            this.focus_sidebar(window, cx);
        }))
        .on_action(cx.listener(|this, _: &FocusContent, window, cx| {
            this.focus_content(window, cx);
        }))
    }

    pub(super) fn record_visit(&mut self) {
        if self.keys.restoring {
            return;
        }
        let location = Location::of(&self.state);
        self.keys.refocus |= self.keys.history.current() != Some(&location);
        self.keys.history.visit(location);
    }

    pub(super) fn keep_keyboard_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.keys.refocus) || self.keys.sidebar_focus.is_focused(window) {
            return;
        }
        let focus = self.focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
    }

    fn go(&mut self, target: Option<Location>, cx: &mut Context<Self>) {
        let Some(target) = target else {
            return;
        };
        self.keys.restoring = true;
        for command in target.commands() {
            self.dispatch(command, cx);
        }
        self.keys.restoring = false;
        self.keys.refocus = true;
        self.record_visit();
    }

    fn close_inspector(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some((screen, _)) = &self.workload {
            return screen.update(cx, WorkloadScreen::close_inspector);
        }
        self.pages.close_inspector(self.state.page, cx)
    }

    pub fn dock_open(&self, cx: &App) -> Option<bool> {
        self.dock(cx).map(|dock| dock.read(cx).is_open())
    }

    fn dock(&self, cx: &App) -> Option<Entity<DockState>> {
        if let Some((screen, _)) = &self.workload {
            return Some(screen.read(cx).dock().clone());
        }
        let pods = self.state.page == Page::Pods && self.has_detail();
        pods.then(|| self.pods.read(cx).dock().clone())
    }

    fn close_dock(&self, cx: &mut Context<Self>) -> bool {
        let Some(dock) = self.dock(cx).filter(|dock| dock.read(cx).is_open()) else {
            return false;
        };
        dock.update(cx, DockState::toggle);
        true
    }

    fn list_focus(&self, cx: &mut Context<Self>) -> Option<FocusHandle> {
        if let Some((screen, _)) = &self.workload {
            return Some(screen.read(cx).list_focus(cx));
        }
        let page = self.state.page;
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
        self.keys.nav_cursor = Some(self.state.page);
        window.focus(&self.keys.sidebar_focus, cx);
        cx.notify();
    }

    pub(super) fn nav_pages(&self) -> Vec<Page> {
        self.state
            .snapshot
            .nav_sections(self.state.cluster.as_ref())
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
            .or(Some(self.state.page))
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
