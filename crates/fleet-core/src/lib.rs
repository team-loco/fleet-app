pub mod changes;
pub mod fleet;
pub mod fuzzy;
pub mod home;
pub mod id;
pub mod nav;
pub mod palette;
pub mod resource;
pub mod state;
pub mod status;
pub mod time;

pub use async_channel::{Receiver, Sender};
pub use changes::ChangesLog;
pub use fleet::{ClusterSummary, Snapshot};
pub use home::FleetHome;
pub use id::{ClusterId, ResourceRef};
pub use nav::{CLUSTER_NAV, FLEET_NAV, NavSection, Page};
pub use resource::ResourceSummary;
pub use state::{AppState, Command, Update};
pub use status::{Counts, Status};

pub trait Backend: Send + 'static {
    fn start(self: Box<Self>, updates: Sender<Update>, commands: Receiver<Command>);
}
