use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trigger {
    #[default]
    Automatic,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdaterRequest {
    Check { trigger: Trigger, prereleases: bool },
    Install,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdaterState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
    },
    Ready {
        version: String,
    },
    Available {
        version: String,
        page: String,
    },
    Installing {
        version: String,
    },
    Relaunching,
    Failed {
        reason: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdaterStatus {
    pub state: UpdaterState,
    pub trigger: Trigger,
    pub checked: Option<SystemTime>,
}
