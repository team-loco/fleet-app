use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    Animation, AnimationExt, AnyElement, App, Axis, Bounds, CursorStyle, ElementId, Entity, Global,
    Hsla, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels,
    RenderOnce, SharedString, Styled, Window, canvas, div, ease_out_quint, prelude::FluentBuilder,
};

use crate::theme::{ActiveTheme, COLLAPSE, RULE, SPLIT_MIN, SPLITTER};

const DEFAULT_RATIO: f32 = 0.6;

#[derive(Default)]
struct SplitRatios(HashMap<SharedString, f32>);

impl Global for SplitRatios {}

pub type OnResize = Rc<dyn Fn(f32, &mut Window, &mut App)>;
pub type OnResizeEnd = Rc<dyn Fn(&mut Window, &mut App)>;

struct SplitState {
    bounds: Bounds<Pixels>,
    dragging: bool,
    collapsed: bool,
    generation: u64,
}

pub fn clamp_ratio(ratio: f32, total: Pixels, min: Pixels) -> f32 {
    let total = f32::from(total);
    let min = f32::from(min);
    if !ratio.is_finite() || total <= 0. {
        return DEFAULT_RATIO;
    }
    let lowest = (min / total).min(0.5);
    ratio.clamp(lowest, 1. - lowest)
}

#[derive(IntoElement)]
pub struct Split {
    id: SharedString,
    axis: Axis,
    first: AnyElement,
    second: AnyElement,
    ratio: f32,
    collapsed: bool,
    line: Option<Hsla>,
    on_resize: Option<OnResize>,
    on_resize_end: Option<OnResizeEnd>,
}

impl Split {
    pub fn horizontal(
        id: impl Into<SharedString>,
        first: impl IntoElement,
        second: impl IntoElement,
    ) -> Self {
        Self::new(id, Axis::Horizontal, first, second)
    }

    pub fn vertical(
        id: impl Into<SharedString>,
        first: impl IntoElement,
        second: impl IntoElement,
    ) -> Self {
        Self::new(id, Axis::Vertical, first, second)
    }

    fn new(
        id: impl Into<SharedString>,
        axis: Axis,
        first: impl IntoElement,
        second: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            axis,
            first: first.into_any_element(),
            second: second.into_any_element(),
            ratio: DEFAULT_RATIO,
            collapsed: false,
            line: None,
            on_resize: None,
            on_resize_end: None,
        }
    }

    pub fn ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio;
        self
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn line(mut self, color: Hsla) -> Self {
        self.line = Some(color);
        self
    }

    pub fn on_resize(mut self, on_resize: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_resize = Some(Rc::new(on_resize));
        self
    }

    pub fn on_resize_end(
        mut self,
        on_resize_end: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_resize_end = Some(Rc::new(on_resize_end));
        self
    }
}

fn stored_ratio(id: &SharedString, fallback: f32, cx: &App) -> f32 {
    cx.try_global::<SplitRatios>()
        .and_then(|ratios| ratios.0.get(id).copied())
        .unwrap_or(fallback)
}

fn store_ratio(id: SharedString, ratio: f32, cx: &mut App) {
    cx.default_global::<SplitRatios>().0.insert(id, ratio);
}

fn divider(
    selector: String,
    axis: Axis,
    state: &Entity<SplitState>,
    color: Hsla,
) -> impl IntoElement {
    let state = state.clone();
    let cursor = match axis {
        Axis::Horizontal => CursorStyle::ResizeLeftRight,
        Axis::Vertical => CursorStyle::ResizeUpDown,
    };
    let hit = div()
        .debug_selector(move || selector.clone())
        .id("splitter-hit")
        .absolute()
        .cursor(cursor)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            state.update(cx, |state, _| state.dragging = true);
            cx.stop_propagation();
        });
    let hit = match axis {
        Axis::Horizontal => hit.top_0().bottom_0().left(-(SPLITTER / 2.)).w(SPLITTER),
        Axis::Vertical => hit.left_0().right_0().top(-(SPLITTER / 2.)).h(SPLITTER),
    };
    let line = div().relative().flex_none().bg(color).child(hit);
    match axis {
        Axis::Horizontal => line.w(RULE).h_full(),
        Axis::Vertical => line.h(RULE).w_full(),
    }
}

fn end_drag(
    state: &Entity<SplitState>,
    on_end: Option<&OnResizeEnd>,
    window: &mut Window,
    cx: &mut App,
) {
    let was_dragging = state.update(cx, |state, _| std::mem::replace(&mut state.dragging, false));
    if was_dragging && let Some(on_end) = on_end {
        on_end(window, cx);
    }
}

fn drag_ratio(
    state: &Entity<SplitState>,
    axis: Axis,
    event: &MouseMoveEvent,
    on_end: Option<&OnResizeEnd>,
    window: &mut Window,
    cx: &mut App,
) -> Option<f32> {
    let (dragging, bounds) = {
        let state = state.read(cx);
        (state.dragging, state.bounds)
    };
    if !dragging {
        return None;
    }
    if !event.dragging() {
        end_drag(state, on_end, window, cx);
        return None;
    }
    let (offset, total) = match axis {
        Axis::Horizontal => (event.position.x - bounds.origin.x, bounds.size.width),
        Axis::Vertical => (event.position.y - bounds.origin.y, bounds.size.height),
    };
    Some(clamp_ratio(offset / total, total, SPLIT_MIN))
}

impl RenderOnce for Split {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let id = self.id.clone();
        let axis = self.axis;
        let state = window.use_keyed_state(ElementId::Name(id.clone()), cx, |_, _| SplitState {
            bounds: Bounds::default(),
            dragging: false,
            collapsed: self.collapsed,
            generation: 0,
        });
        let changed = state.update(cx, |state, _| {
            let changed = state.collapsed != self.collapsed;
            if changed {
                state.collapsed = self.collapsed;
                state.generation += 1;
            }
            changed.then_some(state.generation)
        });
        let ratio = match &self.on_resize {
            Some(_) => self.ratio,
            None => stored_ratio(&id, self.ratio, cx),
        };
        let first_grow = ratio;
        let second_grow = if self.collapsed { 0. } else { 1. - ratio };
        let recorder = {
            let state = state.clone();
            canvas(
                move |bounds, _, cx| state.update(cx, |state, _| state.bounds = bounds),
                |_, (), _, _| {},
            )
            .absolute()
            .size_full()
        };
        let handle = divider(
            format!("{id}-divider"),
            axis,
            &state,
            self.line.unwrap_or(palette.line),
        );
        let first = div()
            .flex_basis(Pixels::ZERO)
            .flex_grow(first_grow)
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(self.first);
        let second = div()
            .flex_basis(Pixels::ZERO)
            .flex_grow(second_grow)
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(self.second);
        let second = match changed {
            Some(generation) => {
                let (from, to) = if self.collapsed {
                    (1. - ratio, 0.)
                } else {
                    (0., 1. - ratio)
                };
                let animation = Animation::new(COLLAPSE).with_easing(ease_out_quint());
                second
                    .with_animation(
                        ElementId::NamedInteger(id.clone(), generation),
                        animation,
                        move |pane, delta| pane.flex_grow(from + (to - from) * delta),
                    )
                    .into_any_element()
            }
            None => second.into_any_element(),
        };
        let moving = state.clone();
        let releasing = state.clone();
        let store_id = id.clone();
        let on_resize = self.on_resize.clone();
        let moved_end = self.on_resize_end.clone();
        let released_end = self.on_resize_end.clone();
        div()
            .id(ElementId::NamedInteger(id, 0))
            .relative()
            .size_full()
            .flex()
            .when(axis == Axis::Vertical, Styled::flex_col)
            .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                let resized = drag_ratio(&moving, axis, event, moved_end.as_ref(), window, cx);
                if let Some(ratio) = resized {
                    match &on_resize {
                        Some(on_resize) => on_resize(ratio, window, cx),
                        None => store_ratio(store_id.clone(), ratio, cx),
                    }
                    window.refresh();
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                end_drag(&releasing, released_end.as_ref(), window, cx);
            })
            .child(recorder)
            .child(first)
            .when(!self.collapsed, |split| split.child(handle))
            .child(second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    #[test]
    fn keeps_both_panes_above_the_minimum() {
        assert!((clamp_ratio(0.05, px(1000.), px(160.)) - 0.16).abs() < 1e-6);
        assert!((clamp_ratio(0.99, px(1000.), px(160.)) - 0.84).abs() < 1e-6);
        assert!((clamp_ratio(0.4, px(1000.), px(160.)) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn degenerate_sizes_fall_back() {
        assert!((clamp_ratio(0.3, px(0.), px(160.)) - DEFAULT_RATIO).abs() < 1e-6);
        assert!((clamp_ratio(f32::NAN, px(500.), px(160.)) - DEFAULT_RATIO).abs() < 1e-6);
        assert!((clamp_ratio(0.9, px(200.), px(160.)) - 0.5).abs() < 1e-6);
    }
}
