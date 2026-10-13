use fleet_core::{Command, Page, Status};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div,
};

use super::Shell;
use crate::theme::{
    ActiveTheme, FONT_MONO, RULE, SPACE_5, SPACE_6, SPACE_8, STATUS_BAR, TEXT_SMALL,
};

const SEPARATOR: &str = " · ";

impl Shell {
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = *cx.palette();
        let snapshot = &self.state.snapshot;
        let problems = snapshot.problems();
        let left = match self.selected_cluster() {
            None if self.state.page == Page::Changes => snapshot.changes.fetched_label(),
            Some(cluster) => {
                let version = cluster.version.as_deref().unwrap_or("—");
                [cluster.distro.as_str(), cluster.region.as_str(), version].join(SEPARATOR)
            }
            None => format!(
                "{} of {} clusters connected",
                snapshot.connected(),
                snapshot.clusters.len()
            ),
        };
        div()
            .flex()
            .items_center()
            .flex_none()
            .gap(SPACE_8)
            .h(STATUS_BAR)
            .px(SPACE_6)
            .border_t(RULE)
            .border_color(palette.line)
            .bg(palette.chrome)
            .font_family(FONT_MONO)
            .text_size(TEXT_SMALL)
            .text_color(palette.mute)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(
                div()
                    .id("status-problems")
                    .flex()
                    .gap(SPACE_5)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dispatch(Command::SelectPage(Page::Overview), cx);
                    }))
                    .child(div().text_color(palette.err).child(format!(
                        "{} {}",
                        Status::Err.glyph(),
                        problems.err
                    )))
                    .child(div().text_color(palette.warn).child(format!(
                        "{} {}",
                        Status::Warn.glyph(),
                        problems.warn
                    ))),
            )
            .child(left)
            .child(div().flex_1())
            .children(self.loco_agent(&palette))
            .into_any_element()
    }

    #[cfg(feature = "loco")]
    fn loco_agent(&self, palette: &crate::theme::Palette) -> Option<AnyElement> {
        use crate::theme::SPACE_3;
        let synced = self.selected_cluster()?.loco_agent_synced?;
        let element = div()
            .flex()
            .items_center()
            .gap(SPACE_3)
            .child(div().text_color(palette.ok).child(Status::Ok.glyph()))
            .child(format!("loco agent · synced {}s ago", synced.as_secs()))
            .into_any_element();
        Some(element)
    }

    #[cfg(not(feature = "loco"))]
    #[allow(clippy::unused_self)]
    fn loco_agent(&self, _: &crate::theme::Palette) -> Option<AnyElement> {
        None
    }
}
