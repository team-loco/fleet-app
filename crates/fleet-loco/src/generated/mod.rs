pub mod applications;
pub mod builds;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplicationStatusPhase {
    Idle,
    Deploying,
    Ready,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildStatusPhase {
    Pending,
    Running,
    Succeeded,
    Failed,
    Canceled,
    #[serde(other)]
    Unknown,
}
