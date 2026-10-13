use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::thread;

use fleet_core::resource::page_for_kind;
use fleet_core::{
    Backend, ChangesLog, ClusterId, Command, FleetHome, GitOps, Page, Receiver, ResourceRef,
    Sender, Snapshot, Update,
};
use tokio::time::MissedTickBehavior;

use crate::discovery::{
    ContextRef, Discovery, DiscoveryEvent, DiscoverySources, KubeContext, discover,
};
use crate::manager::{COALESCE_INTERVAL, ClusterHandle, ClusterManager, Subscription};
use crate::resources::WatchKind;
use crate::settings::KubeSettings;
use crate::shell_env::{capture, user_shell};
use crate::snapshot::{page_kind, summarize};
use crate::watch::KubeconfigWatcher;

const THREAD_NAME: &str = "fleet-kube";
const BASELINE_KINDS: [WatchKind; 2] = [WatchKind::Nodes, WatchKind::Pods];

pub struct LiveBackend {
    settings: KubeSettings,
    sources: DiscoverySources,
}

impl LiveBackend {
    pub fn new(settings: KubeSettings, sources: DiscoverySources) -> Self {
        Self { settings, sources }
    }
}

impl Backend for LiveBackend {
    fn start(self: Box<Self>, updates: Sender<Update>, commands: Receiver<Command>) {
        let spawned = thread::Builder::new()
            .name(THREAD_NAME.into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .thread_name(THREAD_NAME)
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(run(*self, updates, commands)),
                    Err(error) => tracing::error!(%error, "live backend runtime failed to start"),
                }
            });
        if let Err(error) = spawned {
            tracing::error!(%error, "live backend thread failed to start");
        }
    }
}

struct Connected {
    id: ClusterId,
    handle: ClusterHandle,
    _baseline: Vec<Subscription>,
}

struct Live {
    manager: ClusterManager,
    clusters: BTreeMap<ContextRef, Connected>,
    selected: Option<ClusterId>,
    page: Page,
    page_subscription: Option<Subscription>,
}

pub fn cluster_ids(contexts: &[KubeContext]) -> BTreeMap<ContextRef, ClusterId> {
    let mut per_name: BTreeMap<&str, usize> = BTreeMap::new();
    for context in contexts {
        *per_name.entry(context.id.context.as_str()).or_default() += 1;
    }
    contexts
        .iter()
        .map(|context| {
            let name = context.id.context.as_str();
            let duplicated = per_name.get(name).is_some_and(|count| *count > 1);
            let file = context
                .id
                .file
                .file_name()
                .map(|file| file.to_string_lossy().into_owned())
                .unwrap_or_default();
            let id = if duplicated {
                format!("{name} ({file})")
            } else {
                name.to_owned()
            };
            (context.id.clone(), ClusterId::new(id))
        })
        .collect()
}

impl Live {
    fn reconcile(&mut self, discovery: &Discovery) {
        let ids = cluster_ids(&discovery.contexts);
        let wanted: BTreeSet<&ContextRef> = ids.keys().collect();
        let gone: Vec<ContextRef> = self
            .clusters
            .keys()
            .filter(|id| !wanted.contains(id))
            .cloned()
            .collect();
        for id in gone {
            self.clusters.remove(&id);
            self.manager.disconnect(&id);
        }
        for context in &discovery.contexts {
            let Some(id) = ids.get(&context.id).cloned() else {
                continue;
            };
            let unchanged = self
                .clusters
                .get(&context.id)
                .is_some_and(|existing| existing.handle.context() == context && existing.id == id);
            if unchanged {
                continue;
            }
            self.manager.disconnect(&context.id);
            let handle = self.manager.connect(context.clone());
            let baseline = BASELINE_KINDS
                .iter()
                .map(|kind| handle.subscribe(kind.clone()))
                .collect();
            self.clusters.insert(
                context.id.clone(),
                Connected {
                    id,
                    handle,
                    _baseline: baseline,
                },
            );
        }
        self.resubscribe_page();
    }

    fn selected_handle(&self) -> Option<&ClusterHandle> {
        let selected = self.selected.as_ref()?;
        self.clusters
            .values()
            .find(|cluster| &cluster.id == selected)
            .map(|cluster| &cluster.handle)
    }

    fn resubscribe_page(&mut self) {
        let kind = page_kind(self.page);
        let subscription = self
            .selected_handle()
            .zip(kind)
            .map(|(handle, kind)| handle.subscribe(kind));
        self.page_subscription = subscription;
    }

    fn apply(&mut self, command: &Command) {
        match command {
            Command::SelectCluster(id) => {
                self.selected.clone_from(id);
                self.page = if id.is_some() {
                    Page::ClusterOverview
                } else {
                    Page::Overview
                };
                self.resubscribe_page();
            }
            Command::SelectPage(page) => {
                self.page = *page;
                self.resubscribe_page();
            }
            Command::OpenCommit(_) => {
                self.page = Page::Changes;
                self.resubscribe_page();
            }
            Command::ToggleClustersPanel => {}
            Command::ToggleResourcesPanel => {}
            Command::OpenResource(resource) => self.open(resource),
            Command::FollowLogs(resource) => self.open(resource),
            Command::Compare(_) => {}
            Command::PortForward { .. } => {}
            Command::Cordon(_) => {}
            Command::Uncordon(_) => {}
            Command::Drain(_) => {}
            Command::Delete(_) => {}
            Command::OpenAddCluster(_) => {}
            Command::CheckContexts(_) => {}
            Command::AddClusters(_) => {}
            Command::RemoveCluster(_) => {}
            Command::WatchWorkload { .. } => {}
            Command::StreamLogs(_) => {}
            Command::Exec(_) => {}
            Command::RestartRollout(_) => {}
            Command::Scale { .. } => {}
            Command::Rollback { .. } => {}
        }
    }

    fn open(&mut self, resource: &ResourceRef) {
        self.selected = Some(resource.cluster.clone());
        self.page = page_for_kind(&resource.kind);
        self.resubscribe_page();
    }

    fn snapshot(&self) -> Snapshot {
        let mut clusters: Vec<_> = self
            .clusters
            .values()
            .map(|cluster| summarize(cluster.id.clone(), cluster.id.to_string(), &cluster.handle))
            .collect();
        clusters.sort_by(|a, b| a.name.cmp(&b.name));
        Snapshot {
            clusters,
            fleet_nav: BTreeMap::new(),
            home: FleetHome::default(),
            resources: Vec::new(),
            changes: ChangesLog::default(),
            details: BTreeMap::new(),
            gitops: GitOps::default(),
        }
    }
}

async fn run(backend: LiveBackend, updates: Sender<Update>, commands: Receiver<Command>) {
    let LiveBackend { settings, sources } = backend;
    let env = capture(&user_shell(), settings.shell_env_timeout).await;
    let scan_sources = sources.clone();
    let mut discovery = tokio::task::spawn_blocking(move || discover(&scan_sources))
        .await
        .unwrap_or_default();
    let started = KubeconfigWatcher::start(
        sources.clone(),
        discovery.clone(),
        settings.discovery_debounce,
    );
    let (_watcher, discovery_events) = match started {
        Ok((watcher, events)) => (Some(watcher), events),
        Err(error) => {
            tracing::warn!(%error, "kubeconfig files are not watched");
            (None, async_channel::unbounded().1)
        }
    };
    let runtime = tokio::runtime::Handle::current();
    let (manager, cluster_events) = ClusterManager::new(runtime, settings, Arc::new(env));
    let mut live = Live {
        manager,
        clusters: BTreeMap::new(),
        selected: None,
        page: Page::Overview,
        page_subscription: None,
    };
    live.reconcile(&discovery);
    let mut dirty = true;
    let mut publish = tokio::time::interval(COALESCE_INTERVAL);
    publish.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            event = discovery_events.recv(), if !discovery_events.is_closed() => {
                if let Ok(event) = event {
                    apply_discovery(&mut discovery, event);
                    live.reconcile(&discovery);
                    dirty = true;
                }
            }
            event = cluster_events.recv() => {
                if event.is_ok() {
                    dirty = true;
                }
            }
            command = commands.recv() => {
                let Ok(command) = command else { return };
                live.apply(&command);
            }
            _ = publish.tick() => {
                if dirty {
                    dirty = false;
                    if updates.send(Update::Snapshot(Box::new(live.snapshot()))).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

fn apply_discovery(discovery: &mut Discovery, event: DiscoveryEvent) {
    match event {
        DiscoveryEvent::Added(context) => discovery.contexts.push(context),
        DiscoveryEvent::Changed(context) => {
            if let Some(existing) = discovery
                .contexts
                .iter_mut()
                .find(|existing| existing.id == context.id)
            {
                *existing = context;
            }
        }
        DiscoveryEvent::Removed(id) => discovery.contexts.retain(|context| context.id != id),
        DiscoveryEvent::FileError(error) => {
            tracing::warn!(file = %error.file.display(), message = %error.message, "kubeconfig unreadable");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::provider::{AuthKind, Provider};

    fn context(file: &str, name: &str) -> KubeContext {
        KubeContext {
            id: ContextRef {
                file: PathBuf::from(file),
                context: name.to_owned(),
            },
            cluster: name.to_owned(),
            user: None,
            namespace: None,
            server: None,
            provider: Provider::Generic,
            auth: AuthKind::None,
            is_current: false,
        }
    }

    #[test]
    fn ids_are_context_names_unless_duplicated() {
        let contexts = [
            context("/h/.kube/config", "dup"),
            context("/h/.kube/other.yaml", "dup"),
            context("/h/.kube/config", "solo"),
        ];
        let ids = cluster_ids(&contexts);
        let names: Vec<String> = contexts
            .iter()
            .filter_map(|c| ids.get(&c.id).map(ToString::to_string))
            .collect();
        assert_eq!(names, vec!["dup (config)", "dup (other.yaml)", "solo"]);
    }

    #[test]
    fn discovery_events_update_the_context_list() {
        let mut discovery = Discovery::default();
        apply_discovery(&mut discovery, DiscoveryEvent::Added(context("/a", "x")));
        let mut changed = context("/a", "x");
        changed.namespace = Some("apps".to_owned());
        apply_discovery(&mut discovery, DiscoveryEvent::Changed(changed.clone()));
        assert_eq!(discovery.contexts, vec![changed]);
        apply_discovery(
            &mut discovery,
            DiscoveryEvent::Removed(context("/a", "x").id),
        );
        assert_eq!(discovery.contexts, Vec::new());
    }
}
