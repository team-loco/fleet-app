use fleet_core::Status;
use gpui::{
    AnimationExt, AnyElement, App, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, Styled, Window, div,
};

use super::{Button, StatusText};
use crate::theme::{
    self, ActiveTheme, CHART_HEIGHT, Palette, ROW_TABLE, RULE, SKELETON_CHART_WIDTH,
    SKELETON_METER_HEIGHT, SKELETON_METER_WIDTH, SKELETON_NAME_WIDTH, SKELETON_NAMESPACE_WIDTH,
    SKELETON_ROWS, SKELETON_SHIMMER, SKELETON_SHIMMER_OPACITY, SKELETON_SHIMMER_WIDTH,
    SKELETON_STATUS_WIDTH, SKELETON_TITLE_WIDTH, SPACE_4, SPACE_5, SPACE_8, TEXT_LABEL, TEXT_TITLE,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Rows,
    Blocks,
}

#[derive(IntoElement)]
pub struct Skeleton {
    shape: Shape,
}

impl Skeleton {
    pub fn rows() -> Self {
        Self { shape: Shape::Rows }
    }

    pub fn blocks() -> Self {
        Self {
            shape: Shape::Blocks,
        }
    }
}

fn block(id: String, width: Pixels, height: Pixels, palette: &Palette, still: bool) -> AnyElement {
    let base = div()
        .relative()
        .overflow_hidden()
        .w(width)
        .h(height)
        .rounded(theme::RADIUS_SMALL)
        .bg(palette.line);
    if still {
        return base.into_any_element();
    }
    let travel = width + SKELETON_SHIMMER_WIDTH;
    let shine = div()
        .absolute()
        .top_0()
        .h_full()
        .w(SKELETON_SHIMMER_WIDTH)
        .bg(palette.line2)
        .opacity(SKELETON_SHIMMER_OPACITY)
        .with_animation(
            id,
            gpui::Animation::new(SKELETON_SHIMMER).repeat_synced(),
            move |shine, delta| shine.left(travel * delta - SKELETON_SHIMMER_WIDTH),
        );
    base.child(shine).into_any_element()
}

impl RenderOnce for Skeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = *cx.palette();
        let still = cx.reduce_motion();
        let mut next = 0_usize;
        let mut piece = |width, height| {
            next += 1;
            block(format!("skeleton-{next}"), width, height, &palette, still)
        };
        let content = match self.shape {
            Shape::Rows => div().flex().flex_col().children(
                (0..SKELETON_ROWS)
                    .map(|_| {
                        div()
                            .flex()
                            .items_center()
                            .gap(SPACE_8)
                            .h(ROW_TABLE)
                            .border_b(RULE)
                            .border_color(palette.line)
                            .child(piece(SKELETON_NAME_WIDTH, TEXT_LABEL))
                            .child(piece(SKELETON_NAMESPACE_WIDTH, TEXT_LABEL))
                            .child(piece(SKELETON_STATUS_WIDTH, TEXT_LABEL))
                    })
                    .collect::<Vec<_>>(),
            ),
            Shape::Blocks => div()
                .flex()
                .flex_col()
                .gap(SPACE_8)
                .child(piece(SKELETON_TITLE_WIDTH, TEXT_TITLE))
                .child(
                    div()
                        .flex()
                        .gap(SPACE_8)
                        .child(piece(SKELETON_METER_WIDTH, SKELETON_METER_HEIGHT))
                        .child(piece(SKELETON_METER_WIDTH, SKELETON_METER_HEIGHT))
                        .child(piece(SKELETON_METER_WIDTH, SKELETON_METER_HEIGHT)),
                )
                .child(piece(SKELETON_CHART_WIDTH, CHART_HEIGHT)),
        };
        div()
            .debug_selector(|| "loading-skeleton".to_owned())
            .px(SPACE_8)
            .py(SPACE_5)
            .child(content)
    }
}

#[derive(IntoElement)]
pub struct ConnectionProgress {
    cluster: SharedString,
    step: SharedString,
}

impl ConnectionProgress {
    pub fn new(cluster: impl Into<SharedString>, step: impl Into<SharedString>) -> Self {
        Self {
            cluster: cluster.into(),
            step: step.into(),
        }
    }
}

impl RenderOnce for ConnectionProgress {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let title = format!("Connecting to {}", self.cluster);
        div()
            .debug_selector(|| "connection-progress".to_owned())
            .flex()
            .flex_col()
            .gap(SPACE_4)
            .px(SPACE_8)
            .pt(SPACE_8)
            .child(StatusText::new(Status::Progress, title))
            .child(div().text_color(cx.palette().fg2).child(self.step))
    }
}

#[derive(IntoElement)]
pub struct EmptyState {
    sentence: SharedString,
    action: Button,
}

impl EmptyState {
    pub fn new(sentence: impl Into<SharedString>, action: Button) -> Self {
        Self {
            sentence: sentence.into(),
            action,
        }
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .debug_selector(|| "empty-state".to_owned())
            .flex()
            .flex_col()
            .items_start()
            .gap(SPACE_5)
            .px(SPACE_8)
            .pt(SPACE_8)
            .text_color(cx.palette().fg2)
            .child(self.sentence)
            .child(self.action)
    }
}

#[derive(IntoElement)]
pub struct ErrorState {
    reason: SharedString,
    retry: Button,
}

impl ErrorState {
    pub fn new(reason: &str, retry: Button) -> Self {
        let words: Vec<&str> = reason.split_whitespace().collect();
        Self {
            reason: words.join(" ").into(),
            retry,
        }
    }
}

impl RenderOnce for ErrorState {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .debug_selector(|| "error-state".to_owned())
            .flex()
            .flex_col()
            .items_start()
            .gap(SPACE_5)
            .px(SPACE_8)
            .pt(SPACE_8)
            .child(StatusText::new(Status::Err, self.reason))
            .child(self.retry)
    }
}
