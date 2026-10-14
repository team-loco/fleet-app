use flock_core::{Command, Page, Receiver, Sender, Snapshot, Status, Update, WatchState};

const LISTING: &str = "Listing Deployment";
const CONNECTING_STEP: &str = "Checking the API server and nodes";
const FORBIDDEN: &str = "deployments.apps is forbidden: User \"dev\" cannot list resource \"deployments\" in API group \"apps\" at the cluster scope";
const CONNECTING: &str = "Connecting";
const PAGES: [Page; 2] = [Page::ClusterOverview, Page::Deployments];

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum StateFixture {
    Loading,
    Connecting,
    Empty,
    Error,
}

fn snapshot(fixture: StateFixture) -> Snapshot {
    let mut snapshot = flock_demo::snapshot();
    let state = match fixture {
        StateFixture::Loading => WatchState::pending(LISTING),
        StateFixture::Connecting => WatchState::pending(CONNECTING_STEP),
        StateFixture::Empty => WatchState::Synced,
        StateFixture::Error => WatchState::Failed(FORBIDDEN.to_owned()),
    };
    for cluster in &mut snapshot.clusters {
        if fixture == StateFixture::Connecting {
            cluster.status = Status::Progress;
            CONNECTING.clone_into(&mut cluster.state);
            cluster.nodes = None;
        }
        let pages = PAGES
            .into_iter()
            .map(|page| (page, state.clone()))
            .collect();
        snapshot.pages.insert(cluster.id.clone(), pages);
    }
    if fixture == StateFixture::Empty {
        for detail in snapshot.details.values_mut() {
            detail.resources.clear();
        }
    }
    snapshot
}

pub(crate) fn backend(fixture: StateFixture) -> (Receiver<Update>, Sender<Command>) {
    let (updates, receiver) = async_channel::unbounded();
    let (sender, commands) = async_channel::unbounded();
    std::thread::spawn(move || {
        let first = Update::Snapshot(Box::new(snapshot(fixture)));
        if updates.send_blocking(first).is_err() {
            return;
        }
        while let Ok(command) = commands.recv_blocking() {
            let command = match command {
                Command::ForWindow(_, command) => *command,
                command => command,
            };
            if let Command::Connect(_) = command {
                let loaded = Update::Snapshot(Box::new(flock_demo::snapshot()));
                if updates.send_blocking(loaded).is_err() {
                    return;
                }
            }
        }
    });
    (receiver, sender)
}
