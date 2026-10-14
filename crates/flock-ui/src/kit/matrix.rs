use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
    div, prelude::FluentBuilder, transparent_black,
};

use crate::theme::{
    ActiveTheme, FONT_MONO, Palette, ROW_MATRIX, ROW_TABLE, ROW_TABLE_MAX, RULE, SPACE_4, SPACE_5,
    SPACE_8, TEXT_CONTROL, TEXT_LABEL, TEXT_SMALL,
};

const CELL_GROUP: &str = "matrix-cell";
const EVEN_SHARE: f32 = 1.;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub struct MatrixCell {
    content: AnyElement,
    tint: Option<Hsla>,
    on_click: Option<ClickHandler>,
}

impl MatrixCell {
    pub fn new(content: impl IntoElement) -> Self {
        Self {
            content: content.into_any_element(),
            tint: None,
            on_click: None,
        }
    }

    pub fn tint(mut self, tint: Option<Hsla>) -> Self {
        self.tint = tint;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

enum MatrixRow {
    Group {
        label: SharedString,
        count: SharedString,
    },
    Row {
        label: AnyElement,
        cells: Vec<MatrixCell>,
    },
}

#[derive(IntoElement)]
pub struct Matrix {
    id: SharedString,
    corner: SharedString,
    columns: Vec<SharedString>,
    label_min: Pixels,
    column_min: Pixels,
    rows: Vec<MatrixRow>,
}

impl Matrix {
    pub fn new(
        id: impl Into<SharedString>,
        corner: impl Into<SharedString>,
        columns: Vec<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            corner: corner.into(),
            columns,
            label_min: Pixels::ZERO,
            column_min: Pixels::ZERO,
            rows: Vec::new(),
        }
    }

    pub fn min_widths(mut self, label: Pixels, column: Pixels) -> Self {
        self.label_min = label;
        self.column_min = column;
        self
    }

    pub fn group(mut self, label: impl Into<SharedString>, count: impl Into<SharedString>) -> Self {
        self.rows.push(MatrixRow::Group {
            label: label.into(),
            count: count.into(),
        });
        self
    }

    pub fn row(mut self, label: impl IntoElement, cells: Vec<MatrixCell>) -> Self {
        self.rows.push(MatrixRow::Row {
            label: label.into_any_element(),
            cells,
        });
        self
    }

    fn label_frame(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .justify_center()
            .flex_grow(EVEN_SHARE)
            .flex_basis(Pixels::ZERO)
            .min_w(self.label_min)
            .overflow_hidden()
            .px(SPACE_8)
    }

    fn column_frame(&self, palette: &Palette) -> Div {
        div()
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .flex_grow(EVEN_SHARE)
            .flex_basis(Pixels::ZERO)
            .min_w(self.column_min)
            .h_full()
            .overflow_hidden()
            .px(SPACE_5)
            .border_l(RULE)
            .border_color(palette.line)
    }

    fn header(&self, palette: &Palette) -> AnyElement {
        let titles = self
            .columns
            .iter()
            .map(|title| self.column_frame(palette).truncate().child(title.clone()));
        div()
            .flex()
            .items_center()
            .h(ROW_TABLE_MAX)
            .border_b(RULE)
            .border_color(palette.line)
            .text_size(TEXT_CONTROL)
            .text_color(palette.mute)
            .child(self.label_frame().child(self.corner.clone()))
            .children(titles)
            .into_any_element()
    }

    fn cell(&self, index: u64, cell: MatrixCell, palette: &Palette) -> AnyElement {
        let selline = palette.selline;
        let outline = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .border(RULE)
            .border_color(transparent_black())
            .group_hover(CELL_GROUP, move |style| style.border_color(selline));
        let frame = self
            .column_frame(palette)
            .group(CELL_GROUP)
            .when_some(cell.tint, Styled::bg)
            .child(cell.content);
        match cell.on_click {
            Some(handler) => frame
                .id(ElementId::NamedInteger(self.id.clone(), index))
                .cursor_pointer()
                .on_click(move |event, window, cx| handler(event, window, cx))
                .child(outline)
                .into_any_element(),
            None => frame.into_any_element(),
        }
    }
}

fn group_row(label: &str, count: SharedString, palette: &Palette) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(SPACE_4)
        .h(ROW_TABLE)
        .px(SPACE_8)
        .bg(palette.panel)
        .border_b(RULE)
        .border_color(palette.line)
        .text_size(TEXT_LABEL)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.mute)
        .child(label.to_uppercase())
        .child(
            div()
                .font_family(FONT_MONO)
                .font_weight(FontWeight::NORMAL)
                .text_size(TEXT_SMALL)
                .child(count),
        )
        .into_any_element()
}

fn cell_index(row: usize, column: usize, columns: usize) -> u64 {
    (row * columns + column) as u64
}

impl RenderOnce for Matrix {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let header = self.header(&palette);
        let columns = self.columns.len();
        let rows = std::mem::take(&mut self.rows);
        let body: Vec<AnyElement> = rows
            .into_iter()
            .enumerate()
            .map(|(row_ix, row)| match row {
                MatrixRow::Group { label, count } => group_row(&label, count, &palette),
                MatrixRow::Row { label, cells } => {
                    let cells: Vec<AnyElement> = cells
                        .into_iter()
                        .enumerate()
                        .map(|(col, cell)| {
                            self.cell(cell_index(row_ix, col, columns), cell, &palette)
                        })
                        .collect();
                    div()
                        .flex()
                        .h(ROW_MATRIX)
                        .border_b(RULE)
                        .border_color(palette.line)
                        .child(self.label_frame().child(label))
                        .children(cells)
                        .into_any_element()
                }
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .w_full()
            .border_t(RULE)
            .border_color(palette.line)
            .child(header)
            .children(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_indexes_are_unique_across_rows() {
        let columns = 4;
        let mut seen: Vec<u64> = (0..3)
            .flat_map(|row| (0..columns).map(move |col| cell_index(row, col, columns)))
            .collect();
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total);
    }
}
