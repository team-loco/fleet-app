use std::ops::Range;

use gpui::{
    AnyElement, App, Context, ElementId, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, SharedString, StatefulInteractiveElement,
    Styled, UniformListScrollHandle, Window, div, uniform_list,
};

use crate::theme::{
    ActiveTheme, FONT_MONO, LOG_LINE, SPACE_1, SPACE_3, SPACE_6, TERMINAL_CURSOR_HEIGHT,
    TERMINAL_CURSOR_WIDTH, TEXT_MONO,
};

const ESC: char = '\u{1b}';
const BACKSPACE: char = '\u{8}';
const BELL: char = '\u{7}';
const CSI_FINAL: std::ops::RangeInclusive<char> = '\u{40}'..='\u{7e}';
const DELETE: u8 = 0x7f;
const ESCAPE: u8 = 0x1b;
const CTRL_OFFSET: u8 = 0x60;
const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_DOWN: &[u8] = b"\x1b[B";
const ARROW_RIGHT: &[u8] = b"\x1b[C";
const ARROW_LEFT: &[u8] = b"\x1b[D";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Note,
    Output,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalLine {
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct TerminalBuffer {
    lines: Vec<TerminalLine>,
    open: bool,
    pending: Vec<u8>,
    escape: Escape,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Escape {
    #[default]
    None,
    Start,
    Csi,
    Osc,
}

impl TerminalBuffer {
    pub fn lines(&self) -> &[TerminalLine] {
        &self.lines
    }

    pub fn note(&mut self, text: impl Into<String>) {
        if self.current_is_empty_output() {
            self.lines.pop();
        }
        self.lines.push(TerminalLine {
            kind: LineKind::Note,
            text: text.into(),
        });
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&self.pending) {
            Ok(text) => text.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => self.pending.len(),
        };
        let chunk: Vec<u8> = self.pending.drain(..valid).collect();
        let text = String::from_utf8_lossy(&chunk).into_owned();
        for c in text.chars() {
            self.push_char(c);
        }
    }

    fn push_char(&mut self, c: char) {
        self.escape = match (self.escape, c) {
            (Escape::None, ESC) => Escape::Start,
            (Escape::None, _) => {
                self.print(c);
                Escape::None
            }
            (Escape::Start, '[') => Escape::Csi,
            (Escape::Start, ']') => Escape::Osc,
            (Escape::Start, _) => Escape::None,
            (Escape::Csi, c) if CSI_FINAL.contains(&c) => Escape::None,
            (Escape::Csi, _) => Escape::Csi,
            (Escape::Osc, BELL) => Escape::None,
            (Escape::Osc, _) => Escape::Osc,
        };
    }

    fn print(&mut self, c: char) {
        match c {
            '\n' => self.lines.push(Self::output(String::new())),
            '\r' => {}
            BACKSPACE => {
                if let Some(line) = self.current_output() {
                    line.text.pop();
                }
            }
            c if c.is_control() => {}
            c => match self.current_output() {
                Some(line) => line.text.push(c),
                None => self.lines.push(Self::output(c.to_string())),
            },
        }
    }

    fn output(text: String) -> TerminalLine {
        TerminalLine {
            kind: LineKind::Output,
            text,
        }
    }

    fn current_output(&mut self) -> Option<&mut TerminalLine> {
        self.lines
            .last_mut()
            .filter(|line| line.kind == LineKind::Output)
    }

    fn current_is_empty_output(&self) -> bool {
        self.lines
            .last()
            .is_some_and(|line| line.kind == LineKind::Output && line.text.is_empty())
    }
}

pub fn key_bytes(key: &str, key_char: Option<&str>, control: bool) -> Option<Vec<u8>> {
    if control {
        let letter = key
            .as_bytes()
            .first()
            .copied()
            .filter(u8::is_ascii_lowercase);
        return letter
            .filter(|_| key.len() == 1)
            .map(|b| vec![b - CTRL_OFFSET]);
    }
    match key {
        "enter" => Some(b"\r".to_vec()),
        "backspace" => Some(vec![DELETE]),
        "tab" => Some(b"\t".to_vec()),
        "escape" => Some(vec![ESCAPE]),
        "up" => Some(ARROW_UP.to_vec()),
        "down" => Some(ARROW_DOWN.to_vec()),
        "right" => Some(ARROW_RIGHT.to_vec()),
        "left" => Some(ARROW_LEFT.to_vec()),
        _ => key_char.map(|text| text.as_bytes().to_vec()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    Input(Vec<u8>),
}

pub struct Terminal {
    id: SharedString,
    buffer: TerminalBuffer,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl EventEmitter<TerminalEvent> for Terminal {}

impl Terminal {
    pub fn new(id: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            buffer: TerminalBuffer::default(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    pub fn note(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer.note(text);
        self.after_change(cx);
    }

    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.buffer.feed(bytes);
        self.after_change(cx);
    }

    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.buffer.set_open(open);
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.buffer.is_open()
    }

    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform || !self.buffer.is_open() {
            return;
        }
        let bytes = key_bytes(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            keystroke.modifiers.control,
        );
        if let Some(bytes) = bytes {
            cx.emit(TerminalEvent::Input(bytes));
            cx.stop_propagation();
        }
    }

    fn render_lines(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let palette = *cx.palette();
        let last = self.buffer.lines().len().saturating_sub(1);
        let open = self.buffer.is_open();
        range
            .filter_map(|ix| {
                let line = self.buffer.lines().get(ix)?;
                let color = match line.kind {
                    LineKind::Note => palette.mute,
                    LineKind::Output => palette.fg,
                };
                let cursor = (open && ix == last && line.kind == LineKind::Output).then(|| {
                    div()
                        .flex_none()
                        .ml(SPACE_1)
                        .w(TERMINAL_CURSOR_WIDTH)
                        .h(TERMINAL_CURSOR_HEIGHT)
                        .bg(palette.fg)
                });
                Some(
                    div()
                        .flex()
                        .items_center()
                        .h(LOG_LINE)
                        .px(SPACE_6)
                        .whitespace_nowrap()
                        .text_color(color)
                        .child(line.text.clone())
                        .children(cursor)
                        .into_any_element(),
                )
            })
            .collect()
    }
}

impl Focusable for Terminal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Terminal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let list = uniform_list(
            ElementId::Name(self.id.clone()),
            self.buffer.lines().len(),
            cx.processor(Self::render_lines),
        )
        .track_scroll(&self.scroll)
        .flex_1()
        .min_h_0();
        div()
            .id(ElementId::NamedInteger(self.id.clone(), 0))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_click(cx.listener(|this, _, window, cx| window.focus(&this.focus, cx)))
            .size_full()
            .flex()
            .flex_col()
            .py(SPACE_3)
            .bg(palette.editor)
            .font_family(FONT_MONO)
            .text_size(TEXT_MONO)
            .child(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(buffer: &TerminalBuffer) -> Vec<&str> {
        buffer
            .lines()
            .iter()
            .map(|line| line.text.as_str())
            .collect()
    }

    #[test]
    fn splits_output_into_lines_and_drops_carriage_returns() {
        let mut buffer = TerminalBuffer::default();
        buffer.feed(b"/ # ls\r\napp.yaml\r\n/ # ");
        assert_eq!(texts(&buffer), ["/ # ls", "app.yaml", "/ # "]);
    }

    #[test]
    fn notes_sit_between_output_lines() {
        let mut buffer = TerminalBuffer::default();
        buffer.note("$ kubectl debug");
        buffer.feed(b"one\n");
        buffer.note("session ended");
        assert_eq!(texts(&buffer), ["$ kubectl debug", "one", "session ended"]);
        assert_eq!(buffer.lines()[0].kind, LineKind::Note);
        assert_eq!(buffer.lines()[1].kind, LineKind::Output);
    }

    #[test]
    fn backspace_erases_and_escape_sequences_vanish() {
        let mut buffer = TerminalBuffer::default();
        buffer.feed(b"lx\x08 \x08s \x1b[31mred\x1b[0m \x1b]0;title\x07done");
        assert_eq!(texts(&buffer), ["ls red done"]);
    }

    #[test]
    fn utf8_split_across_chunks_is_kept_whole() {
        let mut buffer = TerminalBuffer::default();
        let bullet = "•".as_bytes();
        buffer.feed(&bullet[..1]);
        buffer.feed(&bullet[1..]);
        assert_eq!(texts(&buffer), ["•"]);
    }

    #[test]
    fn keys_map_to_tty_bytes() {
        assert_eq!(key_bytes("enter", None, false), Some(b"\r".to_vec()));
        assert_eq!(key_bytes("backspace", None, false), Some(vec![DELETE]));
        assert_eq!(key_bytes("a", Some("a"), false), Some(b"a".to_vec()));
        assert_eq!(key_bytes("c", Some("c"), true), Some(vec![3]));
        assert_eq!(key_bytes("d", Some("d"), true), Some(vec![4]));
        assert_eq!(key_bytes("left", None, false), Some(ARROW_LEFT.to_vec()));
        assert_eq!(key_bytes("shift", None, false), None);
    }
}
