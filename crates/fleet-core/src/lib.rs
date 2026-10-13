pub mod fleet;
pub mod id;
pub mod nav;
pub mod state;
pub mod status;

pub use async_channel::{Receiver, Sender};
pub use fleet::{ClusterSummary, Snapshot};
pub use id::{ClusterId, ResourceRef};
pub use nav::{CLUSTER_NAV, FLEET_NAV, NavSection, Page};
pub use state::{AppState, Command, Update};
pub use status::{Counts, Status};

pub trait Backend: Send + 'static {
    fn start(self: Box<Self>, updates: Sender<Update>, commands: Receiver<Command>);
}
