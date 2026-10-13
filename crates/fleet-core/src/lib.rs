pub mod add_cluster;
pub mod changes;
pub mod cluster;
pub mod events;
pub mod fleet;
pub mod fuzzy;
pub mod gitops;
pub mod home;
pub mod id;
pub mod nav;
pub mod nodes;
pub mod overview;
pub mod palette;
pub mod resource;
pub mod resources;
pub mod state;
pub mod status;
pub mod stream;
pub mod time;
pub mod workload;

pub use add_cluster::{
    AddClusterFlow, AddClusterStep, Candidate, CheckLine, CheckStatus, ContextChoice, ContextRef,
    KubeconfigFile, NewCluster, Verdict,
};
pub use async_channel::{Receiver, Sender};
pub use changes::ChangesLog;
pub use cluster::ClusterDetail;
pub use fleet::{ClusterSummary, Snapshot};
pub use gitops::GitOps;
pub use home::FleetHome;
pub use id::{ClusterId, ResourceRef};
pub use nav::{CLUSTER_NAV, FLEET_NAV, NavSection, Page};
pub use resource::ResourceSummary;
pub use resources::ResourceKind;
pub use state::{AppState, Command, Update};
pub use status::{Counts, Status};
pub use stream::{ExecEvent, ExecRequest, ExecTarget, LogLevel, LogLine, LogRequest, Sink, Source};
pub use workload::{Workload, WorkloadKind};

pub trait Backend: Send + 'static {
    fn start(self: Box<Self>, updates: Sender<Update>, commands: Receiver<Command>);
}
