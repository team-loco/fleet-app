use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use kube::Client;
use kube::api::DynamicObject;
use kube::core::ApiResource;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};

use crate::backoff::Backoff;
use crate::client::connect;
use crate::discovery::{ContextRef, KubeContext};
use crate::error::{Error, Result};
use crate::failure::{Failure, FailureKind, classify};
use crate::probe;
use crate::provider::{Provider, refine_provider};
use crate::resources::{
    CustomKind, RunningWatch, Signal, SignalEvent, WatchKind, Watched, custom_snapshot, snapshot,
    spawn_builtin, spawn_custom,
};
use crate::settings::KubeSettings;
use crate::shell_env::ShellEnv;
use crate::task::TaskGuard;

pub const COALESCE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindFailure {
    pub kind: WatchKind,
    pub failure: Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Degraded(Vec<KindFailure>),
    AuthExpired(Failure),
    Offline(Failure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterInfo {
    pub provider: Provider,
    pub version: Option<String>,
    pub user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterEvent {
    Status {
        id: ContextRef,
        status: ConnectionStatus,
    },
    Changed {
        id: ContextRef,
        kinds: BTreeSet<WatchKind>,
    },
}

struct WatchEntry {
    refs: usize,
    resource: Option<ApiResource>,
    running: Option<RunningWatch>,
}

pub(crate) struct Shared {
    pub(crate) context: KubeContext,
    pub(crate) runtime: Handle,
    pub(crate) env: Arc<ShellEnv>,
    pub(crate) settings: KubeSettings,
    status: Mutex<ConnectionStatus>,
    info: Mutex<ClusterInfo>,
    client: Mutex<Option<Client>>,
    watches: Mutex<HashMap<WatchKind, WatchEntry>>,
    signals: mpsc::UnboundedSender<Signal>,
    pub(crate) resources: Mutex<HashMap<(String, String), ApiResource>>,
    pub(crate) forwards: Mutex<BTreeMap<u64, crate::ops::ActiveForward>>,
    pub(crate) next_forward: std::sync::atomic::AtomicU64,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    fn start(&self, kind: &WatchKind, entry: &mut WatchEntry, client: &Client) {
        let signals = self.signals.clone();
        entry.running = match (kind, &entry.resource) {
            (WatchKind::Custom(custom), Some(resource)) => Some(spawn_custom(
                &self.runtime,
                client.clone(),
                custom,
                resource,
                signals,
            )),
            (WatchKind::Custom(_), None) => None,
            (builtin, _) => spawn_builtin(builtin, &self.runtime, client.clone(), signals),
        };
    }

    fn set_client(&self, client: Option<&Client>) {
        *lock(&self.client) = client.cloned();
        let mut watches = lock(&self.watches);
        for (kind, entry) in watches.iter_mut() {
            entry.running = None;
            if let Some(client) = client {
                self.start(kind, entry, client);
            }
        }
    }

    fn subscribe(&self, kind: &WatchKind, resource: Option<ApiResource>) {
        let client = lock(&self.client).clone();
        let mut watches = lock(&self.watches);
        let entry = watches.entry(kind.clone()).or_insert_with(|| WatchEntry {
            refs: 0,
            resource,
            running: None,
        });
        entry.refs += 1;
        if entry.running.is_none()
            && let Some(client) = client
        {
            self.start(kind, entry, &client);
        }
    }

    fn unsubscribe(&self, kind: &WatchKind) {
        let mut watches = lock(&self.watches);
        let remove = watches.get_mut(kind).is_some_and(|entry| {
            entry.refs = entry.refs.saturating_sub(1);
            entry.refs == 0
        });
        if remove {
            watches.remove(kind);
        }
    }
}

pub struct Subscription {
    shared: Weak<Shared>,
    kind: WatchKind,
}

impl Subscription {
    pub fn kind(&self) -> &WatchKind {
        &self.kind
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.unsubscribe(&self.kind);
        }
    }
}

#[derive(Clone)]
pub struct ClusterHandle {
    pub(crate) shared: Arc<Shared>,
    _supervisor: Arc<TaskGuard>,
}

impl ClusterHandle {
    pub fn id(&self) -> &ContextRef {
        &self.shared.context.id
    }

    pub fn context(&self) -> &KubeContext {
        &self.shared.context
    }

    pub fn status(&self) -> ConnectionStatus {
        lock(&self.shared.status).clone()
    }

    pub fn info(&self) -> ClusterInfo {
        lock(&self.shared.info).clone()
    }

    pub fn client(&self) -> Result<Client> {
        lock(&self.shared.client).clone().ok_or(Error::NotConnected)
    }

    pub fn subscribe(&self, kind: WatchKind) -> Subscription {
        self.shared.subscribe(&kind, None);
        Subscription {
            shared: Arc::downgrade(&self.shared),
            kind,
        }
    }

    pub async fn subscribe_custom(&self, kind: CustomKind) -> Result<Subscription> {
        let resource = self.resolve_custom(&kind).await?;
        let watch_kind = WatchKind::Custom(kind);
        self.shared.subscribe(&watch_kind, Some(resource));
        Ok(Subscription {
            shared: Arc::downgrade(&self.shared),
            kind: watch_kind,
        })
    }

    pub fn objects<K: Watched>(&self) -> Vec<Arc<K>>
    where
        K::DynamicType: Eq + std::hash::Hash + Clone,
    {
        let watches = lock(&self.shared.watches);
        watches
            .get(&K::KIND)
            .and_then(|entry| entry.running.as_ref())
            .map(snapshot::<K>)
            .unwrap_or_default()
    }

    pub fn custom_objects(&self, kind: &CustomKind) -> Vec<Arc<DynamicObject>> {
        let watches = lock(&self.shared.watches);
        watches
            .get(&WatchKind::Custom(kind.clone()))
            .and_then(|entry| entry.running.as_ref())
            .map(custom_snapshot)
            .unwrap_or_default()
    }

    pub fn watched_kinds(&self) -> Vec<WatchKind> {
        lock(&self.shared.watches).keys().cloned().collect()
    }
}

#[derive(Default)]
struct Health {
    probe: Option<Failure>,
    kinds: BTreeMap<WatchKind, Failure>,
    connected_once: bool,
}

impl Health {
    fn status(&self) -> ConnectionStatus {
        if let Some(failure) = &self.probe {
            if failure.kind.is_auth() {
                return ConnectionStatus::AuthExpired(failure.clone());
            }
            return ConnectionStatus::Offline(failure.clone());
        }
        if !self.connected_once {
            return ConnectionStatus::Connecting;
        }
        if self.kinds.is_empty() {
            return ConnectionStatus::Connected;
        }
        let failures = self
            .kinds
            .iter()
            .map(|(kind, failure)| KindFailure {
                kind: kind.clone(),
                failure: failure.clone(),
            })
            .collect();
        ConnectionStatus::Degraded(failures)
    }
}

async fn establish(shared: &Shared) -> Result<Client> {
    let client = connect(
        &shared.context,
        &shared.env,
        shared.settings.connect_timeout,
    )
    .await?;
    let timeout = shared.settings.check_timeout;
    let version = tokio::time::timeout(timeout, client.apiserver_version())
        .await
        .map_err(|_| Error::Timeout(timeout))??;
    let nodes = tokio::time::timeout(timeout, probe::sample_nodes(&client)).await;
    let provider_id = nodes
        .ok()
        .and_then(Result::ok)
        .and_then(|sample| sample.provider_id);
    let provider = refine_provider(
        shared.context.provider,
        Some(&version.git_version),
        provider_id.as_deref(),
    );
    let mut info = lock(&shared.info);
    info.provider = provider;
    info.version = Some(version.git_version);
    Ok(client)
}

async fn health_check(shared: &Shared) -> Result<(), Failure> {
    let existing = lock(&shared.client).clone();
    let client = if let Some(client) = existing {
        client
    } else {
        let client = establish(shared)
            .await
            .map_err(|err| classify(&err, &shared.context))?;
        shared.set_client(Some(&client));
        client
    };
    let timeout = shared.settings.check_timeout;
    let who = tokio::time::timeout(timeout, probe::whoami(&client))
        .await
        .unwrap_or(Err(Error::Timeout(timeout)));
    match who {
        Ok(user) => {
            lock(&shared.info).user = user;
            Ok(())
        }
        Err(err) => {
            let failure = classify(&err, &shared.context);
            match failure.kind {
                FailureKind::Forbidden => Ok(()),
                FailureKind::NotFound => Ok(()),
                _ => Err(failure),
            }
        }
    }
}

async fn supervise(
    shared: Arc<Shared>,
    mut signals: mpsc::UnboundedReceiver<Signal>,
    events: async_channel::Sender<ClusterEvent>,
) {
    let id = shared.context.id.clone();
    let mut backoff = Backoff::new(shared.settings.backoff_initial, shared.settings.backoff_max);
    let mut health = Health::default();
    let mut published = None;
    let mut dirty = BTreeSet::new();
    let mut flush = tokio::time::interval(COALESCE_INTERVAL);
    flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut next_probe = Instant::now();
    loop {
        tokio::select! {
            () = tokio::time::sleep_until(next_probe) => {
                match health_check(&shared).await {
                    Ok(()) => {
                        health.probe = None;
                        health.connected_once = true;
                        backoff.reset();
                        next_probe = Instant::now() + shared.settings.health_interval;
                    }
                    Err(failure) => {
                        if failure.kind.is_auth() {
                            shared.set_client(None);
                        }
                        health.probe = Some(failure);
                        next_probe = Instant::now() + backoff.next_delay();
                    }
                }
            }
            received = signals.recv() => {
                let Some(signal) = received else { return };
                match signal.event {
                    SignalEvent::Changed => {
                        health.kinds.remove(&signal.kind);
                        dirty.insert(signal.kind);
                    }
                    SignalEvent::Failed(err) => {
                        let failure = classify(&err, &shared.context);
                        health.kinds.insert(signal.kind, failure);
                    }
                }
            }
            _ = flush.tick() => {
                if !dirty.is_empty() {
                    let kinds = std::mem::take(&mut dirty);
                    let event = ClusterEvent::Changed { id: id.clone(), kinds };
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
            }
        }
        let status = health.status();
        if published.as_ref() != Some(&status) {
            *lock(&shared.status) = status.clone();
            published = Some(status.clone());
            let event = ClusterEvent::Status {
                id: id.clone(),
                status,
            };
            if events.send(event).await.is_err() {
                return;
            }
        }
    }
}

pub struct ClusterManager {
    runtime: Handle,
    settings: KubeSettings,
    env: Arc<ShellEnv>,
    events: async_channel::Sender<ClusterEvent>,
    clusters: Mutex<HashMap<ContextRef, ClusterHandle>>,
}

impl ClusterManager {
    pub fn new(
        runtime: Handle,
        settings: KubeSettings,
        env: Arc<ShellEnv>,
    ) -> (Self, async_channel::Receiver<ClusterEvent>) {
        let (events, receiver) = async_channel::unbounded();
        let manager = Self {
            runtime,
            settings,
            env,
            events,
            clusters: Mutex::new(HashMap::new()),
        };
        (manager, receiver)
    }

    pub fn connect(&self, context: KubeContext) -> ClusterHandle {
        let mut clusters = lock(&self.clusters);
        if let Some(existing) = clusters.get(&context.id)
            && existing.shared.context == context
        {
            return existing.clone();
        }
        let (signals_tx, signals_rx) = mpsc::unbounded_channel();
        let info = ClusterInfo {
            provider: context.provider,
            version: None,
            user: None,
        };
        let shared = Arc::new(Shared {
            context: context.clone(),
            runtime: self.runtime.clone(),
            env: self.env.clone(),
            settings: self.settings.clone(),
            status: Mutex::new(ConnectionStatus::Connecting),
            info: Mutex::new(info),
            client: Mutex::new(None),
            watches: Mutex::new(HashMap::new()),
            signals: signals_tx,
            resources: Mutex::new(HashMap::new()),
            forwards: Mutex::new(BTreeMap::new()),
            next_forward: std::sync::atomic::AtomicU64::new(1),
        });
        let _guard = self.runtime.enter();
        let supervisor =
            TaskGuard::spawn(supervise(shared.clone(), signals_rx, self.events.clone()));
        let handle = ClusterHandle {
            shared,
            _supervisor: Arc::new(supervisor),
        };
        clusters.insert(context.id, handle.clone());
        handle
    }

    pub fn disconnect(&self, id: &ContextRef) -> bool {
        lock(&self.clusters).remove(id).is_some()
    }

    pub fn get(&self, id: &ContextRef) -> Option<ClusterHandle> {
        lock(&self.clusters).get(id).cloned()
    }

    pub fn connected(&self) -> Vec<ContextRef> {
        lock(&self.clusters).keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(kind: FailureKind) -> Failure {
        Failure {
            kind,
            detail: String::new(),
            message: String::new(),
            remedy: None,
        }
    }

    #[test]
    fn status_transitions() {
        let mut health = Health::default();
        assert_eq!(health.status(), ConnectionStatus::Connecting);
        health.probe = Some(failure(FailureKind::Unreachable));
        assert!(matches!(health.status(), ConnectionStatus::Offline(_)));
        health.probe = Some(failure(FailureKind::ExpiredCredentials));
        assert!(matches!(health.status(), ConnectionStatus::AuthExpired(_)));
        health.probe = None;
        health.connected_once = true;
        assert_eq!(health.status(), ConnectionStatus::Connected);
        health
            .kinds
            .insert(WatchKind::Secrets, failure(FailureKind::Forbidden));
        assert!(matches!(health.status(), ConnectionStatus::Degraded(ref f) if f.len() == 1));
    }
}
