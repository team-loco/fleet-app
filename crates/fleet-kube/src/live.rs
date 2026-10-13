use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fleet_core::add_cluster::home_relative;
use fleet_core::home::problems_badge;
use fleet_core::resource::page_for_kind;
use fleet_core::settings::{Auth, ClusterSettings, Connection};
use fleet_core::stream::Sink;
use fleet_core::{
    Backend, ChangesLog, CheckLine, ClusterCatalog, ClusterId, ClusterSummary, Command,
    ContextChoice, Counts, FleetHome, GitOps, KubeconfigFile, NewCluster, Notice, Page,
    Preferences, Receiver, ResourceRef, Sender, Snapshot, Status, Update, Verdict, Workload,
};
use fleet_flux::ObjectKind;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Node;
use tokio::time::MissedTickBehavior;

use crate::actions::{Actions, cluster_of};
use crate::assemble::{self, Extras, Prometheus, Selection};
use crate::checks::{self, CheckResult, run_checks_streaming};
use crate::discovery::{
    ContextRef, Discovery, DiscoveryEvent, DiscoverySources, KubeContext, discover,
};
use crate::flux::{WATCHED as FLUX_KINDS, custom_kind};
use crate::gitops::GitOpsState;
use crate::inspect;
use crate::manager::{
    COALESCE_INTERVAL, ClusterEvent, ClusterHandle, ClusterManager, ConnectionStatus, Subscription,
};
use crate::metrics::{NodeUsage, PodUsage};
use crate::paste;
use crate::probe::find_prometheus;
use crate::problems::order;
use crate::provider::AuthKind;
use crate::resources::WatchKind;
use crate::settings::KubeSettings;
use crate::shell_env::{ShellEnv, capture, user_shell};
use crate::snapshot::page_kinds;
use crate::store::{ClusterStore, SavedCluster, SavedPreferences};
use crate::task::TaskGuard;
use crate::usage::prometheus_utilization;
use crate::watch::KubeconfigWatcher;

const THREAD_NAME: &str = "fleet-kube";
const BASELINE_KINDS: [WatchKind; 2] = [WatchKind::Nodes, WatchKind::Pods];
const MISSING_STATE: &str = "Not in kubeconfig";
const DISCONNECTED_STATE: &str = "Disconnected";
const SET_ASIDE_TITLE: &str = "Saved clusters could not be read";
const MISSING_CONTEXT: &str = "This context is no longer in its kubeconfig file.";

pub struct LiveBackend {
    settings: KubeSettings,
    sources: DiscoverySources,
    store: ClusterStore,
}

impl LiveBackend {
    pub fn new(settings: KubeSettings, mut sources: DiscoverySources, store: ClusterStore) -> Self {
        sources.folders = scan_folders(&store);
        Self {
            settings,
            sources,
            store,
        }
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

enum Fetched {
    NodeUsage(ContextRef, Option<Vec<NodeUsage>>),
    PodUsage(ContextRef, Vec<PodUsage>),
    Prometheus(ContextRef, Option<Prometheus>),
    Flux(ContextRef, Vec<(ObjectKind, Subscription)>),
    Yaml(ResourceRef, inspect::Inspected),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FluxWatch {
    Unknown,
    Pending,
    Resolved,
}

struct Connected {
    id: ClusterId,
    handle: ClusterHandle,
    _baseline: Vec<Subscription>,
    extras: Extras,
    flux: FluxWatch,
    flux_subscriptions: Vec<Subscription>,
    usage_task: Option<TaskGuard>,
    pod_usage_task: Option<TaskGuard>,
    prometheus_task: Option<TaskGuard>,
    flux_task: Option<TaskGuard>,
}

struct Inspecting {
    target: ResourceRef,
    shown: Option<String>,
}

struct Live {
    manager: ClusterManager,
    store: ClusterStore,
    discovery: Discovery,
    sources: DiscoverySources,
    sources_changed: bool,
    env: Arc<ShellEnv>,
    settings: KubeSettings,
    updates: Sender<Update>,
    home: Option<PathBuf>,
    clusters: BTreeMap<ContextRef, Connected>,
    disconnected: BTreeSet<ClusterId>,
    selected: Option<ClusterId>,
    page: Page,
    actions: Actions,
    gitops: GitOpsState,
    page_subscriptions: Vec<Subscription>,
    fetched: Sender<Fetched>,
    yaml: Option<(ResourceRef, String)>,
    inspecting: Option<Inspecting>,
    yaml_task: Option<TaskGuard>,
    workload_task: Option<TaskGuard>,
    #[cfg(feature = "loco")]
    loco: crate::loco::LocoBackend,
}

fn reachable(status: &ConnectionStatus) -> bool {
    matches!(
        status,
        ConnectionStatus::Connected | ConnectionStatus::Degraded(_)
    )
}

impl Live {
    fn start(&mut self) {
        if let Some(notice) = set_aside_notice(&self.store) {
            self.publish(Update::Notice(notice));
        }
        self.reconcile();
    }

    fn reconcile(&mut self) {
        let ids = wanted(&self.store, &self.discovery, &self.disconnected);
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
        for (context_ref, id) in ids {
            let Some(context) = self.discovery.get(&context_ref).cloned() else {
                continue;
            };
            let context = &context;
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
                    extras: Extras::new(self.settings.usage_samples()),
                    flux: FluxWatch::Unknown,
                    flux_subscriptions: Vec::new(),
                    usage_task: None,
                    pod_usage_task: None,
                    prometheus_task: None,
                    flux_task: None,
                },
            );
        }
        self.gitops
            .sync(self.clusters.values().map(|c| (&c.id, &c.handle)));
        self.resubscribe_page();
    }

    fn selected_handle(&self) -> Option<&ClusterHandle> {
        let selected = self.selected.as_ref()?;
        self.clusters
            .values()
            .find(|cluster| &cluster.id == selected)
            .map(|cluster| &cluster.handle)
    }

    fn selected_key(&self) -> Option<ContextRef> {
        let selected = self.selected.as_ref()?;
        self.clusters
            .iter()
            .find(|(_, cluster)| &cluster.id == selected)
            .map(|(key, _)| key.clone())
    }

    fn resubscribe_page(&mut self) {
        let subscriptions = self
            .selected_handle()
            .map(|handle| {
                page_kinds(self.page)
                    .into_iter()
                    .map(|kind| handle.subscribe(kind))
                    .collect()
            })
            .unwrap_or_default();
        self.page_subscriptions = subscriptions;
        self.poll_page();
        #[cfg(feature = "loco")]
        self.loco.sync(
            self.clusters.values().map(|c| (&c.id, &c.handle)),
            self.selected.as_ref(),
            self.page,
        );
    }

    fn poll_page(&mut self) {
        match self.page {
            Page::ClusterOverview => self.poll_prometheus(),
            Page::Namespaces => self.poll_pod_usage(),
            Page::Pods => self.poll_pod_usage(),
            _ => {}
        }
    }

    fn poll_usage(&mut self) {
        let timeout = self.settings.check_timeout;
        for (key, cluster) in &mut self.clusters {
            if !reachable(&cluster.handle.status()) {
                continue;
            }
            let handle = cluster.handle.clone();
            let fetched = self.fetched.clone();
            let key = key.clone();
            cluster.usage_task = Some(TaskGuard::spawn(async move {
                let usage = tokio::time::timeout(timeout, handle.node_usage()).await;
                let usage = usage.ok().and_then(Result::ok);
                let _ = fetched.send(Fetched::NodeUsage(key, usage)).await;
            }));
        }
        if matches!(self.page, Page::Namespaces | Page::Pods) {
            self.poll_pod_usage();
        }
    }

    fn poll_pod_usage(&mut self) {
        let timeout = self.settings.check_timeout;
        let fetched = self.fetched.clone();
        let Some(key) = self.selected_key() else {
            return;
        };
        let Some(cluster) = self.clusters.get_mut(&key) else {
            return;
        };
        let handle = cluster.handle.clone();
        cluster.pod_usage_task = Some(TaskGuard::spawn(async move {
            let usage = tokio::time::timeout(timeout, handle.pod_usage(None)).await;
            let usage = usage.ok().and_then(Result::ok).unwrap_or_default();
            let _ = fetched.send(Fetched::PodUsage(key, usage)).await;
        }));
    }

    fn poll_prometheus(&mut self) {
        let timeout = self.settings.check_timeout;
        let fetched = self.fetched.clone();
        let Some(key) = self.selected_key() else {
            return;
        };
        let Some(cluster) = self.clusters.get_mut(&key) else {
            return;
        };
        let Ok(client) = cluster.handle.client() else {
            return;
        };
        let handle = cluster.handle.clone();
        let known = cluster.extras.prometheus.as_ref().map(|p| p.target.clone());
        cluster.prometheus_task = Some(TaskGuard::spawn(async move {
            let target = match known {
                Some(target) => Some(target),
                None => find_prometheus(&client).await,
            };
            let found = match target {
                Some(target) => {
                    let ranges = prometheus_utilization(&handle, &target, Timestamp::now());
                    let utilization = tokio::time::timeout(timeout, ranges).await;
                    let utilization = utilization.ok().and_then(Result::ok).unwrap_or_default();
                    Some(Prometheus {
                        target,
                        utilization,
                    })
                }
                None => None,
            };
            let _ = fetched.send(Fetched::Prometheus(key, found)).await;
        }));
    }

    fn cluster_event(&mut self, event: &ClusterEvent) {
        let (id, status) = match event {
            ClusterEvent::Status { id, status } => (id, status),
            ClusterEvent::Changed { id, .. } => {
                self.refresh_yaml(id);
                return;
            }
        };
        let fetched = self.fetched.clone();
        let Some(cluster) = self.clusters.get_mut(id) else {
            return;
        };
        if !reachable(status) || cluster.flux != FluxWatch::Unknown {
            return;
        }
        cluster.flux = FluxWatch::Pending;
        let handle = cluster.handle.clone();
        let key = id.clone();
        cluster.flux_task = Some(TaskGuard::spawn(async move {
            let mut subscriptions = Vec::new();
            for kind in FLUX_KINDS {
                if let Ok(subscription) = handle.subscribe_custom(custom_kind(kind)).await {
                    subscriptions.push((kind, subscription));
                }
            }
            let _ = fetched.send(Fetched::Flux(key, subscriptions)).await;
        }));
    }

    fn fetched(&mut self, fetched: Fetched) {
        match fetched {
            Fetched::NodeUsage(key, usage) => {
                if let Some(cluster) = self.clusters.get_mut(&key) {
                    let nodes = cluster.handle.objects::<Node>();
                    cluster.extras.record(&nodes, usage);
                }
            }
            Fetched::PodUsage(key, usage) => {
                if let Some(cluster) = self.clusters.get_mut(&key) {
                    cluster.extras.pod_usage = usage;
                }
            }
            Fetched::Prometheus(key, found) => {
                if let Some(cluster) = self.clusters.get_mut(&key) {
                    cluster.extras.prometheus = found;
                }
            }
            Fetched::Flux(key, subscriptions) => {
                if let Some(cluster) = self.clusters.get_mut(&key) {
                    cluster.extras.flux = subscriptions.iter().map(|(kind, _)| *kind).collect();
                    cluster.flux_subscriptions =
                        subscriptions.into_iter().map(|(_, sub)| sub).collect();
                    cluster.flux = FluxWatch::Resolved;
                }
            }
            Fetched::Yaml(target, inspected) => {
                if let Some(inspecting) = self.inspecting.as_mut()
                    && inspecting.target == target
                {
                    inspecting.shown = inspected.version;
                    self.yaml = Some((target, inspected.yaml));
                }
            }
        }
    }

    fn watch_workload(&mut self, target: &ResourceRef, sink: &Sink<Workload>) {
        self.workload_task = None;
        let Some(handle) = self.handle(&target.cluster).cloned() else {
            return;
        };
        let refresh = self.settings.workload_refresh;
        let watch = crate::workload::watch(handle, target.clone(), sink.clone(), refresh);
        self.workload_task = Some(TaskGuard::spawn(watch));
    }

    fn inspect(&mut self, target: &ResourceRef) {
        self.yaml = None;
        self.yaml_task = None;
        self.inspecting = Some(Inspecting {
            target: target.clone(),
            shown: None,
        });
        self.fetch_yaml();
    }

    fn refresh_yaml(&mut self, changed: &ContextRef) {
        let Some(inspecting) = self.inspecting.as_ref() else {
            return;
        };
        let Some(cluster) = self.clusters.get(changed) else {
            return;
        };
        if cluster.id != inspecting.target.cluster {
            return;
        }
        let cached = inspect::cached_version(&cluster.handle, &inspecting.target);
        if inspect::stale(inspecting.shown.as_deref(), cached.as_deref()) {
            self.fetch_yaml();
        }
    }

    fn fetch_yaml(&mut self) {
        let Some(target) = self.inspecting.as_ref().map(|i| i.target.clone()) else {
            return;
        };
        let Some(handle) = self.handle(&target.cluster).cloned() else {
            return;
        };
        let fetched = self.fetched.clone();
        self.yaml_task = Some(TaskGuard::spawn(async move {
            let inspected = match inspect::yaml(&handle, &target).await {
                Ok(inspected) => inspected,
                Err(error) => inspect::Inspected {
                    version: None,
                    yaml: format!("# {error}"),
                },
            };
            let _ = fetched.send(Fetched::Yaml(target, inspected)).await;
        }));
    }

    fn apply(&mut self, command: &Command) -> bool {
        self.gitops.command(command);
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
            Command::OpenFluxObject(key) => {
                self.selected = Some(key.cluster.clone());
                self.page = key.kind.page();
                self.resubscribe_page();
            }
            Command::Flux(..) => self.act(command),
            Command::ToggleClustersPanel => {}
            Command::ToggleResourcesPanel => {}
            Command::OpenResource(resource) => self.open(resource),
            Command::InspectResource(resource) => self.open(resource),
            Command::FollowLogs(resource) => self.open(resource),
            Command::Compare(_) => {}
            Command::PortForward { .. } => self.act(command),
            Command::StopPortForward { .. } => self.act(command),
            Command::Cordon(_) => self.act(command),
            Command::Uncordon(_) => self.act(command),
            Command::Drain(_) => self.act(command),
            Command::Delete(_) => self.act(command),
            Command::OpenAddCluster(_) => self.publish_contexts(),
            Command::SaveKubeconfig { name, yaml } => self.save_kubeconfig(name, yaml),
            Command::WatchFolder(folder) => self.watch_folder(folder),
            Command::CheckContexts(ids) => self.check(ids),
            Command::AddClusters(added) => {
                self.add(added);
                return true;
            }
            Command::RemoveCluster(id) => {
                self.remove(id);
                return true;
            }
            Command::WatchWorkload { target, sink } => self.watch_workload(target, sink),
            Command::StreamLogs(_) => self.act(command),
            Command::Exec(_) => self.act(command),
            Command::RestartRollout(_) => self.act(command),
            Command::Scale { .. } => self.act(command),
            Command::Rollback { .. } => self.act(command),
            #[cfg(feature = "loco")]
            Command::Loco(command) => {
                let cluster = command.target().cluster.clone();
                let handle = self.clusters.values().find(|c| c.id == cluster);
                self.loco.command(command, handle.map(|c| &c.handle));
            }
            Command::CopySpec { .. } => {}
            Command::ShowClusterSettings(_) => {}
            Command::SetPreferences(id, preferences) => {
                return self.set_preferences(id, preferences);
            }
            Command::Connect(id) => self.reconnect(id),
            Command::Reauthenticate(id) => self.reconnect(id),
            Command::Disconnect(id) => {
                self.disconnected.insert(id.clone());
                self.reconcile();
            }
        }
        false
    }

    fn publish(&self, update: Update) {
        if self.updates.try_send(update).is_err() {
            tracing::debug!("UI stopped listening for updates");
        }
    }

    async fn rescan_sources(
        &mut self,
    ) -> Option<(Option<KubeconfigWatcher>, Receiver<DiscoveryEvent>)> {
        if !std::mem::take(&mut self.sources_changed) {
            return None;
        }
        self.discovery = scan(&self.sources).await;
        self.reconcile();
        self.publish_contexts();
        Some(watch_kubeconfigs(
            &self.sources,
            &self.discovery,
            &self.settings,
        ))
    }

    fn publish_contexts(&self) {
        let files = kubeconfig_files(&self.discovery, &self.store, self.home.as_deref());
        self.publish(Update::Contexts(files));
    }

    fn save_kubeconfig(&mut self, name: &str, yaml: &str) {
        let dir = self.store.kubeconfigs_dir();
        match paste::save(&dir, name, yaml) {
            Ok(_) => self.sources_changed = true,
            Err(error) => self.publish(Update::KubeconfigRejected(error.to_string())),
        }
    }

    fn watch_folder(&mut self, folder: &Path) {
        match self.store.add_folder(folder) {
            Ok(_) => {
                self.sources.folders = scan_folders(&self.store);
                self.sources_changed = true;
            }
            Err(error) => self.publish(Update::KubeconfigRejected(error.to_string())),
        }
    }

    fn add(&mut self, added: &[NewCluster]) {
        if let Err(error) = self.store.add(added) {
            tracing::error!(%error, path = %self.store.path().display(), "saving clusters failed");
        }
        self.reconcile();
    }

    fn reconnect(&mut self, id: &ClusterId) {
        self.disconnected.remove(id);
        let existing: Vec<ContextRef> = self
            .clusters
            .iter()
            .filter(|(_, cluster)| &cluster.id == id)
            .map(|(key, _)| key.clone())
            .collect();
        for key in existing {
            self.clusters.remove(&key);
            self.manager.disconnect(&key);
        }
        self.reconcile();
    }

    fn set_preferences(&mut self, id: &ClusterId, preferences: &Preferences) -> bool {
        match self.store.set_preferences(id, preferences) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::error!(%error, path = %self.store.path().display(), "saving preferences failed");
                false
            }
        }
    }

    fn remove(&mut self, id: &ClusterId) {
        if let Err(error) = self.store.remove(id) {
            tracing::error!(%error, path = %self.store.path().display(), "saving clusters failed");
        }
        if self.selected.as_ref() == Some(id) {
            self.selected = None;
            self.page = Page::Overview;
        }
        self.reconcile();
    }

    fn check(&self, ids: &[ContextRef]) {
        for id in ids {
            let Some(context) = self.discovery.get(id).cloned() else {
                let line = CheckLine {
                    status: fleet_core::CheckStatus::Err,
                    detail: MISSING_CONTEXT.to_owned(),
                    remedy: None,
                    duration: Duration::ZERO,
                };
                self.publish(Update::Check(id.clone(), line));
                self.publish(Update::Checked(id.clone(), Verdict::CannotReach));
                continue;
            };
            let id = id.clone();
            let env = self.env.clone();
            let settings = self.settings.clone();
            let updates = self.updates.clone();
            tokio::spawn(async move {
                let (progress, results) = async_channel::unbounded::<CheckResult>();
                let run = async move {
                    let report = run_checks_streaming(&context, &env, &settings, &progress).await;
                    drop(progress);
                    report
                };
                let forward = async {
                    while let Ok(result) = results.recv().await {
                        let line = check_line(&result);
                        if updates.send(Update::Check(id.clone(), line)).await.is_err() {
                            return;
                        }
                    }
                };
                let (report, ()) = tokio::join!(run, forward);
                let verdict = verdict(report.verdict);
                if updates.send(Update::Checked(id, verdict)).await.is_err() {
                    tracing::debug!("UI stopped listening for check results");
                }
            });
        }
    }

    fn handle(&self, id: &ClusterId) -> Option<&ClusterHandle> {
        self.clusters
            .values()
            .find(|cluster| &cluster.id == id)
            .map(|cluster| &cluster.handle)
    }

    fn act(&self, command: &Command) {
        let handle = cluster_of(command).and_then(|id| self.handle(id)).cloned();
        self.actions.run(command, handle);
    }

    fn open(&mut self, resource: &ResourceRef) {
        let page = page_for_kind(&resource.kind);
        let moved = self.selected.as_ref() != Some(&resource.cluster) || self.page != page;
        self.selected = Some(resource.cluster.clone());
        self.page = page;
        if moved {
            self.resubscribe_page();
        }
        self.inspect(resource);
    }

    fn snapshot(&self) -> Snapshot {
        let now = Timestamp::now();
        let mut connected = Vec::new();
        let mut problems = Vec::new();
        let mut details = BTreeMap::new();
        let mut flux = Counts::default();
        let mut pods = Vec::new();
        for cluster in self.clusters.values() {
            let selected = self.selected.as_ref() == Some(&cluster.id);
            let selection = selected.then_some(Selection {
                event_limit: self.settings.event_limit,
                yaml: self.yaml.as_ref(),
            });
            let built = assemble::cluster(
                &cluster.id,
                &cluster.handle,
                &cluster.extras,
                selection.as_ref(),
                now,
            );
            connected.push(built.summary);
            pods.extend(built.pods);
            problems.extend(built.problems);
            flux = flux + built.flux;
            if let Some(detail) = built.detail {
                details.insert(cluster.id.clone(), detail);
            }
        }
        order(&mut problems);
        let flagged = problems_badge(&problems);
        let missing = self
            .store
            .clusters()
            .iter()
            .filter(|saved| !self.clusters.contains_key(&saved.context_ref()))
            .map(|saved| offline_summary(saved, self.disconnected.contains(&saved.id())));
        let mut clusters: Vec<_> = connected.into_iter().chain(missing).collect();
        clusters.sort_by(|a, b| a.name.cmp(&b.name));
        #[cfg(feature = "loco")]
        let loco = self.loco.snapshot(&mut clusters);
        let connected_settings = self.clusters.values().map(|cluster| {
            let expired = matches!(cluster.handle.status(), ConnectionStatus::AuthExpired(_));
            let saved = self.store.get(&cluster.id).map(|c| &c.preferences);
            let mut settings =
                cluster_settings(cluster.id.clone(), cluster.handle.context(), expired, saved);
            settings.prometheus = cluster
                .extras
                .prometheus
                .as_ref()
                .map(|found| format!("{}:{}", found.target.label(), found.target.port));
            settings
        });
        let disconnected_settings = self
            .store
            .clusters()
            .iter()
            .filter(|saved| self.disconnected.contains(&saved.id()))
            .filter_map(|saved| {
                let context = self.discovery.get(&saved.context_ref())?;
                Some(cluster_settings(
                    saved.id(),
                    context,
                    false,
                    Some(&saved.preferences),
                ))
            });
        let mut settings: Vec<ClusterSettings> =
            connected_settings.chain(disconnected_settings).collect();
        settings.sort_by(|a, b| a.cluster.cmp(&b.cluster));
        let sources: BTreeSet<&Path> = self.clusters.keys().map(|id| id.file.as_path()).collect();
        let mut snapshot = Snapshot {
            clusters,
            fleet_nav: BTreeMap::from([(Page::Problems, flagged), (Page::GitOps, flux)]),
            home: FleetHome { problems },
            resources: Vec::new(),
            changes: ChangesLog::default(),
            details,
            gitops: GitOps::default(),
            flux: Vec::new(),
            #[cfg(feature = "loco")]
            loco,
            comparisons: Vec::new(),
            catalog: ClusterCatalog {
                sources: sources.len(),
                clusters: settings,
            },
            pods,
            forwards: Vec::new(),
        };
        self.gitops.fill(&mut snapshot);
        self.actions.overlay(&mut snapshot);
        snapshot
    }
}

pub fn cluster_settings(
    cluster: ClusterId,
    context: &KubeContext,
    auth_expired: bool,
    saved: Option<&SavedPreferences>,
) -> ClusterSettings {
    let auth = match &context.auth {
        AuthKind::Exec(exec) => Auth::Exec(exec.summary()),
        other => Auth::Other(other.to_string()),
    };
    let mut preferences = saved.map(Preferences::from).unwrap_or_default();
    if preferences.default_namespace.is_none() {
        preferences.default_namespace.clone_from(&context.namespace);
    }
    ClusterSettings {
        cluster,
        connection: Connection {
            context: context.id.context.clone(),
            server: context.server.clone().unwrap_or_default(),
            auth,
            kubeconfig: context.id.file.display().to_string(),
        },
        auth_expired,
        prometheus: None,
        preferences,
    }
}

fn scan_folders(store: &ClusterStore) -> Vec<PathBuf> {
    let managed = store.kubeconfigs_dir();
    std::iter::once(managed)
        .chain(store.folders().iter().cloned())
        .collect()
}

fn watch_kubeconfigs(
    sources: &DiscoverySources,
    discovery: &Discovery,
    settings: &KubeSettings,
) -> (Option<KubeconfigWatcher>, Receiver<DiscoveryEvent>) {
    let started = KubeconfigWatcher::start(
        sources.clone(),
        discovery.clone(),
        settings.discovery_debounce,
    );
    match started {
        Ok((watcher, events)) => (Some(watcher), events),
        Err(error) => {
            tracing::warn!(%error, "kubeconfig files are not watched");
            (None, async_channel::unbounded().1)
        }
    }
}

async fn scan(sources: &DiscoverySources) -> Discovery {
    let sources = sources.clone();
    tokio::task::spawn_blocking(move || discover(&sources))
        .await
        .unwrap_or_default()
}

fn ticker(period: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

async fn run(backend: LiveBackend, updates: Sender<Update>, commands: Receiver<Command>) {
    let LiveBackend {
        settings,
        sources,
        store,
    } = backend;
    let env = Arc::new(capture(&user_shell(), settings.shell_env_timeout).await);
    let discovery = scan(&sources).await;
    let (mut _watcher, mut discovery_events) = watch_kubeconfigs(&sources, &discovery, &settings);
    let runtime = tokio::runtime::Handle::current();
    let (fetched, fetched_events) = async_channel::unbounded();
    let [mut usage_tick, mut prometheus_tick] =
        [settings.metrics_interval, settings.prometheus_interval].map(ticker);
    let (manager, cluster_events) = ClusterManager::new(runtime, settings.clone(), env.clone());
    let (actions, action_changes) = Actions::new(updates.clone());
    let gitops = GitOpsState::new(settings.git_timeout);
    let home = sources.home.clone();
    let mut live = Live {
        manager,
        store,
        discovery,
        sources,
        sources_changed: false,
        env,
        settings,
        updates: updates.clone(),
        home,
        clusters: BTreeMap::new(),
        disconnected: BTreeSet::new(),
        selected: None,
        page: Page::Overview,
        actions,
        gitops,
        page_subscriptions: Vec::new(),
        fetched,
        yaml: None,
        inspecting: None,
        yaml_task: None,
        workload_task: None,
        #[cfg(feature = "loco")]
        loco: crate::loco::LocoBackend::new(crate::loco::LocoSettings::default(), updates.clone()),
    };
    let gitops_changed = live.gitops.changed();
    live.start();
    let mut dirty = true;
    let mut publish = ticker(COALESCE_INTERVAL);
    loop {
        tokio::select! {
            event = discovery_events.recv(), if !discovery_events.is_closed() => {
                if let Ok(event) = event {
                    apply_discovery(&mut live.discovery, event);
                    live.reconcile();
                    dirty = true;
                }
            }
            event = cluster_events.recv() => {
                if let Ok(event) = event {
                    live.cluster_event(&event);
                    dirty = true;
                }
            }
            changed = action_changes.recv() => dirty |= changed.is_ok(),
            changed = gitops_changed.recv() => dirty |= changed.is_ok(),
            fetched = fetched_events.recv() => {
                if let Ok(fetched) = fetched {
                    live.fetched(fetched);
                    dirty = true;
                }
            }
            _ = usage_tick.tick() => live.poll_usage(),
            _ = prometheus_tick.tick() => {
                if live.page == Page::ClusterOverview {
                    live.poll_prometheus();
                }
            }
            command = commands.recv() => {
                let Ok(command) = command else { return };
                live.apply(&command);
                if let Some(restarted) = live.rescan_sources().await {
                    (_watcher, discovery_events) = restarted;
                }
                dirty = true;
            }
            _ = publish.tick() => {
                #[cfg(feature = "loco")]
                if live.loco.refresh() {
                    dirty = true;
                }
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

fn wanted(
    store: &ClusterStore,
    discovery: &Discovery,
    disconnected: &BTreeSet<ClusterId>,
) -> BTreeMap<ContextRef, ClusterId> {
    store
        .clusters()
        .iter()
        .filter(|saved| !disconnected.contains(&saved.id()))
        .filter(|saved| discovery.get(&saved.context_ref()).is_some())
        .map(|saved| (saved.context_ref(), saved.id()))
        .collect()
}

fn set_aside_notice(store: &ClusterStore) -> Option<Notice> {
    let kept = store.set_aside()?;
    Some(Notice {
        status: Status::Warn,
        title: SET_ASIDE_TITLE.to_owned(),
        detail: Some(format!("Kept as {}", kept.display())),
    })
}

fn offline_summary(saved: &SavedCluster, disconnected: bool) -> ClusterSummary {
    let state = if disconnected {
        DISCONNECTED_STATE
    } else {
        MISSING_STATE
    };
    ClusterSummary {
        id: saved.id(),
        name: saved.name.clone(),
        status: Status::Offline,
        state: state.to_owned(),
        distro: String::new(),
        region: String::new(),
        version: None,
        nodes: None,
        problems: Counts::default(),
        nav: BTreeMap::new(),
        metrics: None,
    }
}

pub fn auth_label(auth: &AuthKind) -> String {
    match auth.exec() {
        Some(exec) => exec.summary(),
        None => auth.to_string(),
    }
}

pub fn kubeconfig_files(
    discovery: &Discovery,
    store: &ClusterStore,
    home: Option<&Path>,
) -> Vec<KubeconfigFile> {
    let mut files: Vec<(PathBuf, KubeconfigFile)> = Vec::new();
    let available = discovery
        .contexts
        .iter()
        .filter(|context| !store.contains(&context.id));
    for context in available {
        let choice = ContextChoice {
            id: context.id.clone(),
            server: context.server.clone(),
            auth: auth_label(&context.auth),
            selected: false,
        };
        if let Some((_, file)) = files.iter_mut().find(|(path, _)| path == &context.id.file) {
            file.contexts.push(choice);
        } else {
            let file = KubeconfigFile {
                label: home_relative(&context.id.file, home),
                contexts: vec![choice],
            };
            files.push((context.id.file.clone(), file));
        }
    }
    files.into_iter().map(|(_, file)| file).collect()
}

pub fn check_line(result: &CheckResult) -> CheckLine {
    let status = match result.status {
        checks::CheckStatus::Ok => fleet_core::CheckStatus::Ok,
        checks::CheckStatus::Warn => fleet_core::CheckStatus::Warn,
        checks::CheckStatus::Err => fleet_core::CheckStatus::Err,
        checks::CheckStatus::Info => fleet_core::CheckStatus::Info,
    };
    CheckLine {
        status,
        detail: result.detail.clone(),
        remedy: result.remedy.clone(),
        duration: result.duration,
    }
}

pub fn verdict(verdict: checks::Verdict) -> Verdict {
    match verdict {
        checks::Verdict::Ready => Verdict::Ready,
        checks::Verdict::CannotReach => Verdict::CannotReach,
        checks::Verdict::CannotAuthenticate => Verdict::CannotAuthenticate,
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
    use crate::provider::{ExecCommand, Provider};

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
    fn picker_lists_unadded_contexts_by_file() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let mut store = ClusterStore::empty(dir.path());
        let added = NewCluster {
            context: context("/h/.kube/config", "kind-fleet-a").id,
            name: "fleet-a".to_owned(),
        };
        assert!(store.add(&[added]).is_ok());
        let mut eks = context("/h/.kube/eks/sandbox.yaml", "sandbox");
        eks.server = Some("https://x.eks.amazonaws.com".to_owned());
        eks.auth = AuthKind::Exec(ExecCommand {
            command: "aws".to_owned(),
            args: ["eks", "get-token", "--cluster-name", "sandbox"]
                .map(str::to_owned)
                .to_vec(),
        });
        let discovery = Discovery {
            files: Vec::new(),
            contexts: vec![
                context("/h/.kube/config", "kind-fleet-a"),
                context("/h/.kube/config", "kind-fleet-b"),
                eks,
            ],
            errors: Vec::new(),
        };
        let files = kubeconfig_files(&discovery, &store, Some(Path::new("/h")));
        let labels: Vec<&str> = files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["~/.kube/config", "~/.kube/eks/sandbox.yaml"]);
        let shown: Vec<(String, String, bool)> = files
            .iter()
            .flat_map(|f| &f.contexts)
            .map(|c| (c.id.context.clone(), c.meta(), c.selected))
            .collect();
        assert_eq!(
            shown,
            [
                ("kind-fleet-b".to_owned(), "none".to_owned(), false),
                (
                    "sandbox".to_owned(),
                    "https://x.eks.amazonaws.com · aws eks get-token".to_owned(),
                    false
                ),
            ]
        );
    }

    #[test]
    fn saved_clusters_without_a_context_show_offline() {
        let saved = SavedCluster {
            file: PathBuf::from("/gone"),
            context: "old".to_owned(),
            name: "old".to_owned(),
            preferences: SavedPreferences::default(),
        };
        let summary = offline_summary(&saved, false);
        assert_eq!(summary.status, Status::Offline);
        assert_eq!(summary.state, MISSING_STATE);
        assert_eq!(offline_summary(&saved, true).state, DISCONNECTED_STATE);
    }

    #[test]
    fn disconnected_clusters_stay_saved_but_are_not_watched() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let mut store = ClusterStore::empty(dir.path());
        let added = ["kind-fleet-a", "kind-fleet-b", "gone"].map(|name| NewCluster {
            context: context("/h/.kube/config", name).id,
            name: name.to_owned(),
        });
        assert!(store.add(&added).is_ok());
        let discovery = Discovery {
            files: Vec::new(),
            contexts: vec![
                context("/h/.kube/config", "kind-fleet-a"),
                context("/h/.kube/config", "kind-fleet-b"),
            ],
            errors: Vec::new(),
        };
        let names = |disconnected: &BTreeSet<ClusterId>| -> Vec<String> {
            wanted(&store, &discovery, disconnected)
                .into_values()
                .map(|id| id.to_string())
                .collect()
        };
        assert_eq!(names(&BTreeSet::new()), ["kind-fleet-a", "kind-fleet-b"]);
        let off = BTreeSet::from([ClusterId::new("kind-fleet-b")]);
        assert_eq!(names(&off), ["kind-fleet-a"]);
        assert_eq!(store.clusters().len(), 3);
    }

    #[test]
    fn a_set_aside_store_raises_a_notice() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        assert_eq!(set_aside_notice(&ClusterStore::empty(dir.path())), None);
        assert!(std::fs::write(dir.path().join("clusters.json"), "{").is_ok());
        let Ok(store) = ClusterStore::open(dir.path()) else {
            panic!("open");
        };
        let notice = set_aside_notice(&store);
        assert_eq!(notice.as_ref().map(|n| n.status), Some(Status::Warn));
        assert_eq!(notice.map(|n| n.title), Some(SET_ASIDE_TITLE.to_owned()));
    }

    #[test]
    fn check_results_map_to_core_lines() {
        let result = CheckResult {
            kind: checks::CheckKind::Authenticate,
            status: checks::CheckStatus::Err,
            detail: "ExpiredToken".to_owned(),
            remedy: Some("aws sso login".to_owned()),
            duration: Duration::from_millis(5),
        };
        let line = check_line(&result);
        assert_eq!(line.status, fleet_core::CheckStatus::Err);
        assert_eq!(line.remedy.as_deref(), Some("aws sso login"));
        assert_eq!(
            verdict(checks::Verdict::CannotAuthenticate),
            Verdict::CannotAuthenticate
        );
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

    #[test]
    fn settings_carry_the_kubeconfig_connection() {
        let mut gke = context("/h/.kube/config", "gke_qa");
        gke.server = Some("https://10.0.0.1".to_owned());
        gke.namespace = Some("payments".to_owned());
        gke.auth = AuthKind::Exec(ExecCommand {
            command: "/usr/local/bin/gke-gcloud-auth-plugin".to_owned(),
            args: Vec::new(),
        });
        let settings = cluster_settings(ClusterId::new("gke_qa"), &gke, false, None);
        assert_eq!(
            settings.connection.auth.label(),
            "exec · gke-gcloud-auth-plugin"
        );
        assert_eq!(settings.connection.server, "https://10.0.0.1");
        assert_eq!(settings.connection.kubeconfig, "/h/.kube/config");
        assert_eq!(
            settings.preferences.default_namespace.as_deref(),
            Some("payments")
        );
        assert!(!settings.auth_expired);
    }
}
