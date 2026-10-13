use fleet_core::pods::{Container, ContainerState, Pod};
use fleet_core::{
    Command, ExecEvent, ExecRequest, ExecTarget, LogRequest, ResourceRef, Sender, Sink, Source,
    Status, TermSize,
};
use gpui::{
    AppContext as _, Context, Focusable, IntoElement, ParentElement, Styled, Subscription, Task,
    Window, div,
};

use super::PodsScreen;
use crate::assets::{DOWNLOAD, PLUS};
use crate::kit::{
    Button, Dock, DockTab, IconButton, LogView, Terminal, TerminalEvent, TextField, Toast, toast,
};
use crate::screens::workload::kit_line;
use crate::theme::SPACE_3;

pub const SEARCH_PLACEHOLDER: &str = "filter  /regex/";
pub const TERMINAL_ID: &str = "pod-terminal";
const LOGS_TAB: usize = 0;
const TERMINAL_TAB: usize = 1;
const LOGS_LABEL: &str = "Logs";
const TERMINAL_LABEL: &str = "Terminal";
const PREVIOUS: &str = "previous";
const FOLLOW: &str = "follow";
const WRAP: &str = "wrap";
const TIMESTAMPS: &str = "timestamps";
const DEBUG: &str = "debug";
const DOWNLOAD_TOOLTIP: &str = "Download logs";
const NEW_TERMINAL_TOOLTIP: &str = "New terminal";
const DOWNLOADS: &str = "Downloads";
const SAVE_FAILED: &str = "Could not save the logs";
const SAVED: &str = "Saved logs";
const PREVIOUS_TOOLTIP: &str = "Logs of the container instance before the last restart";
const NO_PREVIOUS_TOOLTIP: &str = "This container has not restarted";
const DEBUG_IMAGE: &str = "busybox:1.36";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogKey {
    pod: ResourceRef,
    container: String,
    previous: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellKey {
    pod: ResourceRef,
    container: String,
}

pub struct LogSession {
    key: LogKey,
    _task: Task<()>,
}

pub struct ShellSession {
    key: ShellKey,
    debug: bool,
    input: Sender<Vec<u8>>,
    resize: Sender<TermSize>,
    open: bool,
    _task: Task<()>,
    _input: Subscription,
}

pub fn needs_debug(pod: &Pod, container: &str) -> bool {
    pod.containers
        .iter()
        .find(|c| c.name == container)
        .is_some_and(|c| c.status == Status::Err)
}

pub fn has_previous(pod: &Pod, container: &str) -> bool {
    pod.containers
        .iter()
        .find(|c| c.name == container)
        .is_some_and(|c| c.restarts > 0)
}

pub fn crash_looping(container: &Container) -> bool {
    container.restarts > 0 && container.state != ContainerState::Running
}

pub fn debug_reason(container: &str, other: Option<&str>) -> String {
    let debug = format!("start a debug container that shares {container}’s process namespace");
    match other {
        Some(other) => format!("{container} is restarting. Attach to {other} instead, or {debug}."),
        None => format!("{container} is restarting, so Fleet will {debug}."),
    }
}

pub fn log_file_name(pod: &str, container: &str) -> String {
    format!("{pod}-{container}.log")
}

pub fn exit_note(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("Session ended (exit {code})."),
        None => "Session ended.".to_owned(),
    }
}

impl PodsScreen {
    fn container_name(&self) -> Option<String> {
        self.selected_container().map(|c| c.name.clone())
    }

    pub(super) fn sync_streams(&mut self, cx: &mut Context<Self>) {
        let pod = self.selected_pod().map(Pod::resource);
        let container = self.container_name();
        let target = pod.zip(container);
        let log_key = target.clone().map(|(pod, container)| LogKey {
            pod,
            container,
            previous: self.previous,
        });
        if self.log.as_ref().map(|s| &s.key) != log_key.as_ref() {
            self.log = None;
            self.logs.update(cx, LogView::clear);
            if let Some(key) = log_key {
                self.open_log(key, cx);
            }
        }
        let shell_key = target.map(|(pod, container)| ShellKey { pod, container });
        if self.shell.as_ref().map(|s| &s.key) != shell_key.as_ref() && self.shell.take().is_some()
        {
            self.terminal = cx.new(|cx| Terminal::new(TERMINAL_ID, cx));
        }
        let (active, open) = {
            let dock = self.dock.read(cx);
            (dock.active(), dock.is_open())
        };
        if active == TERMINAL_TAB
            && open
            && self.shell.is_none()
            && let Some(key) = shell_key
        {
            self.open_shell(key, cx);
        }
    }

    fn open_log(&mut self, key: LogKey, cx: &mut Context<Self>) {
        let (sink, lines) = Sink::channel();
        let follow = !key.previous;
        self.logs.update(cx, |logs, cx| logs.set_follow(follow, cx));
        cx.emit(Command::StreamLogs(LogRequest {
            pod: key.pod.clone(),
            container: key.container.clone(),
            previous: key.previous,
            sink,
        }));
        let task = cx.spawn(async move |this, cx| {
            while let Ok(line) = lines.recv().await {
                let pushed = this.update(cx, |screen, cx| {
                    let line = kit_line(line);
                    screen.logs.update(cx, |logs, cx| logs.push(line, cx));
                });
                if pushed.is_err() {
                    break;
                }
            }
        });
        self.log = Some(LogSession { key, _task: task });
    }

    fn open_shell(&mut self, key: ShellKey, cx: &mut Context<Self>) {
        let debug = self
            .selected_pod()
            .is_some_and(|pod| needs_debug(pod, &key.container));
        let target = ExecTarget {
            pod: key.pod.clone(),
            container: key.container.clone(),
            debug_image: debug.then(|| DEBUG_IMAGE.to_owned()),
        };
        let terminal = cx.new(|cx| Terminal::new(TERMINAL_ID, cx));
        let command_line = target.command_line();
        let other = self.selected_pod().and_then(|pod| {
            pod.containers
                .iter()
                .find(|c| c.name != key.container && c.state == ContainerState::Running)
                .map(|c| c.name.clone())
        });
        let reason = debug.then(|| debug_reason(&key.container, other.as_deref()));
        terminal.update(cx, |terminal, cx| {
            if let Some(reason) = reason {
                terminal.note(reason, cx);
                terminal.note(String::new(), cx);
            }
            terminal.note(command_line, cx);
            terminal.set_open(true, cx);
        });
        let forward = cx.subscribe(&terminal, |this, _, event: &TerminalEvent, _| {
            let Some(session) = this.shell.as_ref().filter(|s| s.open) else {
                return;
            };
            match event {
                TerminalEvent::Input(bytes) => {
                    session.input.try_send(bytes.clone()).ok();
                }
                TerminalEvent::Resize(size) => {
                    session.resize.try_send(*size).ok();
                }
            }
        });
        let (input, source) = Source::channel();
        let (resize, sizes) = Source::channel();
        if let Some(size) = terminal.read(cx).size() {
            resize.try_send(size).ok();
        }
        self.terminal = terminal;
        let (output, events) = Sink::channel();
        cx.emit(Command::Exec(ExecRequest {
            target,
            input: source,
            resize: sizes,
            output,
        }));
        let task = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let applied = this.update(cx, |screen, cx| screen.exec_event(event, cx));
                if applied.is_err() {
                    break;
                }
            }
        });
        self.shell = Some(ShellSession {
            key,
            debug,
            input,
            resize,
            open: true,
            _task: task,
            _input: forward,
        });
    }

    fn exec_event(&mut self, event: ExecEvent, cx: &mut Context<Self>) {
        match event {
            ExecEvent::Started { note } => {
                self.terminal.update(cx, |t, cx| t.note(note, cx));
            }
            ExecEvent::Output(bytes) => {
                self.terminal.update(cx, |t, cx| t.feed(&bytes, cx));
            }
            ExecEvent::Exited { code } => {
                self.terminal.update(cx, |t, cx| {
                    t.note(exit_note(code), cx);
                    t.set_open(false, cx);
                });
                if let Some(session) = self.shell.as_mut() {
                    session.open = false;
                }
                cx.notify();
            }
        }
    }

    fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shell = None;
        self.show_terminal(window, cx);
    }

    fn download_logs(&self, cx: &mut Context<Self>) {
        let text = self.logs.read(cx).export();
        let name = match (self.selected_pod(), self.container_name()) {
            (Some(pod), Some(container)) => log_file_name(&pod.name, &container),
            (Some(pod), None) => log_file_name(&pod.name, LOGS_LABEL),
            (None, _) => return,
        };
        let directory = std::env::home_dir().unwrap_or_default().join(DOWNLOADS);
        let chosen = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let detail = path.display().to_string();
            let result = std::fs::write(&path, text);
            cx.update(|cx| {
                let message = match result {
                    Ok(()) => Toast::new(Status::Ok, SAVED).detail(detail),
                    Err(error) => Toast::new(Status::Err, SAVE_FAILED).detail(error.to_string()),
                };
                toast(message, cx);
            });
        })
        .detach();
    }

    pub fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shell.as_ref().is_some_and(|s| !s.open) {
            self.shell = None;
        }
        self.dock
            .update(cx, |dock, cx| dock.select(TERMINAL_TAB, cx));
        self.sync_streams(cx);
        let focus = self.terminal.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    pub(super) fn show_logs(&mut self, previous: bool, cx: &mut Context<Self>) {
        self.dock.update(cx, |dock, cx| dock.select(LOGS_TAB, cx));
        self.previous = previous;
        self.sync_streams(cx);
        cx.notify();
    }

    fn log_tools(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let follow = self.logs.read(cx).follow();
        let wrap = self.logs.read(cx).wrap();
        let timestamps = self.logs.read(cx).timestamps();
        let container = self.container_name().unwrap_or_default();
        let can_previous = self
            .selected_pod()
            .is_some_and(|pod| has_previous(pod, &container));
        let previous_tip = if can_previous {
            PREVIOUS_TOOLTIP
        } else {
            NO_PREVIOUS_TOOLTIP
        };
        div()
            .flex()
            .items_center()
            .gap(SPACE_3)
            .child(self.containers.clone())
            .child(TextField::search(&self.search))
            .child(
                Button::new("pod-logs-previous", PREVIOUS)
                    .small()
                    .selected(self.previous)
                    .disabled(!can_previous && !self.previous)
                    .tooltip(previous_tip)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let previous = !this.previous;
                        this.show_logs(previous, cx);
                    })),
            )
            .child(
                Button::new("pod-logs-follow", FOLLOW)
                    .small()
                    .selected(follow)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.logs
                            .update(cx, |logs, cx| logs.set_follow(!follow, cx));
                        cx.notify();
                    })),
            )
            .child(
                Button::new("pod-logs-timestamps", TIMESTAMPS)
                    .small()
                    .selected(timestamps)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.logs
                            .update(cx, |logs, cx| logs.set_timestamps(!timestamps, cx));
                        cx.notify();
                    })),
            )
            .child(
                Button::new("pod-logs-wrap", WRAP)
                    .small()
                    .selected(wrap)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.logs.update(cx, |logs, cx| logs.set_wrap(!wrap, cx));
                        cx.notify();
                    })),
            )
            .child(
                IconButton::svg("pod-logs-download", DOWNLOAD, DOWNLOAD_TOOLTIP)
                    .on_click(cx.listener(|this, _, _, cx| this.download_logs(cx))),
            )
    }

    fn terminal_tools(&self) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap(SPACE_3)
            .child(self.containers.clone())
    }

    pub(super) fn dock_view(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let detail = match (self.selected_pod(), self.container_name()) {
            (Some(pod), Some(container)) => format!("{} · {container}", pod.short_name()),
            (Some(pod), None) => pod.short_name().to_owned(),
            (None, _) => String::new(),
        };
        let terminal_detail = if self.shell.as_ref().is_some_and(|s| s.debug) {
            DEBUG.to_owned()
        } else {
            detail.clone()
        };
        let new_terminal = IconButton::svg("pod-new-terminal", PLUS, NEW_TERMINAL_TOOLTIP)
            .on_click(cx.listener(|this, _, window, cx| this.new_terminal(window, cx)));
        Dock::new(&self.dock)
            .after_tabs(new_terminal)
            .tab(
                DockTab::new(LOGS_LABEL)
                    .detail(detail.clone())
                    .tools(self.log_tools(cx))
                    .content(self.logs.clone()),
            )
            .tab(
                DockTab::new(TERMINAL_LABEL)
                    .detail(terminal_detail)
                    .tools(self.terminal_tools())
                    .content(self.terminal.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod(status: Status, restarts: u32) -> Pod {
        Pod {
            name: "checkout-api-7f9c6d-x2lqp".into(),
            containers: vec![Container {
                name: "api".into(),
                state: ContainerState::Waiting,
                status,
                restarts,
                ..Container::default()
            }],
            ..Pod::default()
        }
    }

    #[test]
    fn crashing_containers_get_a_debug_container() {
        assert!(needs_debug(&pod(Status::Err, 14), "api"));
        assert!(!needs_debug(&pod(Status::Warn, 0), "api"));
        assert!(!needs_debug(&pod(Status::Err, 14), "envoy"));
    }

    #[test]
    fn previous_logs_need_a_restart() {
        assert!(has_previous(&pod(Status::Err, 14), "api"));
        assert!(!has_previous(&pod(Status::Ok, 0), "api"));
    }

    #[test]
    fn crash_looping_containers_open_on_previous_logs() {
        let crashing = pod(Status::Err, 14);
        assert!(crash_looping(&crashing.containers[0]));
        let mut restarted = pod(Status::Ok, 1);
        restarted.containers[0].state = ContainerState::Running;
        assert!(!crash_looping(&restarted.containers[0]));
        assert!(!crash_looping(&pod(Status::Warn, 0).containers[0]));
    }

    #[test]
    fn debug_sessions_explain_why_and_logs_save_per_container() {
        assert_eq!(
            debug_reason("api", Some("envoy")),
            "api is restarting. Attach to envoy instead, or start a debug container that shares api’s process namespace."
        );
        assert_eq!(
            debug_reason("api", None),
            "api is restarting, so Fleet will start a debug container that shares api’s process namespace."
        );
        assert_eq!(
            log_file_name("checkout-api-7f9c6d-x2lqp", "api"),
            "checkout-api-7f9c6d-x2lqp-api.log"
        );
    }

    #[test]
    fn exit_notes_include_the_code_when_known() {
        assert_eq!(exit_note(Some(1)), "Session ended (exit 1).");
        assert_eq!(exit_note(None), "Session ended.");
    }
}
