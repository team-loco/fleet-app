use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fleet_core::add_cluster::home_relative;
use fleet_core::resource::page_for_kind;
use fleet_core::settings::{Auth, ClusterSettings, Connection};
use fleet_core::{
    Backend, ChangesLog, CheckLine, ClusterCatalog, ClusterId, ClusterSummary, Command,
    ContextChoice, Counts, FleetHome, GitOps, KubeconfigFile, NewCluster, Page, Preferences,
    Receiver, ResourceRef, Sender, Snapshot, Status, Update, Verdict,
};
use tokio::time::MissedTickBehavior;

use crate::checks::{self, CheckResult, run_checks_streaming};
use crate::discovery::{
    ContextRef, Discovery, DiscoveryEvent, DiscoverySources, KubeContext, discover,
};
use crate::manager::{
    COALESCE_INTERVAL, ClusterHandle, ClusterManager, ConnectionStatus, Subscription,
};
use crate::provider::AuthKind;
use crate::resources::WatchKind;
use crate::settings::KubeSettings;
use crate::shell_env::{ShellEnv, capture, user_shell};
use crate::snapshot::{page_kind, summarize};
use crate::store::{ClusterStore, SavedCluster, SavedPreferences};
use crate::watch::KubeconfigWatcher;

const THREAD_NAME: &str = "fleet-kube";
const BASELINE_KINDS: [WatchKind; 2] = [WatchKind::Nodes, WatchKind::Pods];
const MISSING_STATE: &str = "Not in kubeconfig";
const MISSING_CONTEXT: &str = "This context is no longer in its kubeconfig file.";

pub struct LiveBackend {
    settings: KubeSettings,
    sources: DiscoverySources,
    store: ClusterStore,
}

impl LiveBackend {
    pub fn new(settings: KubeSettings, sources: DiscoverySources, store: ClusterStore) -> Self {
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

struct Connected {
    id: ClusterId,
    handle: ClusterHandle,
    _baseline: Vec<Subscription>,
}

struct Live {
    manager: ClusterManager,
    store: ClusterStore,
    discovery: Discovery,
    env: Arc<ShellEnv>,
    settings: KubeSettings,
    updates: Sender<Update>,
    home: Option<PathBuf>,
    clusters: BTreeMap<ContextRef, Connected>,
    selected: Option<ClusterId>,
    page: Page,
    page_subscription: Option<Subscription>,
}

impl Live {
    fn reconcile(&mut self) {
        let ids: BTreeMap<ContextRef, ClusterId> = self
            .store
            .clusters()
            .iter()
            .filter(|saved| self.discovery.get(&saved.context_ref()).is_some())
            .map(|saved| (saved.context_ref(), saved.id()))
            .collect();
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

    fn apply(&mut self, command: &Command) -> bool {
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
            Command::Flux(..) => {}
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
            Command::OpenAddCluster(_) => {
                let files = kubeconfig_files(&self.discovery, &self.store, self.home.as_deref());
                self.publish(Update::Contexts(files));
            }
            Command::CheckContexts(ids) => self.check(ids),
            Command::AddClusters(added) => {
                self.add(added);
                return true;
            }
            Command::RemoveCluster(id) => {
                self.remove(id);
                return true;
            }
            Command::WatchWorkload { .. } => {}
            Command::StreamLogs(_) => {}
            Command::Exec(_) => {}
            Command::RestartRollout(_) => {}
            Command::Scale { .. } => {}
            Command::Rollback { .. } => {}
            #[cfg(feature = "loco")]
            Command::Loco(_) => {}
            Command::CopySpec { .. } => {}
            Command::ShowClusterSettings(_) => {}
            Command::SetPreferences(id, preferences) => {
                return self.set_preferences(id, preferences);
            }
            Command::Connect(_) => {}
            Command::Reauthenticate(_) => {}
            Command::Disconnect(_) => {}
        }
        false
    }

    fn publish(&self, update: Update) {
        if self.updates.try_send(update).is_err() {
            tracing::debug!("UI stopped listening for updates");
        }
    }

    fn add(&mut self, added: &[NewCluster]) {
        if let Err(error) = self.store.add(added) {
            tracing::error!(%error, path = %self.store.path().display(), "saving clusters failed");
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

    fn open(&mut self, resource: &ResourceRef) {
        self.selected = Some(resource.cluster.clone());
        self.page = page_for_kind(&resource.kind);
        self.resubscribe_page();
    }

    fn snapshot(&self) -> Snapshot {
        let connected = self
            .clusters
            .values()
            .map(|cluster| summarize(cluster.id.clone(), cluster.id.to_string(), &cluster.handle));
        let missing = self
            .store
            .clusters()
            .iter()
            .filter(|saved| !self.clusters.contains_key(&saved.context_ref()))
            .map(missing_summary);
        let mut clusters: Vec<_> = connected.chain(missing).collect();
        clusters.sort_by(|a, b| a.name.cmp(&b.name));
        let settings: Vec<ClusterSettings> = self
            .clusters
            .values()
            .map(|cluster| {
                let status = cluster.handle.status();
                let saved = self.store.get(&cluster.id).map(|c| &c.preferences);
                cluster_settings(cluster.id.clone(), cluster.handle.context(), &status, saved)
            })
            .collect();
        let sources: BTreeSet<&Path> = self.clusters.keys().map(|id| id.file.as_path()).collect();
        Snapshot {
            clusters,
            fleet_nav: BTreeMap::new(),
            home: FleetHome::default(),
            resources: Vec::new(),
            changes: ChangesLog::default(),
            details: BTreeMap::new(),
            gitops: GitOps::default(),
            flux: Vec::new(),
            #[cfg(feature = "loco")]
            loco: BTreeMap::new(),
            comparisons: Vec::new(),
            catalog: ClusterCatalog {
                sources: sources.len(),
                clusters: settings,
            },
            pods: Vec::new(),
        }
    }
}

pub fn cluster_settings(
    cluster: ClusterId,
    context: &KubeContext,
    status: &ConnectionStatus,
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
        auth_expired: matches!(status, ConnectionStatus::AuthExpired(_)),
        prometheus: None,
        preferences,
    }
}

async fn run(backend: LiveBackend, updates: Sender<Update>, commands: Receiver<Command>) {
    let LiveBackend {
        settings,
        sources,
        store,
    } = backend;
    let env = Arc::new(capture(&user_shell(), settings.shell_env_timeout).await);
    let scan_sources = sources.clone();
    let discovery = tokio::task::spawn_blocking(move || discover(&scan_sources))
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
    let (manager, cluster_events) = ClusterManager::new(runtime, settings.clone(), env.clone());
    let mut live = Live {
        manager,
        store,
        discovery,
        env,
        settings,
        updates: updates.clone(),
        home: sources.home.clone(),
        clusters: BTreeMap::new(),
        selected: None,
        page: Page::Overview,
        page_subscription: None,
    };
    live.reconcile();
    let mut dirty = true;
    let mut publish = tokio::time::interval(COALESCE_INTERVAL);
    publish.set_missed_tick_behavior(MissedTickBehavior::Skip);
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
                if event.is_ok() {
                    dirty = true;
                }
            }
            command = commands.recv() => {
                let Ok(command) = command else { return };
                if live.apply(&command) {
                    dirty = true;
                }
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

fn missing_summary(saved: &SavedCluster) -> ClusterSummary {
    ClusterSummary {
        id: saved.id(),
        name: saved.name.clone(),
        status: Status::Offline,
        state: MISSING_STATE.to_owned(),
        distro: String::new(),
        region: String::new(),
        version: None,
        nodes: None,
        problems: Counts::default(),
        nav: BTreeMap::new(),
        metrics: None,
        #[cfg(feature = "loco")]
        loco_agent_synced: None,
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
        let summary = missing_summary(&saved);
        assert_eq!(summary.status, Status::Offline);
        assert_eq!(summary.state, MISSING_STATE);
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
        let settings = cluster_settings(
            ClusterId::new("gke_qa"),
            &gke,
            &ConnectionStatus::Connected,
            None,
        );
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
