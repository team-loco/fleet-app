use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub use fleet_core::ContextRef;
use kube::config::{AuthInfo, Cluster, Kubeconfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::kubeconfig_reason;
use crate::provider::{AuthKind, Provider, detect_auth, detect_provider};

const KUBECONFIG_VAR: &str = "KUBECONFIG";
const KUBE_DIR: &str = ".kube";
const DEFAULT_FILE: &str = "config";
const SKIPPED_DIRS: [&str; 2] = ["cache", "http-cache"];
const MAX_KUBECONFIG_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SCAN_DEPTH: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeContext {
    pub id: ContextRef,
    pub cluster: String,
    pub user: Option<String>,
    pub namespace: Option<String>,
    pub server: Option<String>,
    pub provider: Provider,
    pub auth: AuthKind,
    pub is_current: bool,
    pub trust: Fingerprint,
    pub credentials: Fingerprint,
}

impl KubeContext {
    pub fn same_target(&self, other: &Self) -> bool {
        self.id == other.id
            && self.cluster == other.cluster
            && self.user == other.user
            && self.server == other.server
            && self.trust == other.trust
            && self.credentials == other.credentials
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn of(parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for part in parts {
            let length = u64::try_from(part.len()).unwrap_or(u64::MAX);
            hasher.update(length.to_be_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        let hex = digest.iter().fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        });
        Self(hex)
    }
}

fn file_bytes(path: Option<&str>) -> Vec<u8> {
    path.and_then(|path| fs::read(path).ok())
        .unwrap_or_default()
}

fn trust_of(cluster: Option<&Cluster>) -> Fingerprint {
    let declared = serde_json::to_vec(&cluster).unwrap_or_default();
    let authority = file_bytes(cluster.and_then(|c| c.certificate_authority.as_deref()));
    Fingerprint::of(&[&declared, &authority])
}

fn credentials_of(auth: Option<&AuthInfo>) -> Fingerprint {
    let declared = serde_json::to_vec(&auth).unwrap_or_default();
    let token = file_bytes(auth.and_then(|a| a.token_file.as_deref()));
    let certificate = file_bytes(auth.and_then(|a| a.client_certificate.as_deref()));
    let key = file_bytes(auth.and_then(|a| a.client_key.as_deref()));
    Fingerprint::of(&[&declared, &token, &certificate, &key])
}

#[derive(Debug, Clone, Default)]
pub struct DiscoverySources {
    pub kubeconfig_env: Option<OsString>,
    pub home: Option<PathBuf>,
    pub folders: Vec<PathBuf>,
}

impl DiscoverySources {
    pub fn from_env() -> Self {
        Self {
            kubeconfig_env: std::env::var_os(KUBECONFIG_VAR),
            home: std::env::home_dir(),
            folders: Vec::new(),
        }
    }

    pub fn kube_dir(&self) -> Option<PathBuf> {
        self.home.as_ref().map(|home| home.join(KUBE_DIR))
    }

    pub fn explicit_files(&self) -> Vec<PathBuf> {
        let from_env: Vec<PathBuf> = self
            .kubeconfig_env
            .as_ref()
            .map(|value| {
                std::env::split_paths(value)
                    .filter(|path| !path.as_os_str().is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let default_file = self.kube_dir().map(|dir| dir.join(DEFAULT_FILE));
        from_env.into_iter().chain(default_file).collect()
    }

    pub fn watch_roots(&self) -> Vec<PathBuf> {
        let mut roots: BTreeSet<PathBuf> = self
            .explicit_files()
            .iter()
            .filter_map(|file| file.parent().map(Path::to_path_buf))
            .collect();
        roots.extend(self.kube_dir());
        roots.extend(self.folders.iter().cloned());
        roots.into_iter().filter(|root| root.is_dir()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryError {
    pub file: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    pub files: Vec<PathBuf>,
    pub contexts: Vec<KubeContext>,
    pub errors: Vec<DiscoveryError>,
}

impl Discovery {
    pub fn get(&self, id: &ContextRef) -> Option<&KubeContext> {
        self.contexts.iter().find(|context| &context.id == id)
    }
}

pub fn discover(sources: &DiscoverySources) -> Discovery {
    let mut seen = BTreeSet::new();
    let mut discovery = Discovery::default();
    for file in sources.explicit_files() {
        if !file.exists() {
            continue;
        }
        load_file(&file, true, &mut seen, &mut discovery);
    }
    for dir in sources.kube_dir().iter().chain(&sources.folders) {
        let mut scanned = Vec::new();
        scan_dir(dir, 0, &mut scanned);
        scanned.sort();
        for file in scanned {
            load_file(&file, false, &mut seen, &mut discovery);
        }
    }
    discovery
}

fn scan_dir(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let skipped = SKIPPED_DIRS.iter().any(|skip| name == *skip);
        if file_type.is_dir() && !skipped {
            scan_dir(&path, depth + 1, out);
        } else if is_candidate(&path) {
            out.push(path);
        }
    }
}

fn is_candidate(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_KUBECONFIG_BYTES)
}

fn load_file(file: &Path, explicit: bool, seen: &mut BTreeSet<PathBuf>, out: &mut Discovery) {
    let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    if !seen.insert(canonical) {
        return;
    }
    match parse_file(file) {
        Ok(contexts) if contexts.is_empty() && !explicit => {}
        Ok(contexts) => {
            out.files.push(file.to_path_buf());
            out.contexts.extend(contexts);
        }
        Err(message) if explicit => out.errors.push(DiscoveryError {
            file: file.to_path_buf(),
            message,
        }),
        Err(_) => {}
    }
}

pub(crate) fn looks_like_kubeconfig(config: &Kubeconfig) -> bool {
    let declared = config.kind.as_deref() == Some("Config");
    let has_parts = !config.clusters.is_empty() || !config.auth_infos.is_empty();
    !config.contexts.is_empty() && (declared || has_parts)
}

fn parse_file(file: &Path) -> Result<Vec<KubeContext>, String> {
    let config = Kubeconfig::read_from(file).map_err(|err| kubeconfig_reason(&err))?;
    if !looks_like_kubeconfig(&config) {
        return Ok(Vec::new());
    }
    Ok(contexts_of(file, &config))
}

pub fn contexts_of(file: &Path, config: &Kubeconfig) -> Vec<KubeContext> {
    config
        .contexts
        .iter()
        .filter_map(|named| {
            let context = named.context.as_ref()?;
            let cluster = config
                .clusters
                .iter()
                .find(|cluster| cluster.name == context.cluster)
                .and_then(|named| named.cluster.as_ref());
            let auth_info = context.user.as_ref().and_then(|user| {
                config
                    .auth_infos
                    .iter()
                    .find(|auth| &auth.name == user)
                    .and_then(|named| named.auth_info.as_ref())
            });
            let server = cluster.and_then(|cluster| cluster.server.clone());
            let trust = trust_of(cluster);
            let credentials = credentials_of(auth_info);
            let auth = detect_auth(auth_info);
            let provider = detect_provider(&named.name, server.as_deref(), &auth);
            Some(KubeContext {
                id: ContextRef {
                    file: file.to_path_buf(),
                    context: named.name.clone(),
                },
                cluster: context.cluster.clone(),
                user: context.user.clone(),
                namespace: context.namespace.clone(),
                server,
                provider,
                auth,
                is_current: config.current_context.as_deref() == Some(named.name.as_str()),
                trust,
                credentials,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Added(KubeContext),
    Changed(KubeContext),
    Removed(ContextRef),
    FileError(DiscoveryError),
}

pub fn diff(previous: &Discovery, next: &Discovery) -> Vec<DiscoveryEvent> {
    let mut events = Vec::new();
    for context in &next.contexts {
        match previous.get(&context.id) {
            None => events.push(DiscoveryEvent::Added(context.clone())),
            Some(old) if old != context => events.push(DiscoveryEvent::Changed(context.clone())),
            Some(_) => {}
        }
    }
    for context in &previous.contexts {
        if next.get(&context.id).is_none() {
            events.push(DiscoveryEvent::Removed(context.id.clone()));
        }
    }
    for error in &next.errors {
        if !previous.errors.contains(error) {
            events.push(DiscoveryEvent::FileError(error.clone()));
        }
    }
    events
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;

    use super::*;

    pub(crate) fn kubeconfig(context: &str, server: &str) -> String {
        format!(
            "apiVersion: v1
kind: Config
current-context: {context}
clusters:
- name: {context}
  cluster:
    server: {server}
contexts:
- name: {context}
  context:
    cluster: {context}
    user: {context}
    namespace: apps
users:
- name: {context}
  user:
    token: abc
"
        )
    }

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            assert!(fs::create_dir_all(parent).is_ok());
        }
        assert!(fs::write(path, body).is_ok());
    }

    fn sources(home: &Path, kubeconfig: Option<OsString>) -> DiscoverySources {
        DiscoverySources {
            kubeconfig_env: kubeconfig,
            home: Some(home.to_path_buf()),
            folders: Vec::new(),
        }
    }

    fn names(discovery: &Discovery) -> Vec<String> {
        discovery
            .contexts
            .iter()
            .map(|context| context.id.context.clone())
            .collect()
    }

    #[test]
    fn finds_default_nested_and_env_files() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let kube = home.path().join(".kube");
        write(
            &kube.join("config"),
            &kubeconfig("kind-fleet-a", "https://127.0.0.1:6443"),
        );
        write(
            &kube.join("eks/sandbox.yaml"),
            &kubeconfig("sandbox", "https://x.eks.amazonaws.com"),
        );
        write(
            &kube.join("cache/discovery/x.json"),
            &kubeconfig("cached", "https://c"),
        );
        write(&kube.join("http-cache/x"), &kubeconfig("http", "https://h"));
        write(&kube.join("notes.txt"), "just some text: [1, 2]\n");
        write(&kube.join("kubens/state.yaml"), "foo: bar\n");
        let external = home.path().join("elsewhere/extra.yaml");
        write(&external, &kubeconfig("edge-k3s", "https://10.20.0.4:6443"));

        let discovery = discover(&sources(
            home.path(),
            Some(external.clone().into_os_string()),
        ));

        assert_eq!(
            names(&discovery),
            vec!["edge-k3s", "kind-fleet-a", "sandbox"]
        );
        assert_eq!(discovery.errors, Vec::new());
        let Some(sandbox) = discovery
            .contexts
            .iter()
            .find(|c| c.id.context == "sandbox")
        else {
            panic!("sandbox missing");
        };
        assert_eq!(sandbox.provider, Provider::Eks);
        assert_eq!(sandbox.auth, AuthKind::Token);
        assert_eq!(sandbox.namespace.as_deref(), Some("apps"));
        assert_eq!(sandbox.id.file, kube.join("eks/sandbox.yaml"));
        assert!(sandbox.is_current);
    }

    #[test]
    fn scans_added_folders_outside_kube_dir() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let watched = home.path().join("clusters/team");
        write(
            &watched.join("one.yaml"),
            &kubeconfig("team-one", "https://127.0.0.1:1"),
        );
        write(
            &watched.join("sub/two.yaml"),
            &kubeconfig("team-two", "https://127.0.0.1:2"),
        );
        write(&watched.join("readme.txt"), "not a kubeconfig\n");
        let mut src = sources(home.path(), None);
        src.folders = vec![watched.clone()];

        let discovery = discover(&src);

        assert_eq!(names(&discovery), vec!["team-one", "team-two"]);
        assert!(src.watch_roots().contains(&watched));
    }

    #[test]
    fn explicit_file_reported_once_with_errors() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let kube = home.path().join(".kube");
        write(
            &kube.join("config"),
            &kubeconfig("one", "https://127.0.0.1:1"),
        );
        let broken = home.path().join("broken.yaml");
        write(&broken, "clusters: [[[");
        let env = std::env::join_paths([broken.clone(), kube.join("config")]).ok();

        let discovery = discover(&sources(home.path(), env));

        assert_eq!(names(&discovery), vec!["one"]);
        assert_eq!(discovery.errors.len(), 1);
        assert_eq!(
            discovery.errors.first().map(|e| e.file.clone()),
            Some(broken)
        );
    }

    #[test]
    fn same_context_name_in_two_files_is_two_contexts() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let kube = home.path().join(".kube");
        write(
            &kube.join("config"),
            &kubeconfig("dup", "https://127.0.0.1:1"),
        );
        write(
            &kube.join("other.yaml"),
            &kubeconfig("dup", "https://127.0.0.1:2"),
        );

        let discovery = discover(&sources(home.path(), None));

        assert_eq!(names(&discovery), vec!["dup", "dup"]);
        assert_eq!(discovery.files.len(), 2);
    }

    #[test]
    fn rotated_credentials_and_trust_are_changes_without_exposing_them() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let config = home.path().join(".kube").join("config");
        let original = kubeconfig("lab", "https://127.0.0.1:1");
        write(&config, &original);
        let src = sources(home.path(), None);
        let before = discover(&src);

        write(
            &config,
            &original.replace("token: abc", "token: restricted"),
        );
        let rotated = discover(&src);
        let events = diff(&before, &rotated);
        let changed: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                DiscoveryEvent::Changed(context) => Some(context.id.context.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(changed, ["lab"]);
        let debug = format!("{rotated:?}");
        assert!(!debug.contains("restricted"), "{debug}");

        let with_ca = original.replace(
            "    server: https://127.0.0.1:1\n",
            "    server: https://127.0.0.1:1\n    certificate-authority-data: Zmlyc3Q=\n",
        );
        write(&config, &with_ca);
        let first = discover(&src);
        write(&config, &with_ca.replace("Zmlyc3Q=", "c2Vjb25k"));
        let second = discover(&src);
        assert_eq!(diff(&first, &second).len(), 1);
        let trust = |discovery: &Discovery| discovery.contexts.first().map(|c| c.trust.clone());
        assert_ne!(trust(&first), trust(&second));
        let credentials =
            |discovery: &Discovery| discovery.contexts.first().map(|c| c.credentials.clone());
        assert_eq!(credentials(&first), credentials(&second));
    }

    #[test]
    fn diff_reports_added_changed_removed() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let kube = home.path().join(".kube");
        write(
            &kube.join("config"),
            &kubeconfig("a", "https://127.0.0.1:1"),
        );
        write(
            &kube.join("b.yaml"),
            &kubeconfig("b", "https://127.0.0.1:2"),
        );
        let src = sources(home.path(), None);
        let before = discover(&src);

        write(
            &kube.join("config"),
            &kubeconfig("a", "https://127.0.0.1:9"),
        );
        assert!(fs::remove_file(kube.join("b.yaml")).is_ok());
        write(
            &kube.join("c.yaml"),
            &kubeconfig("c", "https://127.0.0.1:3"),
        );
        let after = discover(&src);

        let summary: Vec<String> = diff(&before, &after)
            .iter()
            .map(|event| match event {
                DiscoveryEvent::Added(c) => format!("+{}", c.id.context),
                DiscoveryEvent::Changed(c) => format!("~{}", c.id.context),
                DiscoveryEvent::Removed(id) => format!("-{}", id.context),
                DiscoveryEvent::FileError(e) => format!("!{}", e.file.display()),
            })
            .collect();
        assert_eq!(summary, vec!["~a", "+c", "-b"]);
    }
}
