use fleet_core::Status;
use gpui::{
    AnyElement, App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder,
};

use super::status::StatusText;
use super::text::SectionLabel;
use crate::theme::{
    ActiveTheme, FONT_MONO, INSPECTOR, RULE, SPACE_3, SPACE_4, SPACE_5, SPACE_6, TEXT_BODY, TEXT_UI,
};

#[derive(IntoElement)]
pub struct Inspector {
    id: ElementId,
    title: SharedString,
    mono: bool,
    status: Option<(Status, SharedString)>,
    actions: Option<AnyElement>,
    width: Pixels,
    sections: Vec<(Option<SharedString>, AnyElement)>,
}

impl Inspector {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            mono: false,
            status: None,
            actions: None,
            width: INSPECTOR,
            sections: Vec::new(),
        }
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn status(mut self, status: Status, label: impl Into<SharedString>) -> Self {
        self.status = Some((status, label.into()));
        self
    }

    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn section(mut self, label: Option<&str>, content: impl IntoElement) -> Self {
        let label = label.map(|l| SharedString::from(l.to_owned()));
        self.sections.push((label, content.into_any_element()));
        self
    }
}

impl RenderOnce for Inspector {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = cx.palette();
        let line = palette.line;
        let title = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(TEXT_UI)
            .font_weight(FontWeight::SEMIBOLD)
            .when(self.mono, |title| title.font_family(FONT_MONO))
            .child(self.title);
        let header = div()
            .flex()
            .flex_col()
            .gap(SPACE_3)
            .p(SPACE_6)
            .border_b(RULE)
            .border_color(line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(SPACE_4)
                    .child(title)
                    .children(self.actions),
            )
            .when_some(self.status, |header, (status, label)| {
                header.child(
                    div()
                        .text_size(TEXT_BODY)
                        .child(StatusText::new(status, label)),
                )
            });
        let sections = self.sections.into_iter().map(|(label, content)| {
            div()
                .flex()
                .flex_col()
                .gap(SPACE_5)
                .p(SPACE_6)
                .border_b(RULE)
                .border_color(line)
                .when_some(label, |section, label| {
                    section.child(SectionLabel::new(label))
                })
                .child(content)
        });
        div()
            .id(self.id)
            .w(self.width)
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .border_l(RULE)
            .border_color(line)
            .bg(palette.panel)
            .text_size(TEXT_BODY)
            .child(header)
            .children(sections)
    }
}
