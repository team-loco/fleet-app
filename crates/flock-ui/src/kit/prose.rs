use std::ops::Range;
use std::rc::Rc;

use gpui::{App, ElementId, HighlightStyle, IntoElement, RenderOnce, SharedString, Window};

use super::selectable::SelectableText;
use crate::theme::ActiveTheme;

type LinkHandler = Rc<dyn Fn(&mut Window, &mut App)>;

enum Part {
    Text(SharedString),
    Link(SharedString, Option<LinkHandler>),
}

#[derive(IntoElement)]
pub struct Prose {
    id: ElementId,
    parts: Vec<Part>,
}

impl Prose {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            parts: Vec::new(),
        }
    }

    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.parts.push(Part::Text(text.into()));
        self
    }

    pub fn link(
        mut self,
        text: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.parts
            .push(Part::Link(text.into(), Some(Rc::new(handler))));
        self
    }

    pub fn inert_link(mut self, text: impl Into<SharedString>) -> Self {
        self.parts.push(Part::Link(text.into(), None));
        self
    }
}

fn link_ranges<'a>(
    parts: impl IntoIterator<Item = (&'a str, bool)>,
) -> (String, Vec<Range<usize>>) {
    let mut text = String::new();
    let mut links = Vec::new();
    for (part, is_link) in parts {
        let start = text.len();
        text.push_str(part);
        if is_link {
            links.push(start..text.len());
        }
    }
    (text, links)
}

impl RenderOnce for Prose {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let acc = cx.palette().acc;
        let pieces = self.parts.iter().map(|part| match part {
            Part::Text(text) => (text.as_ref(), false),
            Part::Link(text, _) => (text.as_ref(), true),
        });
        let (text, ranges) = link_ranges(pieces);
        let handlers = self.parts.into_iter().filter_map(|part| match part {
            Part::Text(_) => None,
            Part::Link(_, handler) => Some(handler),
        });
        let link_style = HighlightStyle {
            color: Some(acc),
            ..HighlightStyle::default()
        };
        let highlights = ranges.iter().map(|range| (range.clone(), link_style));
        let selectable = ranges.iter().cloned().fold(
            SelectableText::new(text).id(self.id).highlights(highlights),
            SelectableText::mono,
        );
        ranges
            .into_iter()
            .zip(handlers)
            .fold(selectable, |selectable, (range, handler)| match handler {
                Some(handler) => selectable.link(range, move |window, cx| handler(window, cx)),
                None => selectable,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_ranges_cover_only_links() {
        let (text, links) = link_ranges([
            ("applied ", false),
            ("a41e07c", true),
            (" 3h ago · last healthy ", false),
            ("7c2d9e0", true),
        ]);
        assert_eq!(text, "applied a41e07c 3h ago · last healthy 7c2d9e0");
        let linked: Vec<_> = links.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(linked, ["a41e07c", "7c2d9e0"]);
    }
}
