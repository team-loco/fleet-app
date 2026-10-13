use std::ops::Range;

use fleet_core::Status;
use gpui::{
    App, ClipboardItem, ElementId, HighlightStyle, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, StyledText, Window, div,
};

use super::button::Button;
use super::overlay::{Toast, toast};
use crate::theme::{
    ActiveTheme, CODE_LINE, FONT_MONO, GUTTER_NUMBER, Palette, RADIUS, RULE, SPACE_2, SPACE_3,
    SPACE_4, TEXT_MONO,
};

const COPY: &str = "Copy";
const COPIED: &str = "Copied to the clipboard";
const COMMENT: char = '#';
const KEY_END: &str = ": ";
const KEY_END_LINE: char = ':';
const LIST_ITEM: &str = "- ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Key,
    Punctuation,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Plain,
    Yaml,
}

fn comment_start(line: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (at, c) in line.char_indices() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == COMMENT && previous.is_whitespace() => return Some(at),
            None => {}
        }
        previous = c;
    }
    None
}

pub fn yaml_tokens(line: &str) -> Vec<(Range<usize>, Token)> {
    let mut tokens = Vec::new();
    let comment = comment_start(line);
    let code_end = comment.unwrap_or(line.len());
    let code = line.get(..code_end).unwrap_or(line);
    let indent = code.len() - code.trim_start().len();
    let mut start = indent;
    if code
        .get(start..)
        .is_some_and(|rest| rest.starts_with(LIST_ITEM))
    {
        tokens.push((start..start + 1, Token::Punctuation));
        start += LIST_ITEM.len();
    }
    let body = code.get(start..).unwrap_or_default();
    let colon = body.find(KEY_END).or_else(|| {
        body.trim_end()
            .ends_with(KEY_END_LINE)
            .then(|| body.trim_end().len() - 1)
    });
    if let Some(colon) = colon {
        let key = body.get(..colon).unwrap_or_default();
        let quoted = key.starts_with('"') || key.starts_with('\'');
        if !key.is_empty() && (quoted || !key.contains(' ')) {
            tokens.push((start..start + colon, Token::Key));
            tokens.push((start + colon..start + colon + 1, Token::Punctuation));
        }
    }
    if let Some(at) = comment {
        tokens.push((at..line.len(), Token::Comment));
    }
    tokens
}

fn token_style(token: Token, palette: &Palette) -> HighlightStyle {
    let color = match token {
        Token::Key => palette.acc,
        Token::Punctuation => palette.mute,
        Token::Comment => palette.mute,
    };
    HighlightStyle {
        color: Some(color),
        ..HighlightStyle::default()
    }
}

#[derive(IntoElement)]
pub struct CodeView {
    id: ElementId,
    text: SharedString,
    language: Language,
    copy: bool,
}

impl CodeView {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            language: Language::Plain,
            copy: true,
        }
    }

    pub fn yaml(mut self) -> Self {
        self.language = Language::Yaml;
        self
    }

    pub fn without_copy(mut self) -> Self {
        self.copy = false;
        self
    }
}

impl RenderOnce for CodeView {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let numbers = self.text.lines().enumerate().map(|(ix, _)| {
            div()
                .h(CODE_LINE)
                .flex()
                .justify_end()
                .pr(SPACE_4)
                .child(SharedString::from((ix + 1).to_string()))
        });
        let lines = self.text.lines().map(|line| {
            let owned = SharedString::from(line.to_owned());
            let styled = match self.language {
                Language::Yaml => {
                    let highlights = yaml_tokens(line)
                        .into_iter()
                        .map(|(range, token)| (range, token_style(token, &palette)));
                    StyledText::new(owned).with_highlights(highlights)
                }
                Language::Plain => StyledText::new(owned),
            };
            div().h(CODE_LINE).whitespace_nowrap().child(styled)
        });
        let copy = self.copy.then(|| {
            let text = self.text.clone();
            div().absolute().top(SPACE_2).right(SPACE_2).child(
                Button::new("copy", COPY).small().on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                    toast(Toast::new(Status::Ok, COPIED), cx);
                }),
            )
        });
        div()
            .id(self.id)
            .relative()
            .flex()
            .rounded(RADIUS)
            .border(RULE)
            .border_color(palette.line)
            .bg(palette.editor)
            .py(SPACE_3)
            .font_family(FONT_MONO)
            .text_size(TEXT_MONO)
            .child(
                div()
                    .w(GUTTER_NUMBER)
                    .flex_none()
                    .flex()
                    .flex_col()
                    .text_color(palette.mute)
                    .children(numbers),
            )
            .child(
                div()
                    .id("code-scroll")
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .text_color(palette.fg)
                    .child(div().flex().flex_col().children(lines)),
            )
            .children(copy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(&str, Token)> {
        yaml_tokens(line)
            .into_iter()
            .map(|(range, token)| (&line[range], token))
            .collect()
    }

    #[test]
    fn keys_values_and_comments() {
        assert_eq!(
            kinds("  image: ghcr.io/acme/api:2.14 # pinned"),
            [
                ("image", Token::Key),
                (":", Token::Punctuation),
                ("# pinned", Token::Comment),
            ]
        );
        assert_eq!(
            kinds("- name: STRIPE_WEBHOOK_SECRET"),
            [
                ("-", Token::Punctuation),
                ("name", Token::Key),
                (":", Token::Punctuation),
            ]
        );
        assert_eq!(
            kinds("spec:"),
            [("spec", Token::Key), (":", Token::Punctuation)]
        );
    }

    #[test]
    fn hashes_inside_values_are_not_comments() {
        assert_eq!(
            kinds("url: \"http://x/#frag\""),
            [("url", Token::Key), (":", Token::Punctuation)]
        );
        assert_eq!(
            kinds("tag: a#b"),
            [("tag", Token::Key), (":", Token::Punctuation)]
        );
    }

    #[test]
    fn plain_scalars_are_left_alone() {
        assert!(
            kinds("    - app.yaml")
                .iter()
                .all(|(_, t)| *t == Token::Punctuation)
        );
        assert_eq!(kinds("just text: with spaces in key"), []);
        assert_eq!(kinds("# whole line"), [("# whole line", Token::Comment)]);
    }
}
