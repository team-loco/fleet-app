use std::time::Instant;

use flock_core::watch::{NOT_STARTED, WATCH_SYNC_TIMEOUT};
use flock_core::{ClusterId, Command, Page, Status, WatchState};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Task, div};

use super::Shell;
use crate::kit::{Button, ConnectionProgress, EmptyState, ErrorState, Skeleton};
use crate::screens::is_flux_list;

const RETRY: &str = "Retry";
const REFRESH: &str = "Refresh";
const OPEN_GITOPS: &str = "Open GitOps";

#[derive(Default)]
pub(super) struct LoadClock {
    key: Option<(ClusterId, Page)>,
    since: Option<Instant>,
    wake: Option<Task<()>>,
}

impl Shell {
    fn tracked(&self) -> Option<(ClusterId, Page)> {
        let page = self.state.location().page;
        let cluster = self.state.location().cluster.clone()?;
        let tracked = !page.is_flock()
            && page != Page::Compare
            && self.state.location().resource.is_none()
            && self.state.location().flux_object.is_none();
        tracked.then_some((cluster, page))
    }

    fn reported(&self, cluster: &ClusterId, page: Page) -> WatchState {
        let snapshot = &self.state.snapshot;
        if let Some(state) = snapshot.page_state(cluster, page) {
            return state.clone();
        }
        let Some(summary) = snapshot.cluster(cluster) else {
            return WatchState::pending(NOT_STARTED);
        };
        if summary.nodes.is_none() && summary.status != Status::Progress {
            return WatchState::Failed(format!("{}: {}", summary.name, summary.state));
        }
        if snapshot.detail(cluster).is_some() {
            WatchState::Synced
        } else {
            WatchState::pending(NOT_STARTED)
        }
    }

    pub(super) fn track_load(&mut self, cx: &mut Context<Self>) {
        let tracked = self.tracked();
        if self.load.key != tracked {
            self.load = LoadClock {
                key: tracked.clone(),
                ..LoadClock::default()
            };
        }
        let pending = tracked.is_some_and(|(cluster, page)| {
            matches!(self.reported(&cluster, page), WatchState::Pending(_))
        });
        if !pending {
            self.load.since = None;
            self.load.wake = None;
            return;
        }
        if self.load.since.is_some() {
            return;
        }
        self.load.since = Some(Instant::now());
        self.load.wake = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(WATCH_SYNC_TIMEOUT).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    pub(super) fn page_state(&self) -> Option<WatchState> {
        let (cluster, page) = self.tracked()?;
        let reported = self.reported(&cluster, page);
        let waited = self.load.since.map(|since| since.elapsed());
        Some(reported.expire(waited.unwrap_or_default()))
    }

    pub(super) fn load_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.page_state()?;
        let cluster = self.selected_cluster()?;
        let id = cluster.id.clone();
        let page = self.state.location().page;
        let element = match state {
            WatchState::Synced => {
                if self.state.snapshot.page_rows(&id, page) != Some(0) {
                    return None;
                }
                let sentence = format!("No {} in {}.", page.label(), cluster.name);
                EmptyState::new(sentence, empty_action(id, page, cx)).into_any_element()
            }
            WatchState::Failed(reason) => {
                let retry = Button::new("page-retry", RETRY)
                    .on_click(cx.listener(move |this, _, _, cx| this.retry(id.clone(), cx)));
                ErrorState::new(&reason, retry).into_any_element()
            }
            WatchState::Pending(step) => {
                let progress = (cluster.status == Status::Progress)
                    .then(|| ConnectionProgress::new(cluster.name.clone(), step));
                let skeleton = if page == Page::ClusterOverview {
                    Skeleton::blocks()
                } else {
                    Skeleton::rows()
                };
                div().children(progress).child(skeleton).into_any_element()
            }
        };
        Some(element)
    }

    fn retry(&mut self, cluster: ClusterId, cx: &mut Context<Self>) {
        self.load = LoadClock::default();
        self.dispatch(Command::Connect(cluster), cx);
    }
}

fn empty_action(cluster: ClusterId, page: Page, cx: &mut Context<Shell>) -> Button {
    if is_flux_list(page) {
        return Button::new("page-empty-action", OPEN_GITOPS).on_click(
            cx.listener(|this, _, _, cx| this.dispatch(Command::SelectPage(Page::GitOps), cx)),
        );
    }
    Button::new("page-empty-action", REFRESH)
        .on_click(cx.listener(move |this, _, _, cx| this.retry(cluster.clone(), cx)))
}
