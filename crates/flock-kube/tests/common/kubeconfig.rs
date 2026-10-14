use flock_kube::discovery::KubeContext;
use kube::config::Kubeconfig;

pub fn kind_only_home(contexts: &[KubeContext]) -> tempfile::TempDir {
    let mut files: Vec<&std::path::Path> = contexts.iter().map(|c| c.id.file.as_path()).collect();
    let mut seen = std::collections::BTreeSet::new();
    files.retain(|file| seen.insert(*file));
    let mut merged: Option<Kubeconfig> = None;
    for file in files {
        let Ok(config) = Kubeconfig::read_from(file) else {
            panic!("kubeconfig readable");
        };
        merged = match merged {
            Some(previous) => match previous.merge(config) {
                Ok(both) => Some(both),
                Err(error) => panic!("kubeconfigs merge: {error}"),
            },
            None => Some(config),
        };
    }
    let Some(mut config) = merged else {
        panic!("at least one kind context");
    };
    let wanted = |name: &str| contexts.iter().any(|c| c.id.context == name);
    let clusters = |name: &str| contexts.iter().any(|c| c.cluster == name);
    let users = |name: &String| contexts.iter().any(|c| c.user.as_ref() == Some(name));
    config.contexts.retain(|named| wanted(&named.name));
    config.clusters.retain(|named| clusters(&named.name));
    config.auth_infos.retain(|named| users(&named.name));
    let Some(first) = contexts.first() else {
        panic!("at least one kind context");
    };
    config.current_context = Some(first.id.context.clone());
    let Ok(home) = tempfile::tempdir() else {
        panic!("tempdir");
    };
    let kube_dir = home.path().join(".kube");
    assert!(std::fs::create_dir_all(&kube_dir).is_ok());
    let Ok(yaml) = serde_norway::to_string(&config) else {
        panic!("serialize kubeconfig");
    };
    assert!(std::fs::write(kube_dir.join("config"), yaml).is_ok());
    home
}
