use fleet_core::action::forwards_label;
use fleet_core::gitops::flux_ready;
use fleet_core::palette::{COMMAND_PREFIX, STOP_FORWARD};
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
#[cfg(feature = "loco")]
const LOCO_SYNC_UNKNOWN: &str = "loco agent · sync not observable in-cluster";

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
            None if self.state.page == Page::GitOps => {
                let flux = flux_ready(&snapshot.clusters);
                format!("flux {}/{} ready", flux.ready, flux.total)
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
            .children(self.forwards(cx))
            .children(self.loco_agent(&palette))
            .into_any_element()
    }

    fn forwards(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let label = forwards_label(self.state.snapshot.forwards.len())?;
        let element = div()
            .id("status-forwards")
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| {
                let query = format!("{COMMAND_PREFIX}{STOP_FORWARD}");
                this.open_palette(&query, window, cx);
            }))
            .child(label)
            .into_any_element();
        Some(element)
    }

    #[cfg(feature = "loco")]
    fn loco_agent(&self, palette: &crate::theme::Palette) -> Option<AnyElement> {
        use crate::theme::SPACE_3;
        let id = self.state.cluster.as_ref()?;
        let agent = &self.state.snapshot.loco.get(id)?.platform.agent;
        let (status, label) = match agent.last_heartbeat {
            Some(heartbeat) => (
                Status::Ok,
                format!(
                    "loco agent · synced {} ago",
                    fleet_core::time::age(heartbeat)
                ),
            ),
            None => (Status::Offline, LOCO_SYNC_UNKNOWN.to_owned()),
        };
        let element = div()
            .flex()
            .items_center()
            .gap(SPACE_3)
            .child(
                div()
                    .text_color(palette.status(status))
                    .child(status.glyph()),
            )
            .child(label)
            .into_any_element();
        Some(element)
    }

    #[cfg(not(feature = "loco"))]
    #[allow(clippy::unused_self)]
    fn loco_agent(&self, _: &crate::theme::Palette) -> Option<AnyElement> {
        None
    }
}
