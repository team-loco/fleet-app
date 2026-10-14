use std::rc::Rc;

use fleet_core::tabs::{SavedWindow, Session};
use gpui::{App, Global};

pub type Saver = Rc<dyn Fn(&Session) -> Result<(), String>>;

#[derive(Default)]
struct Sessions {
    windows: Windows,
    save: Option<Saver>,
}

impl Global for Sessions {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Windows(Vec<(u64, SavedWindow)>);

impl Windows {
    fn record(&mut self, window: u64, saved: SavedWindow) -> bool {
        match self.0.iter_mut().find(|(id, _)| *id == window) {
            Some((_, current)) if *current == saved => false,
            Some((_, current)) => {
                *current = saved;
                true
            }
            None => {
                self.0.push((window, saved));
                true
            }
        }
    }

    fn forget(&mut self, window: u64) -> bool {
        let before = self.0.len();
        self.0.retain(|(id, _)| *id != window);
        self.0.len() != before
    }

    fn session(&self) -> Session {
        Session {
            windows: self.0.iter().map(|(_, saved)| saved.clone()).collect(),
        }
    }
}

pub fn install(save: Saver, cx: &mut App) {
    cx.default_global::<Sessions>().save = Some(save);
}

pub(crate) fn record(window: u64, saved: SavedWindow, cx: &mut App) {
    if cx
        .default_global::<Sessions>()
        .windows
        .record(window, saved)
    {
        persist(cx);
    }
}

pub(crate) fn forget(window: u64, cx: &mut App) {
    if cx.default_global::<Sessions>().windows.forget(window) {
        persist(cx);
    }
}

fn persist(cx: &App) {
    let Some(sessions) = cx.try_global::<Sessions>() else {
        return;
    };
    let Some(save) = sessions.save.as_ref() else {
        return;
    };
    if let Err(error) = save(&sessions.windows.session()) {
        tracing::warn!(%error, "could not save the open tabs");
    }
}

#[cfg(test)]
mod tests {
    use fleet_core::history::Location;
    use fleet_core::tabs::SavedPane;
    use fleet_core::{ClusterId, Page};

    use super::*;

    fn window(page: Page) -> SavedWindow {
        SavedWindow {
            panes: vec![SavedPane {
                tabs: vec![Location::page_on(ClusterId::new("a"), page)],
                active: 0,
            }],
            focused: 0,
        }
    }

    #[test]
    fn windows_keep_their_open_order_and_report_changes() {
        let mut windows = Windows::default();
        assert!(windows.record(7, window(Page::Nodes)));
        assert!(windows.record(3, window(Page::Pods)));
        assert!(!windows.record(7, window(Page::Nodes)));
        assert!(windows.record(7, window(Page::Events)));
        assert_eq!(
            windows.session().windows,
            [window(Page::Events), window(Page::Pods)]
        );
        assert!(windows.forget(7));
        assert!(!windows.forget(7));
        assert_eq!(windows.session().windows, [window(Page::Pods)]);
    }
}
