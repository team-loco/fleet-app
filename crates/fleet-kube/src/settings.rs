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
    pub metrics_interval: Duration,
    pub prometheus_interval: Duration,
    pub usage_history: Duration,
    pub event_limit: usize,
    pub workload_refresh: Duration,
}

impl KubeSettings {
    pub fn usage_samples(&self) -> usize {
        let interval = self.metrics_interval.as_secs().max(1);
        let samples = self.usage_history.as_secs() / interval;
        usize::try_from(samples).unwrap_or(usize::MAX).max(1)
    }
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
            metrics_interval: Duration::from_secs(15),
            prometheus_interval: Duration::from_secs(60),
            usage_history: Duration::from_secs(3600),
            event_limit: 1000,
            workload_refresh: Duration::from_secs(2),
        }
    }
}
