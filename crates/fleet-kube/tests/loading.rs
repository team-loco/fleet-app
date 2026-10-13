use std::time::Duration;

use fleet_core::{
    Backend, ClusterId, Command, ContextRef, NewCluster, Page, Snapshot, Update, WatchState,
};
use fleet_kube::LiveBackend;
use fleet_kube::discovery::DiscoverySources;
use fleet_kube::settings::KubeSettings;
use fleet_kube::store::ClusterStore;

const E2E_VAR: &str = "FLEET_E2E";
const CLUSTERS: [&str; 2] = ["kind-fleet-a", "kind-fleet-b"];
const WINDOW: u64 = 7;
const WAIT: Duration = Duration::from_secs(120);
const DATA_DIR: &str = "data";

fn kubeconfig() -> Option<std::path::PathBuf> {
    let joined = std::env::var_os("KUBECONFIG")?;
    std::env::split_paths(&joined).next()
}

fn settled(snapshot: &Snapshot, id: &ClusterId, page: Page) -> bool {
    let synced = snapshot.page_state(id, page) == Some(&WatchState::Synced);
    synced && snapshot.page_rows(id, page).is_some_and(|rows| rows > 0)
}

#[test]
fn flux_pages_of_both_kind_clusters_settle_with_their_objects() {
    if std::env::var(E2E_VAR).as_deref() != Ok("1") {
        return;
    }
    let Some(file) = kubeconfig() else {
        panic!("{E2E_VAR}=1 needs KUBECONFIG with the kind contexts");
    };
    let Ok(home) = tempfile::tempdir() else {
        panic!("tempdir");
    };
    let mut store = ClusterStore::empty(&home.path().join(DATA_DIR));
    let added: Vec<NewCluster> = CLUSTERS
        .iter()
        .map(|name| NewCluster {
            context: ContextRef {
                file: file.clone(),
                context: (*name).to_owned(),
            },
            name: (*name).to_owned(),
        })
        .collect();
    assert!(store.add(&added).is_ok());
    let sources = DiscoverySources {
        kubeconfig_env: std::env::var_os("KUBECONFIG"),
        home: Some(home.path().to_path_buf()),
        folders: Vec::new(),
    };
    let backend = LiveBackend::new(KubeSettings::default(), sources, store);
    let (updates_tx, updates) = async_channel::unbounded();
    let (commands, commands_rx) = async_channel::unbounded();
    Box::new(backend).start(updates_tx, commands_rx);
    for name in CLUSTERS {
        let id = ClusterId::new(name);
        for page in [Page::Kustomizations, Page::HelmReleases, Page::Sources] {
            let steps = [
                Command::SelectCluster(Some(id.clone())),
                Command::SelectPage(page),
            ];
            for step in steps {
                let command = Command::ForWindow(WINDOW, Box::new(step));
                assert!(commands.send_blocking(command).is_ok());
            }
            let deadline = std::time::Instant::now() + WAIT;
            let mut last = None;
            loop {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{name} {page:?} never settled; last state {last:?}"
                );
                let Ok(Update::Snapshot(snapshot)) = updates.recv_blocking() else {
                    continue;
                };
                if settled(&snapshot, &id, page) {
                    break;
                }
                last = snapshot.page_state(&id, page).cloned();
            }
        }
    }
}
