use std::error::Error;
use std::path::Path;

use flock_kube::discovery::{DiscoverySources, KubeContext, discover};
use kube::config::Kubeconfig;
use serde_json::json;

#[path = "common/kubeconfig.rs"]
mod kubeconfig;

const FIRST: &str = "kind-fleet-a";
const SECOND: &str = "kind-fleet-b";
const EXCLUDED: &str = "unrelated-context";
const SERVER: &str = "https://127.0.0.1:6443";

type TestResult = Result<(), Box<dyn Error>>;

fn write_config(path: &Path, names: &[&str]) -> TestResult {
    let contexts = names
        .iter()
        .map(|name| json!({"name": name, "context": {"cluster": name, "user": name}}))
        .collect::<Vec<_>>();
    let clusters = names
        .iter()
        .map(|name| json!({"name": name, "cluster": {"server": SERVER}}))
        .collect::<Vec<_>>();
    let users = names
        .iter()
        .map(|name| json!({"name": name, "user": {"token": name}}))
        .collect::<Vec<_>>();
    let value = json!({
        "apiVersion": "v1",
        "kind": "Config",
        "current-context": EXCLUDED,
        "contexts": contexts,
        "clusters": clusters,
        "users": users,
    });
    let yaml = serde_norway::to_string(&value)?;
    std::fs::write(path, yaml)?;
    Ok(())
}

fn selected(sources: &DiscoverySources, names: &[&str]) -> Vec<KubeContext> {
    let discovery = discover(sources);
    names
        .iter()
        .map(|name| {
            let Some(context) = discovery.contexts.iter().find(|c| c.id.context == *name) else {
                panic!("fixture context {name} missing: {discovery:?}");
            };
            context.clone()
        })
        .collect()
}

fn assert_isolated(contexts: &[KubeContext], names: &[&str]) -> TestResult {
    let home = kubeconfig::kind_only_home(contexts);
    let file = home.path().join(".kube").join("config");
    let config = Kubeconfig::read_from(&file)?;
    let context_names = config
        .contexts
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    let cluster_names = config
        .clusters
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    let user_names = config
        .auth_infos
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(context_names, names);
    assert_eq!(cluster_names, names);
    assert_eq!(user_names, names);
    let current = contexts.first().map(|context| context.id.context.as_str());
    assert_eq!(config.current_context.as_deref(), current);
    let sources = DiscoverySources {
        kubeconfig_env: None,
        home: Some(home.path().to_path_buf()),
        folders: Vec::new(),
    };
    let discovery = discover(&sources);
    assert_eq!(discovery.contexts.len(), names.len());
    for context in discovery.contexts {
        assert_eq!(context.id.file, file);
        assert_eq!(context.server.as_deref(), Some(SERVER));
    }
    Ok(())
}

#[test]
fn split_kubeconfig_keeps_both_clusters_and_excludes_unselected_credentials() -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = directory.path().join("first.yaml");
    let second = directory.path().join("second.yaml");
    write_config(&first, &[FIRST, EXCLUDED])?;
    write_config(&second, &[SECOND])?;
    let paths = std::env::join_paths([first, second])?;
    let sources = DiscoverySources {
        kubeconfig_env: Some(paths),
        home: None,
        folders: Vec::new(),
    };
    let contexts = selected(&sources, &[FIRST, SECOND]);
    assert_isolated(&contexts, &[FIRST, SECOND])
}

#[test]
fn shared_file_keeps_only_selected_contexts_and_sets_the_first_as_current() -> TestResult {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("merged.yaml");
    write_config(&file, &[FIRST, SECOND, EXCLUDED])?;
    let sources = DiscoverySources {
        kubeconfig_env: Some(file.into_os_string()),
        home: None,
        folders: Vec::new(),
    };
    let contexts = selected(&sources, &[SECOND]);
    assert_isolated(&contexts, &[SECOND])?;
    let contexts = selected(&sources, &[SECOND, FIRST]);
    assert_isolated(&contexts, &[FIRST, SECOND])?;
    Ok(())
}
