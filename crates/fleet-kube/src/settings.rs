use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeSettings {
    pub shell_env_timeout: Duration,
    pub connect_timeout: Duration,
    pub check_timeout: Duration,
    pub health_interval: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    pub discovery_debounce: Duration,
}

impl Default for KubeSettings {
    fn default() -> Self {
        Self {
            shell_env_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
            check_timeout: Duration::from_secs(15),
            health_interval: Duration::from_secs(30),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            discovery_debounce: Duration::from_millis(300),
        }
    }
}
