use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use kube::config::Kubeconfig;

use crate::discovery::looks_like_kubeconfig;
use crate::error::{Error, Result};

const FILE_EXTENSION: &str = "yaml";
const FILE_MODE: u32 = 0o600;
const NO_CONTEXTS: &str = "it has no contexts.";

pub fn save(dir: &Path, name: &str, yaml: &str) -> Result<PathBuf> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(Error::InvalidKubeconfigName);
    }
    let config =
        Kubeconfig::from_yaml(yaml).map_err(|error| Error::InvalidKubeconfig(error.to_string()))?;
    if !looks_like_kubeconfig(&config) {
        return Err(Error::InvalidKubeconfig(NO_CONTEXTS.to_owned()));
    }
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.{FILE_EXTENSION}"));
    let created = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(&path);
    let mut file = match created {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(Error::KubeconfigExists(name.to_owned()));
        }
        Err(error) => return Err(error.into()),
    };
    file.write_all(yaml.as_bytes())?;
    Ok(path)
}

fn valid_name(name: &str) -> bool {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    !name.is_empty() && !name.starts_with('.') && name.chars().all(allowed)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::discovery::tests::kubeconfig;

    fn dir() -> tempfile::TempDir {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        dir
    }

    #[test]
    fn saves_a_private_file_named_after_the_name() {
        let dir = dir();
        let target = dir.path().join("kubeconfigs");
        let yaml = kubeconfig("kind-paste", "https://127.0.0.1:1");

        let saved = save(&target, " lab ", &yaml);

        let Ok(path) = saved else {
            panic!("{saved:?}");
        };
        assert_eq!(path, target.join("lab.yaml"));
        assert_eq!(fs::read_to_string(&path).ok(), Some(yaml));
        let mode = fs::metadata(&path).map(|meta| meta.permissions().mode() & 0o777);
        assert_eq!(mode.ok(), Some(FILE_MODE));
    }

    #[test]
    fn rejects_text_that_is_not_a_kubeconfig() {
        let dir = dir();
        for yaml in [
            "",
            "foo: bar\n",
            "clusters: [[[",
            "kind: Config\ncontexts: []\n",
        ] {
            let saved = save(dir.path(), "x", yaml);
            assert!(
                matches!(saved, Err(Error::InvalidKubeconfig(_))),
                "{yaml:?}: {saved:?}"
            );
        }
        assert_eq!(fs::read_dir(dir.path()).map(Iterator::count).ok(), Some(0));
    }

    #[test]
    fn rejects_unsafe_names_and_never_overwrites() {
        let dir = dir();
        let yaml = kubeconfig("c", "https://127.0.0.1:1");
        for name in ["", "  ", "../escape", "a/b", ".hidden", "sp ace"] {
            let saved = save(dir.path(), name, &yaml);
            assert!(
                matches!(saved, Err(Error::InvalidKubeconfigName)),
                "{name:?}: {saved:?}"
            );
        }
        assert!(save(dir.path(), "one", &yaml).is_ok());
        let again = save(dir.path(), "one", &yaml);
        assert!(
            matches!(again, Err(Error::KubeconfigExists(_))),
            "{again:?}"
        );
    }
}
