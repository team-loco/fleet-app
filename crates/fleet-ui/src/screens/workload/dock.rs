use std::ops::Range;

use fleet_core::workload::{Event, Pod};
use fleet_core::{
    Command, ExecEvent, ExecRequest, ExecTarget, LogLevel, LogRequest, ResourceRef, Sink, Source,
    TermSize, Workload,
};
use gpui::{
    AnyElement, Context, Entity, Focusable, IntoElement, ParentElement, Pixels, Styled,
    Subscription, Window, div, px, uniform_list,
};

use super::{WorkloadScreen, logs_tab_label};
use crate::kit::status_symbols;
use crate::kit::{
    Button, Dock, DockState, DockTab, InputEvent, Level, LogLine, LogView, Terminal, TerminalEvent,
    TextField, TextFieldState,
};
use crate::theme::{
    ActiveTheme, FONT_MONO, LOG_LINE, Palette, SPACE_3, SPACE_4, SPACE_5, SPACE_6, TEXT_GLYPH,
    TEXT_MONO,
};

pub const LOGS: usize = 0;
pub const TERMINAL: usize = 2;
pub const FILTER_PLACEHOLDER: &str = "filter  /regex/";

const LOGS_LABEL: &str = "Logs";
const EVENTS_LABEL: &str = "Events";
const TERMINAL_LABEL: &str = "Terminal";
const DEBUG: &str = "debug";
const EXEC: &str = "exec";
const PREVIOUS: &str = "previous";
const FOLLOW: &str = "follow";
const WRAP: &str = "wrap";
const PREVIOUS_TOOLTIP: &str = "Logs of the container instance before the last restart";
const NO_PREVIOUS_TOOLTIP: &str = "This container has not restarted";
const DEBUG_IMAGE: &str = "busybox:1.36";
const POD_KIND: &str = "Pod";

const EVENT_AGE: Pixels = px(48.);
const EVENT_GLYPH: Pixels = px(14.);
const EVENT_REASON: Pixels = px(150.);
const EVENT_COUNT: Pixels = px(34.);

pub struct ExecSession {
    input: async_channel::Sender<Vec<u8>>,
    resize: async_channel::Sender<TermSize>,
    debug: bool,
    open: bool,
    _task: gpui::Task<()>,
}

pub fn subscribe(
    dock: &Entity<DockState>,
    filter: &Entity<TextFieldState>,
    terminal: &Entity<Terminal>,
    cx: &mut Context<WorkloadScreen>,
) -> Vec<Subscription> {
    vec![
        cx.observe(dock, |this, dock, cx| {
            let (active, open) = {
                let dock = dock.read(cx);
                (dock.active(), dock.is_open())
            };
            if active == TERMINAL && open {
                this.ensure_exec(cx);
            }
            cx.notify();
        }),
        cx.subscribe(filter, |this, filter, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                let query = filter.read(cx).value();
                this.logs.update(cx, |logs, cx| logs.set_query(query, cx));
            }
        }),
        cx.subscribe(terminal, |this, _, event: &TerminalEvent, _| {
            let Some(session) = this.exec.as_ref().filter(|session| session.open) else {
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
        }),
    ]
}

pub fn kit_line(line: fleet_core::LogLine) -> LogLine {
    let level = line.level.map(kit_level);
    LogLine::new(line.time.as_deref(), level, line.message)
}

fn kit_level(level: LogLevel) -> Level {
    match level {
        LogLevel::Debug => Level::Debug,
        LogLevel::Info => Level::Info,
        LogLevel::Warn => Level::Warn,
        LogLevel::Error => Level::Error,
        LogLevel::Fatal => Level::Fatal,
    }
}

fn pod_ref(workload: &ResourceRef, pod: &str) -> ResourceRef {
    ResourceRef {
        cluster: workload.cluster.clone(),
        kind: POD_KIND.into(),
        namespace: workload.namespace.clone(),
        name: pod.into(),
    }
}

pub fn has_previous(pod: &Pod, container: &str) -> bool {
    pod.container(container).is_some_and(|c| c.restarts > 0)
}

pub fn needs_debug(pod: &Pod, container: &str) -> bool {
    !pod.container(container).is_some_and(|c| c.ready)
}

impl WorkloadScreen {
    fn container_name(&self) -> Option<String> {
        self.selected_container().map(|spec| spec.name.clone())
    }

    pub(super) fn follow_logs(&mut self, cx: &mut Context<Self>) {
        let (Some(pod), Some(container)) = (self.selected_pod().cloned(), self.container_name())
        else {
            return;
        };
        self.previous = has_previous(&pod, &container);
        self.request_logs(&pod, container, cx);
    }

    fn request_logs(&mut self, pod: &Pod, container: String, cx: &mut Context<Self>) {
        let (sink, lines) = Sink::channel();
        self.logs.update(cx, LogView::clear);
        cx.emit(Command::StreamLogs(LogRequest {
            pod: pod_ref(&self.target, &pod.name),
            container,
            previous: self.previous,
            sink,
        }));
        self.log_task = Some(cx.spawn(async move |this, cx| {
            while let Ok(line) = lines.recv().await {
                let pushed = this.update(cx, |screen, cx| {
                    let line = kit_line(line);
                    screen.logs.update(cx, |logs, cx| logs.push(line, cx));
                });
                if pushed.is_err() {
                    break;
                }
            }
        }));
    }

    fn toggle_previous(&mut self, cx: &mut Context<Self>) {
        let (Some(pod), Some(container)) = (self.selected_pod().cloned(), self.container_name())
        else {
            return;
        };
        self.previous = !self.previous;
        self.request_logs(&pod, container, cx);
        cx.notify();
    }

    pub(super) fn show_logs(&mut self, cx: &mut Context<Self>) {
        self.dock.update(cx, |dock, cx| dock.select(LOGS, cx));
    }

    pub(super) fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dock.update(cx, |dock, cx| dock.select(TERMINAL, cx));
        self.ensure_exec(cx);
        let focus = self.terminal.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    fn ensure_exec(&mut self, cx: &mut Context<Self>) {
        if self.exec.as_ref().is_some_and(|session| session.open) {
            return;
        }
        let (Some(pod), Some(container)) = (self.selected_pod().cloned(), self.container_name())
        else {
            return;
        };
        let debug = needs_debug(&pod, &container);
        let target = ExecTarget {
            pod: pod_ref(&self.target, &pod.name),
            container,
            debug_image: debug.then(|| DEBUG_IMAGE.to_owned()),
        };
        let (input, source) = Source::channel();
        let (resize, sizes) = Source::channel();
        if let Some(size) = self.terminal.read(cx).size() {
            resize.try_send(size).ok();
        }
        let (output, events) = Sink::channel();
        let command_line = target.command_line();
        self.terminal.update(cx, |terminal, cx| {
            terminal.note(command_line, cx);
            terminal.set_open(true, cx);
        });
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
        self.exec = Some(ExecSession {
            input,
            resize,
            debug,
            open: true,
            _task: task,
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
                let note = match code {
                    Some(code) => format!("Session ended (exit {code})."),
                    None => "Session ended.".to_owned(),
                };
                self.terminal.update(cx, |t, cx| {
                    t.note(note, cx);
                    t.set_open(false, cx);
                });
                if let Some(session) = self.exec.as_mut() {
                    session.open = false;
                }
            }
        }
    }

    fn terminal_detail(&self) -> &'static str {
        let debug = if let Some(session) = &self.exec {
            session.debug
        } else {
            let container = self.container_name().unwrap_or_default();
            self.selected_pod()
                .is_some_and(|pod| needs_debug(pod, &container))
        };
        if debug { DEBUG } else { EXEC }
    }

    fn log_tools(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let follow = self.logs.read(cx).follow();
        let wrap = self.logs.read(cx).wrap();
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
            .child(TextField::search(&self.filter))
            .child(
                Button::new("logs-previous", PREVIOUS)
                    .small()
                    .selected(self.previous)
                    .disabled(!can_previous)
                    .tooltip(previous_tip)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_previous(cx))),
            )
            .child(
                Button::new("logs-follow", FOLLOW)
                    .small()
                    .selected(follow)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.logs
                            .update(cx, |logs, cx| logs.set_follow(!follow, cx));
                        cx.notify();
                    })),
            )
            .child(
                Button::new("logs-wrap", WRAP)
                    .small()
                    .selected(wrap)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.logs.update(cx, |logs, cx| logs.set_wrap(!wrap, cx));
                        cx.notify();
                    })),
            )
    }

    pub(super) fn render_dock(&self, workload: &Workload, cx: &mut Context<Self>) -> AnyElement {
        let logs_detail = match (&self.pod, self.container_name()) {
            (Some(pod), Some(container)) => logs_tab_label(pod, &container),
            _ => String::new(),
        };
        let events = Self::events_pane(workload.events.len(), cx);
        Dock::new(&self.dock)
            .tab(
                DockTab::new(LOGS_LABEL)
                    .detail(logs_detail)
                    .tools(self.log_tools(cx))
                    .content(self.logs.clone()),
            )
            .tab(
                DockTab::new(EVENTS_LABEL)
                    .detail(workload.events.len().to_string())
                    .content(events),
            )
            .tab(
                DockTab::new(TERMINAL_LABEL)
                    .detail(self.terminal_detail())
                    .content(self.terminal.clone()),
            )
            .into_any_element()
    }

    fn events_pane(count: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let palette = *cx.palette();
        let list = uniform_list("workload-events", count, cx.processor(Self::render_events))
            .flex_1()
            .min_h_0();
        div()
            .size_full()
            .flex()
            .flex_col()
            .py(SPACE_3)
            .bg(palette.editor)
            .font_family(FONT_MONO)
            .text_size(TEXT_MONO)
            .child(list)
    }

    fn render_events(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let palette = *cx.palette();
        let events = self
            .workload
            .as_ref()
            .map(|w| w.events.as_slice())
            .unwrap_or_default();
        range
            .filter_map(|ix| events.get(ix).map(|event| event_row(event, &palette)))
            .collect()
    }
}

fn event_row(event: &Event, palette: &Palette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(SPACE_5)
        .w_full()
        .h(LOG_LINE)
        .px(SPACE_6)
        .whitespace_nowrap()
        .child(
            div()
                .w(EVENT_AGE)
                .flex_none()
                .text_color(palette.mute)
                .child(fleet_core::time::age(event.age)),
        )
        .child(
            div()
                .w(EVENT_GLYPH)
                .flex_none()
                .text_size(TEXT_GLYPH)
                .text_color(palette.status(event.status))
                .child(status_symbols(event.status.glyph())),
        )
        .child(
            div()
                .w(EVENT_REASON)
                .flex_none()
                .truncate()
                .text_color(palette.fg)
                .child(event.reason.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(palette.fg2)
                .child(event.message.clone()),
        )
        .child(
            div()
                .w(EVENT_COUNT)
                .flex_none()
                .flex()
                .justify_end()
                .pr(SPACE_4)
                .text_color(palette.mute)
                .child(format!("×{}", event.count)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use fleet_core::Status;
    use fleet_core::workload::ContainerStatus;

    use super::*;

    fn pod(ready: bool, restarts: u32) -> Pod {
        Pod {
            name: "checkout-api-7f9c6d-x2lqp".into(),
            phase: "Running".into(),
            status: Status::Ok,
            node: String::new(),
            cpu_millis: None,
            memory_bytes: None,
            age: Duration::ZERO,
            containers: vec![ContainerStatus {
                name: "api".into(),
                ready,
                restarts,
                last_exit: None,
            }],
        }
    }

    #[test]
    fn previous_logs_only_exist_after_a_restart() {
        assert!(has_previous(&pod(false, 14), "api"));
        assert!(!has_previous(&pod(true, 0), "api"));
        assert!(!has_previous(&pod(false, 3), "envoy"));
    }

    #[test]
    fn unready_containers_get_a_debug_container() {
        assert!(needs_debug(&pod(false, 14), "api"));
        assert!(!needs_debug(&pod(true, 0), "api"));
        assert!(needs_debug(&pod(true, 0), "missing"));
    }

    #[test]
    fn core_lines_keep_time_level_and_message() {
        let line = kit_line(fleet_core::LogLine::new(
            "21:12:20.231",
            LogLevel::Fatal,
            "config invalid",
        ));
        assert_eq!(line.timestamp.as_deref(), Some("21:12:20.231"));
        assert_eq!(line.level, Some(Level::Fatal));
        assert_eq!(line.message.as_ref(), "config invalid");
    }
}
