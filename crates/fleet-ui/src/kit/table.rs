use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use fleet_core::Status;
use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, Context, Div, ElementId, EventEmitter,
    FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement, KeyDownEvent,
    ListSizingBehavior, ParentElement, Pixels, Render, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, UniformListScrollHandle, Window, div, ease_in_out,
    prelude::FluentBuilder, uniform_list,
};

use crate::theme::{
    ActiveTheme, FLASH, FONT_MONO, Palette, ROW_TABLE, ROW_TABLE_MIN, RULE, SPACE_6, SPACE_8,
    TEXT_BODY, TEXT_MONO, TEXT_SMALL,
};

const SORT_ASCENDING: &str = "↑";
const SORT_DESCENDING: &str = "↓";
const FLASH_STRENGTH: f32 = 0.35;

pub type CellRenderer<R> = Rc<dyn Fn(&R, &Palette) -> AnyElement>;
pub type Comparator<R> = Rc<dyn Fn(&R, &R) -> Ordering>;
type RowStatus<R> = Rc<dyn Fn(&R) -> Option<Status>>;
type RowKey<R> = Rc<dyn Fn(&R) -> SharedString>;
type SpanRow<R> = Rc<dyn Fn(&R, &Palette) -> Option<AnyElement>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    Fixed(Pixels),
    Flex { weight: f32, min: Pixels },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

pub struct Column<R> {
    title: SharedString,
    width: Width,
    align: Align,
    mono: bool,
    sort: Option<Comparator<R>>,
    cell: CellRenderer<R>,
}

impl<R: 'static> Column<R> {
    pub fn new(
        title: impl Into<SharedString>,
        cell: impl Fn(&R, &Palette) -> AnyElement + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            width: Width::Flex {
                weight: 1.,
                min: Pixels::ZERO,
            },
            align: Align::Start,
            mono: false,
            sort: None,
            cell: Rc::new(cell),
        }
    }

    pub fn text(
        title: impl Into<SharedString>,
        text: impl Fn(&R) -> SharedString + 'static,
    ) -> Self {
        Self::new(title, move |row, _| {
            div().truncate().child(text(row)).into_any_element()
        })
    }

    pub fn colored_text(
        title: impl Into<SharedString>,
        text: impl Fn(&R) -> SharedString + 'static,
        color: impl Fn(&R, &Palette) -> Hsla + 'static,
    ) -> Self {
        Self::new(title, move |row, palette| {
            div()
                .truncate()
                .text_color(color(row, palette))
                .child(text(row))
                .into_any_element()
        })
    }

    pub fn fixed(mut self, width: Pixels) -> Self {
        self.width = Width::Fixed(width);
        self
    }

    pub fn flex(mut self, weight: f32, min: Pixels) -> Self {
        self.width = Width::Flex { weight, min };
        self
    }

    pub fn end(mut self) -> Self {
        self.align = Align::End;
        self
    }

    pub fn center(mut self) -> Self {
        self.align = Align::Center;
        self
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn sortable(mut self, compare: impl Fn(&R, &R) -> Ordering + 'static) -> Self {
        self.sort = Some(Rc::new(compare));
        self
    }

    pub fn sort_by_key<K: Ord>(self, key: impl Fn(&R) -> K + 'static) -> Self {
        self.sortable(move |a, b| key(a).cmp(&key(b)))
    }
}

fn cell_frame(width: Width, align: Align) -> Div {
    let cell = div()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap();
    let cell = match width {
        Width::Fixed(width) => cell.w(width).flex_none(),
        Width::Flex { weight, min } => cell.flex_grow(weight).flex_basis(Pixels::ZERO).min_w(min),
    };
    match align {
        Align::Start => cell,
        Align::Center => cell.justify_center(),
        Align::End => cell.justify_end(),
    }
}

pub fn sort_order<R>(
    rows: &[R],
    compare: &dyn Fn(&R, &R) -> Ordering,
    direction: SortDirection,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&a, &b| {
        let ordering = compare(&rows[a], &rows[b]);
        match direction {
            SortDirection::Ascending => ordering,
            SortDirection::Descending => ordering.reverse(),
        }
    });
    order
}

pub fn next_sort(
    current: Option<(usize, SortDirection)>,
    column: usize,
) -> Option<(usize, SortDirection)> {
    match current {
        Some((col, SortDirection::Ascending)) if col == column => {
            Some((column, SortDirection::Descending))
        }
        Some((col, SortDirection::Descending)) if col == column => None,
        Some(_) => Some((column, SortDirection::Ascending)),
        None => Some((column, SortDirection::Ascending)),
    }
}

pub fn changed_statuses(
    before: &HashMap<SharedString, Status>,
    after: &[(SharedString, Status)],
) -> Vec<SharedString> {
    after
        .iter()
        .filter(|(key, status)| before.get(key).is_some_and(|old| old != status))
        .map(|(key, _)| key.clone())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEvent {
    Selected(usize),
    Opened(usize),
}

pub struct Table<R: 'static> {
    id: SharedString,
    columns: Rc<Vec<Column<R>>>,
    rows: Rc<Vec<R>>,
    order: Vec<usize>,
    sort: Option<(usize, SortDirection)>,
    selected: Option<usize>,
    row_height: Pixels,
    column_gap: Pixels,
    inset: Pixels,
    span: Option<SpanRow<R>>,
    click_opens: bool,
    fit: bool,
    header: bool,
    status: Option<RowStatus<R>>,
    key: Option<RowKey<R>>,
    flashes: HashMap<SharedString, u64>,
    flash_generation: u64,
    empty: SharedString,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl<R: 'static> EventEmitter<TableEvent> for Table<R> {}

impl<R: 'static> Table<R> {
    pub fn new(
        id: impl Into<SharedString>,
        columns: Vec<Column<R>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            id: id.into(),
            columns: Rc::new(columns),
            rows: Rc::new(Vec::new()),
            order: Vec::new(),
            sort: None,
            selected: None,
            row_height: ROW_TABLE,
            column_gap: SPACE_6,
            inset: SPACE_8,
            span: None,
            click_opens: false,
            fit: false,
            header: true,
            status: None,
            key: None,
            flashes: HashMap::new(),
            flash_generation: 0,
            empty: SharedString::default(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    pub fn rows(mut self, rows: Vec<R>) -> Self {
        self.rows = Rc::new(rows);
        self.resort();
        self
    }

    pub fn row_height(mut self, height: Pixels) -> Self {
        self.row_height = height;
        self
    }

    pub fn column_gap(mut self, gap: Pixels) -> Self {
        self.column_gap = gap;
        self
    }

    pub fn inset(mut self, inset: Pixels) -> Self {
        self.inset = inset;
        self
    }

    pub fn span_rows(
        mut self,
        span: impl Fn(&R, &Palette) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.span = Some(Rc::new(span));
        self
    }

    pub fn click_opens(mut self) -> Self {
        self.click_opens = true;
        self
    }

    pub fn fit_content(mut self) -> Self {
        self.fit = true;
        self
    }

    pub fn headerless(mut self) -> Self {
        self.header = false;
        self
    }

    pub fn empty(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    pub fn row_status(mut self, status: impl Fn(&R) -> Option<Status> + 'static) -> Self {
        self.status = Some(Rc::new(status));
        self
    }

    pub fn row_key(mut self, key: impl Fn(&R) -> SharedString + 'static) -> Self {
        self.key = Some(Rc::new(key));
        self
    }

    pub fn sorted_by(mut self, column: usize, direction: SortDirection) -> Self {
        self.sort = Some((column, direction));
        self.resort();
        self
    }

    pub fn row(&self, index: usize) -> Option<&R> {
        self.rows.get(index)
    }

    pub fn selected_row(&self) -> Option<&R> {
        self.selected.and_then(|ix| self.rows.get(ix))
    }

    pub fn select(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.selected = index.filter(|&ix| ix < self.rows.len());
        if let Some(position) = self.position_of(self.selected) {
            self.scroll.scroll_to_item(position, ScrollStrategy::Center);
        }
        cx.notify();
    }

    pub fn set_columns(&mut self, columns: Vec<Column<R>>, cx: &mut Context<Self>) {
        self.columns = Rc::new(columns);
        self.sort = None;
        self.resort();
        cx.notify();
    }

    pub fn set_rows(&mut self, rows: Vec<R>, cx: &mut Context<Self>) {
        let selected_key = self.selected_key();
        self.record_flashes(&rows);
        self.rows = Rc::new(rows);
        self.selected = match (selected_key, &self.key) {
            (Some(wanted), Some(key)) => self.rows.iter().position(|row| key(row) == wanted),
            _ => self.selected.filter(|&ix| ix < self.rows.len()),
        };
        self.resort();
        cx.notify();
    }

    fn selected_key(&self) -> Option<SharedString> {
        let key = self.key.as_ref()?;
        self.selected_row().map(|row| key(row))
    }

    fn record_flashes(&mut self, next: &[R]) {
        let (Some(key), Some(status)) = (&self.key, &self.status) else {
            return;
        };
        let before: HashMap<SharedString, Status> = self
            .rows
            .iter()
            .filter_map(|row| status(row).map(|s| (key(row), s)))
            .collect();
        let after: Vec<(SharedString, Status)> = next
            .iter()
            .filter_map(|row| status(row).map(|s| (key(row), s)))
            .collect();
        for changed in changed_statuses(&before, &after) {
            self.flash_generation += 1;
            self.flashes.insert(changed, self.flash_generation);
        }
    }

    fn resort(&mut self) {
        let comparator = self.sort.and_then(|(col, dir)| {
            self.columns
                .get(col)
                .and_then(|c| c.sort.clone())
                .map(|c| (c, dir))
        });
        self.order = match comparator {
            Some((compare, direction)) => sort_order(&self.rows, compare.as_ref(), direction),
            None => (0..self.rows.len()).collect(),
        };
    }

    fn position_of(&self, row: Option<usize>) -> Option<usize> {
        let row = row?;
        self.order.iter().position(|&ix| ix == row)
    }

    fn toggle_sort(&mut self, column: usize, cx: &mut Context<Self>) {
        self.sort = next_sort(self.sort, column);
        self.resort();
        cx.notify();
    }

    fn click_row(
        &mut self,
        row: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.selected = Some(row);
        cx.emit(TableEvent::Selected(row));
        if self.click_opens || event.click_count() > 1 {
            cx.emit(TableEvent::Opened(row));
        }
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let position = self.position_of(self.selected);
        let last = self.order.len().checked_sub(1);
        let target = match event.keystroke.key.as_str() {
            "down" => position.map_or(Some(0), |p| Some((p + 1).min(last.unwrap_or(0)))),
            "up" => position.map_or(last, |p| Some(p.saturating_sub(1))),
            "enter" => {
                if let Some(row) = self.selected {
                    cx.emit(TableEvent::Opened(row));
                }
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        let Some(row) = target.and_then(|p| self.order.get(p).copied()) else {
            return;
        };
        self.selected = Some(row);
        if let Some(position) = self.position_of(Some(row)) {
            self.scroll
                .scroll_to_item(position, ScrollStrategy::Nearest);
        }
        cx.emit(TableEvent::Selected(row));
        cx.notify();
    }

    fn render_header(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let cells: Vec<AnyElement> = self
            .columns
            .iter()
            .enumerate()
            .map(|(ix, column)| {
                let arrow = match self.sort {
                    Some((col, SortDirection::Ascending)) if col == ix => Some(SORT_ASCENDING),
                    Some((col, SortDirection::Descending)) if col == ix => Some(SORT_DESCENDING),
                    Some(_) => None,
                    None => None,
                };
                let frame = cell_frame(column.width, column.align)
                    .id(ix)
                    .child(column.title.clone())
                    .children(arrow);
                if column.sort.is_some() {
                    let fg = palette.fg;
                    frame
                        .cursor_pointer()
                        .hover(move |style| style.text_color(fg))
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_sort(ix, cx)))
                        .into_any_element()
                } else {
                    frame.into_any_element()
                }
            })
            .collect();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(self.column_gap)
            .h(ROW_TABLE_MIN)
            .px(SPACE_8)
            .border_b(RULE)
            .border_color(palette.line)
            .text_size(TEXT_SMALL)
            .text_color(palette.mute)
            .children(cells)
            .into_any_element()
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let palette = *cx.palette();
        let columns = self.columns.clone();
        let rows = self.rows.clone();
        range
            .filter_map(|position| {
                let ix = *self.order.get(position)?;
                let row = rows.get(ix)?;
                Some(self.render_row(ix, row, &columns, &palette, cx))
            })
            .collect()
    }

    fn render_row(
        &self,
        ix: usize,
        row: &R,
        columns: &[Column<R>],
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(content) = self.span.as_ref().and_then(|span| span(row, palette)) {
            return div()
                .flex()
                .items_center()
                .w_full()
                .h(self.row_height)
                .border_b(RULE)
                .border_color(palette.line)
                .child(content)
                .into_any_element();
        }
        let selected = self.selected == Some(ix);
        let status = self.status.as_ref().and_then(|status| status(row));
        let tint = status.and_then(|status| palette.tint(status));
        let hover = palette.hover;
        let cells = columns.iter().map(|column| {
            cell_frame(column.width, column.align)
                .when(column.mono, |cell| {
                    cell.font_family(FONT_MONO).text_size(TEXT_MONO)
                })
                .child((column.cell)(row, palette))
        });
        let element = div()
            .id(ix)
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(self.column_gap)
            .h(self.row_height)
            .pl(self.inset)
            .pr(SPACE_8)
            .border_b(RULE)
            .border_color(palette.line)
            .text_size(TEXT_BODY)
            .text_color(palette.fg)
            .cursor_pointer()
            .when_some(tint, Styled::bg)
            .when(selected, |row| {
                row.bg(palette.sel).child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .border(RULE)
                        .border_color(palette.selline),
                )
            })
            .when(!selected, |row| row.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, event, window, cx| {
                this.click_row(ix, event, window, cx);
            }))
            .children(cells);
        let flash = self
            .key
            .as_ref()
            .and_then(|key| self.flashes.get(&key(row)).copied())
            .zip(status);
        match flash {
            Some((generation, status)) if !selected => {
                let from = palette.status(status).opacity(FLASH_STRENGTH);
                let to = tint.unwrap_or(palette.editor);
                let animation = Animation::new(FLASH).with_easing(ease_in_out);
                let key = ElementId::NamedInteger(self.id.clone(), generation);
                element
                    .with_animation(key, animation, move |row, delta| {
                        row.bg(mix(from, to, delta))
                    })
                    .into_any_element()
            }
            Some(_) => element.into_any_element(),
            None => element.into_any_element(),
        }
    }
}

fn mix(from: Hsla, to: Hsla, delta: f32) -> Hsla {
    let from = from.to_rgb();
    let to = to.to_rgb();
    let lerp = |a: f32, b: f32| a + (b - a) * delta;
    gpui::Rgba {
        r: lerp(from.r, to.r),
        g: lerp(from.g, to.g),
        b: lerp(from.b, to.b),
        a: lerp(from.a, to.a),
    }
    .into()
}

impl<R: 'static> Focusable for Table<R> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl<R: 'static> Render for Table<R> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = *cx.palette();
        let header = self.header.then(|| self.render_header(&palette, cx));
        let body = if self.rows.is_empty() {
            div()
                .flex()
                .items_center()
                .h(self.row_height)
                .px(SPACE_8)
                .border_b(RULE)
                .border_color(palette.line)
                .text_size(TEXT_BODY)
                .text_color(palette.mute)
                .child(self.empty.clone())
                .into_any_element()
        } else {
            let list = uniform_list(
                ElementId::Name(self.id.clone()),
                self.order.len(),
                cx.processor(Self::render_rows),
            )
            .track_scroll(&self.scroll);
            if self.fit {
                list.with_sizing_behavior(ListSizingBehavior::Infer)
                    .w_full()
                    .into_any_element()
            } else {
                list.flex_1().min_h_0().into_any_element()
            }
        };
        div()
            .id(ElementId::NamedInteger(self.id.clone(), 0))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .flex()
            .flex_col()
            .when(!self.fit, Styled::size_full)
            .when(self.fit, Styled::w_full)
            .children(header)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_stably_in_both_directions() {
        let rows = [("b", 2), ("a", 1), ("c", 2), ("d", 0)];
        let by_number = |a: &(&str, i32), b: &(&str, i32)| a.1.cmp(&b.1);
        assert_eq!(
            sort_order(&rows, &by_number, SortDirection::Ascending),
            [3, 1, 0, 2]
        );
        assert_eq!(
            sort_order(&rows, &by_number, SortDirection::Descending),
            [0, 2, 1, 3]
        );
    }

    #[test]
    fn header_clicks_cycle_ascending_descending_off() {
        let first = next_sort(None, 2);
        assert_eq!(first, Some((2, SortDirection::Ascending)));
        let second = next_sort(first, 2);
        assert_eq!(second, Some((2, SortDirection::Descending)));
        assert_eq!(next_sort(second, 2), None);
        assert_eq!(next_sort(second, 0), Some((0, SortDirection::Ascending)));
    }

    #[test]
    fn flags_only_rows_whose_status_changed() {
        let before = HashMap::from([
            (SharedString::from("a"), Status::Ok),
            (SharedString::from("b"), Status::Warn),
        ]);
        let after = [
            (SharedString::from("a"), Status::Err),
            (SharedString::from("b"), Status::Warn),
            (SharedString::from("c"), Status::Err),
        ];
        assert_eq!(changed_statuses(&before, &after), [SharedString::from("a")]);
    }
}
