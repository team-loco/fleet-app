use gpui::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};

use crate::theme::{
    ActiveTheme, CODE_LINE, DIFF_HEADER, DIFF_NOTE_MIN, FONT_MONO, FONT_SANS, GUTTER_NUMBER,
    GUTTER_SIGN, Palette, RADIUS, RULE, SPACE_2, SPACE_4, SPACE_5, SPACE_7, SPACE_8, TEXT_MONO,
    TEXT_SMALL,
};

const ADDED: &str = "+";
const REMOVED: &str = "−";
const NOTE_ARROW: &str = "← ";
const GIT_HEADER: &str = "diff --git ";
const NEW_PATH: &str = "+++ ";
const OLD_PATH: &str = "--- ";
const HUNK: &str = "@@";
const NO_NEWLINE: &str = "\\";
const DEV_NULL: &str = "/dev/null";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    Hunk,
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: SharedString,
    pub note: Option<SharedString>,
}

impl DiffLine {
    pub fn new(kind: LineKind, number: Option<u32>, text: impl Into<SharedString>) -> Self {
        let (old, new) = match kind {
            LineKind::Removed => (number, None),
            LineKind::Added => (None, number),
            LineKind::Context => (number, number),
            LineKind::Hunk => (None, None),
            LineKind::Meta => (None, None),
        };
        Self {
            kind,
            old,
            new,
            text: text.into(),
            note: None,
        }
    }

    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    fn number(&self) -> Option<u32> {
        match self.kind {
            LineKind::Removed => self.old,
            LineKind::Added => self.new,
            LineKind::Context => self.new,
            LineKind::Hunk => None,
            LineKind::Meta => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiffFile {
    pub path: SharedString,
    pub lines: Vec<DiffLine>,
}

impl DiffFile {
    pub fn new(path: impl Into<SharedString>, lines: Vec<DiffLine>) -> Self {
        Self {
            path: path.into(),
            lines,
        }
    }

    pub fn added(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind == LineKind::Added)
            .count()
    }

    pub fn removed(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind == LineKind::Removed)
            .count()
    }
}

fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.split_whitespace().skip(1);
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let start = |range: &str| range.split(',').next()?.parse::<u32>().ok();
    Some((start(old)?, start(new)?))
}

fn clean_path(path: &str) -> &str {
    let path = path.split('\t').next().unwrap_or(path);
    path.strip_prefix("b/")
        .or_else(|| path.strip_prefix("a/"))
        .unwrap_or(path)
}

pub fn parse_unified(text: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut old = 0u32;
    let mut new = 0u32;
    let mut in_hunk = false;
    let mut header_open = false;
    for raw in text.lines() {
        if let Some(rest) = raw.strip_prefix(GIT_HEADER) {
            let path = rest
                .split_whitespace()
                .last()
                .map(clean_path)
                .unwrap_or_default();
            files.push(DiffFile::new(path.to_owned(), Vec::new()));
            in_hunk = false;
            header_open = true;
            continue;
        }
        if !in_hunk && raw.starts_with(OLD_PATH) {
            continue;
        }
        if !in_hunk && let Some(path) = raw.strip_prefix(NEW_PATH) {
            let path = clean_path(path);
            let current = files.last_mut().filter(|_| header_open);
            match current {
                Some(file) if path != DEV_NULL => file.path = path.to_owned().into(),
                Some(_) => {}
                None => files.push(DiffFile::new(path.to_owned(), Vec::new())),
            }
            header_open = false;
            continue;
        }
        if files.is_empty() {
            files.push(DiffFile::default());
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if raw.starts_with(HUNK) {
            if let Some((o, n)) = hunk_starts(raw) {
                old = o;
                new = n;
            }
            in_hunk = true;
            header_open = false;
            file.lines
                .push(DiffLine::new(LineKind::Hunk, None, raw.to_owned()));
            continue;
        }
        if !in_hunk {
            file.lines
                .push(DiffLine::new(LineKind::Meta, None, raw.to_owned()));
            continue;
        }
        let line = if let Some(body) = raw.strip_prefix('+') {
            new += 1;
            DiffLine::new(LineKind::Added, Some(new - 1), body.to_owned())
        } else if let Some(body) = raw.strip_prefix('-') {
            old += 1;
            DiffLine::new(LineKind::Removed, Some(old - 1), body.to_owned())
        } else if raw.starts_with(NO_NEWLINE) {
            DiffLine::new(LineKind::Meta, None, raw.to_owned())
        } else {
            let body = raw.strip_prefix(' ').unwrap_or(raw);
            old += 1;
            new += 1;
            let mut line = DiffLine::new(LineKind::Context, Some(new - 1), body.to_owned());
            line.old = Some(old - 1);
            line
        };
        file.lines.push(line);
    }
    files
}

#[derive(IntoElement)]
pub struct DiffView {
    id: ElementId,
    files: Vec<DiffFile>,
    sides: Option<(SharedString, SharedString)>,
}

impl DiffView {
    pub fn new(id: impl Into<ElementId>, files: Vec<DiffFile>) -> Self {
        Self {
            id: id.into(),
            files,
            sides: None,
        }
    }

    pub fn sides(mut self, old: impl Into<SharedString>, new: impl Into<SharedString>) -> Self {
        self.sides = Some((old.into(), new.into()));
        self
    }
}

fn render_line(line: &DiffLine, palette: &Palette) -> impl IntoElement {
    let (sign, sign_color, bg) = match line.kind {
        LineKind::Added => (ADDED, palette.addfg, Some(palette.addbg)),
        LineKind::Removed => (REMOVED, palette.delfg, Some(palette.delbg)),
        LineKind::Context => ("", palette.mute, None),
        LineKind::Hunk => ("", palette.mute, Some(palette.panel)),
        LineKind::Meta => ("", palette.mute, None),
    };
    let bg = if line.note.is_some() {
        Some(palette.warnbg)
    } else {
        bg
    };
    let text_color = match line.kind {
        LineKind::Hunk => palette.mute,
        LineKind::Meta => palette.mute,
        LineKind::Context => palette.fg,
        LineKind::Added => palette.fg,
        LineKind::Removed => palette.fg,
    };
    let number = line.number().map(|n| SharedString::from(n.to_string()));
    div()
        .flex()
        .min_h(CODE_LINE)
        .line_height(CODE_LINE)
        .whitespace_nowrap()
        .when_some(bg, Styled::bg)
        .child(
            div()
                .w(GUTTER_NUMBER)
                .flex_none()
                .flex()
                .justify_end()
                .pr(SPACE_4)
                .text_color(palette.mute)
                .children(number),
        )
        .child(
            div()
                .w(GUTTER_SIGN)
                .flex_none()
                .text_color(sign_color)
                .child(sign),
        )
        .child(
            div()
                .flex_none()
                .text_color(text_color)
                .child(line.text.clone()),
        )
        .when_some(line.note.clone(), |row, note| {
            row.child(
                div()
                    .flex_grow(1.)
                    .flex_basis(Pixels::ZERO)
                    .w(DIFF_NOTE_MIN)
                    .min_w(DIFF_NOTE_MIN)
                    .pr(SPACE_4)
                    .whitespace_normal()
                    .ml(SPACE_8)
                    .font_family(FONT_SANS)
                    .text_size(TEXT_MONO)
                    .text_color(palette.warn)
                    .child(SharedString::from(format!("{NOTE_ARROW}{note}"))),
            )
        })
}

fn render_file(ix: usize, file: &DiffFile, palette: &Palette) -> impl IntoElement {
    let added = file.added();
    let removed = file.removed();
    let header = div()
        .flex()
        .items_center()
        .gap(SPACE_5)
        .h(DIFF_HEADER)
        .px(SPACE_5)
        .bg(palette.panel)
        .border_b(RULE)
        .border_color(palette.line)
        .child(div().flex_1().min_w_0().truncate().child(file.path.clone()))
        .when(added > 0, |header| {
            header.child(
                div()
                    .text_color(palette.addfg)
                    .child(format!("{ADDED}{added}")),
            )
        })
        .when(removed > 0, |header| {
            header.child(
                div()
                    .text_color(palette.delfg)
                    .child(format!("{REMOVED}{removed}")),
            )
        });
    let lines = file.lines.iter().map(|line| render_line(line, palette));
    div()
        .flex()
        .flex_col()
        .border(RULE)
        .border_color(palette.line)
        .rounded(RADIUS)
        .overflow_hidden()
        .child(header)
        .child(
            div()
                .id(ix)
                .flex()
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .py(SPACE_2)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_none()
                        .min_w_full()
                        .children(lines),
                ),
        )
}

impl RenderOnce for DiffView {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let sides = self.sides.map(|(old, new)| {
            div()
                .flex()
                .gap(SPACE_8)
                .text_size(TEXT_SMALL)
                .text_color(palette.mute)
                .child(
                    div()
                        .flex()
                        .gap(SPACE_2)
                        .child(
                            div()
                                .text_color(palette.delfg)
                                .font_weight(FontWeight::MEDIUM)
                                .child(REMOVED),
                        )
                        .child(old),
                )
                .child(
                    div()
                        .flex()
                        .gap(SPACE_2)
                        .child(
                            div()
                                .text_color(palette.addfg)
                                .font_weight(FontWeight::MEDIUM)
                                .child(ADDED),
                        )
                        .child(new),
                )
        });
        let files = self
            .files
            .iter()
            .enumerate()
            .map(|(ix, file)| render_file(ix, file, &palette));
        div()
            .id(self.id)
            .flex()
            .flex_col()
            .gap(SPACE_7)
            .font_family(FONT_MONO)
            .text_size(TEXT_MONO)
            .children(sides)
            .children(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str =
        "diff --git a/apps/payments/base/deployment.yaml b/apps/payments/base/deployment.yaml
index 1a2b3c4..5d6e7f8 100644
--- a/apps/payments/base/deployment.yaml
+++ b/apps/payments/base/deployment.yaml
@@ -21,4 +21,5 @@ spec:
       containers:
-        image: ghcr.io/acme/checkout-api:2.13.4
+        image: ghcr.io/acme/checkout-api:2.14.0
+        env:
         - name: LOG_LEVEL
\\ No newline at end of file
diff --git a/old.yaml b/old.yaml
deleted file mode 100644
--- a/old.yaml
+++ /dev/null
@@ -1,1 +0,0 @@
-gone: true
";

    #[test]
    fn classifies_lines_and_numbers_them() {
        let files = parse_unified(SAMPLE);
        assert_eq!(files.len(), 2);
        let first = &files[0];
        assert_eq!(first.path.as_ref(), "apps/payments/base/deployment.yaml");
        let kinds: Vec<_> = first.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Meta,
                LineKind::Hunk,
                LineKind::Context,
                LineKind::Removed,
                LineKind::Added,
                LineKind::Added,
                LineKind::Context,
                LineKind::Meta,
            ]
        );
        let numbers: Vec<_> = first.lines.iter().map(DiffLine::number).collect();
        assert_eq!(
            numbers,
            [
                None,
                None,
                Some(21),
                Some(22),
                Some(22),
                Some(23),
                Some(24),
                None
            ]
        );
        assert_eq!((first.added(), first.removed()), (2, 1));
    }

    #[test]
    fn deleted_files_keep_their_old_path() {
        let files = parse_unified(SAMPLE);
        assert_eq!(files[1].path.as_ref(), "old.yaml");
        assert_eq!(files[1].removed(), 1);
    }

    #[test]
    fn hunk_body_without_headers_still_parses() {
        let files = parse_unified("@@ -8,3 +8,2 @@\n   files:\n-    - stripe.env\n");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].lines[2].kind, LineKind::Removed);
        assert_eq!(files[0].lines[2].old, Some(9));
    }

    #[test]
    fn malformed_hunk_headers_do_not_panic() {
        let files = parse_unified("@@ nonsense @@\n+x\n");
        assert_eq!(files[0].lines[1].kind, LineKind::Added);
    }
}
