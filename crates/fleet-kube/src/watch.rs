use std::path::Path;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::discovery::{Discovery, DiscoveryEvent, DiscoverySources, diff, discover};
use crate::error::Result;
use crate::task::TaskGuard;

const IGNORED_SEGMENTS: [&str; 2] = ["cache", "http-cache"];

pub struct KubeconfigWatcher {
    _watcher: RecommendedWatcher,
    _task: TaskGuard,
}

fn is_relevant(event: &notify::Event) -> bool {
    event.paths.iter().any(|path| !in_ignored_dir(path))
}

fn in_ignored_dir(path: &Path) -> bool {
    path.components().any(|part| {
        IGNORED_SEGMENTS
            .iter()
            .any(|ignored| part.as_os_str() == *ignored)
    })
}

impl KubeconfigWatcher {
    pub fn start(
        sources: DiscoverySources,
        initial: Discovery,
        debounce: Duration,
    ) -> Result<(Self, async_channel::Receiver<DiscoveryEvent>)> {
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<()>();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok_and(|event| is_relevant(&event)) {
                    let _ = raw_tx.send(());
                }
            })?;
        if let Some(dir) = sources.kube_dir().filter(|dir| dir.is_dir()) {
            watcher.watch(&dir, RecursiveMode::Recursive)?;
        }
        for root in sources.watch_roots() {
            if Some(&root) != sources.kube_dir().as_ref() {
                watcher.watch(&root, RecursiveMode::NonRecursive)?;
            }
        }
        let (events_tx, events_rx) = async_channel::unbounded();
        let task = TaskGuard::spawn(async move {
            let mut current = initial;
            while raw_rx.recv().await.is_some() {
                tokio::time::sleep(debounce).await;
                while raw_rx.try_recv().is_ok() {}
                let scan_sources = sources.clone();
                let Ok(next) = tokio::task::spawn_blocking(move || discover(&scan_sources)).await
                else {
                    continue;
                };
                for event in diff(&current, &next) {
                    if events_tx.send(event).await.is_err() {
                        return;
                    }
                }
                current = next;
            }
        });
        Ok((
            Self {
                _watcher: watcher,
                _task: task,
            },
            events_rx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::discovery::tests::kubeconfig;

    const EVENT_WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn ignores_cache_paths() {
        assert!(in_ignored_dir(Path::new("/h/.kube/cache/discovery/x")));
        assert!(!in_ignored_dir(Path::new("/h/.kube/eks/prod.yaml")));
    }

    #[tokio::test]
    async fn emits_added_and_removed() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let kube = home.path().join(".kube");
        assert!(fs::create_dir_all(&kube).is_ok());
        let sources = DiscoverySources {
            kubeconfig_env: None,
            home: Some(home.path().to_path_buf()),
        };
        let initial = discover(&sources);
        let Ok((_watcher, events)) =
            KubeconfigWatcher::start(sources, initial, Duration::from_millis(50))
        else {
            panic!("watcher starts");
        };

        let file = kube.join("dev.yaml");
        assert!(fs::write(&file, kubeconfig("kind-dev", "https://127.0.0.1:1")).is_ok());
        let added = tokio::time::timeout(EVENT_WAIT, events.recv()).await;
        assert!(
            matches!(added, Ok(Ok(DiscoveryEvent::Added(ref c))) if c.id.context == "kind-dev"),
            "{added:?}"
        );

        assert!(fs::remove_file(&file).is_ok());
        let removed = tokio::time::timeout(EVENT_WAIT, events.recv()).await;
        assert!(
            matches!(removed, Ok(Ok(DiscoveryEvent::Removed(ref id))) if id.context == "kind-dev"),
            "{removed:?}"
        );
    }
}
