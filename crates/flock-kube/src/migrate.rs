use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

use crate::store::{APP_ID, APP_SUPPORT, CACHES};

const PREVIOUS_APP_ID: &str = "dev.fleet.app";
const STAGING_EXTENSION: &str = "migrating";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Migration {
    Renamed,
    Copied,
}

pub fn previous_app_dirs(home: &Path) {
    for library in [APP_SUPPORT, CACHES] {
        let base = home.join(library);
        let from = base.join(PREVIOUS_APP_ID);
        let to = base.join(APP_ID);
        match migrate(&from, &to, |from, to| fs::rename(from, to)) {
            Ok(Some(how)) => tracing::info!(
                from = %from.display(),
                to = %to.display(),
                ?how,
                "moved data from the previous app id"
            ),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                %error,
                from = %from.display(),
                to = %to.display(),
                "could not move data from the previous app id"
            ),
        }
    }
}

fn migrate(
    from: &Path,
    to: &Path,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<Option<Migration>> {
    if fs::exists(to)? || !from.is_dir() {
        return Ok(None);
    }
    if rename(from, to).is_ok() {
        return Ok(Some(Migration::Renamed));
    }
    let staging = to.with_added_extension(STAGING_EXTENSION);
    remove_dir(&staging)?;
    let copied = copy_dir(from, &staging).and_then(|()| fs::rename(&staging, to));
    if let Err(error) = copied {
        remove_dir(&staging)?;
        return Err(error);
    }
    Ok(Some(Migration::Copied))
}

fn remove_dir(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&source, &target)?;
        } else if kind.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&source)?, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::*;

    const PRIVATE: u32 = 0o600;
    const MODE_BITS: u32 = 0o777;

    fn seed(dir: &Path) -> PathBuf {
        let nested = dir.join("kubeconfigs");
        let file = nested.join("prod.yaml");
        assert!(fs::create_dir_all(&nested).is_ok());
        assert!(fs::write(dir.join("clusters.json"), "{}").is_ok());
        assert!(fs::write(&file, "kind: Config").is_ok());
        assert!(fs::set_permissions(&file, fs::Permissions::from_mode(PRIVATE)).is_ok());
        file
    }

    fn refuse(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::from(ErrorKind::CrossesDevices))
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).map_or(0, |meta| meta.permissions().mode() & MODE_BITS)
    }

    #[test]
    fn the_previous_dir_is_renamed_to_the_new_one() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let from = dir.path().join(PREVIOUS_APP_ID);
        let to = dir.path().join(APP_ID);
        seed(&from);
        assert_eq!(
            migrate(&from, &to, |a, b| fs::rename(a, b)).ok(),
            Some(Some(Migration::Renamed))
        );
        assert!(!from.exists());
        assert_eq!(
            fs::read_to_string(to.join("clusters.json")).ok().as_deref(),
            Some("{}")
        );
        assert_eq!(mode(&to.join("kubeconfigs/prod.yaml")), PRIVATE);
    }

    #[test]
    fn a_failed_rename_copies_and_keeps_the_previous_dir() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let from = dir.path().join(PREVIOUS_APP_ID);
        let to = dir.path().join(APP_ID);
        let original = seed(&from);
        assert_eq!(
            migrate(&from, &to, refuse).ok(),
            Some(Some(Migration::Copied))
        );
        assert!(original.exists());
        let copied = to.join("kubeconfigs/prod.yaml");
        assert_eq!(
            fs::read_to_string(&copied).ok().as_deref(),
            Some("kind: Config")
        );
        assert_eq!(mode(&copied), PRIVATE);
        assert!(!to.with_added_extension(STAGING_EXTENSION).exists());
    }

    #[test]
    fn an_existing_new_dir_is_left_alone() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let from = dir.path().join(PREVIOUS_APP_ID);
        let to = dir.path().join(APP_ID);
        seed(&from);
        assert!(fs::create_dir_all(&to).is_ok());
        assert_eq!(
            migrate(&from, &to, |a, b| fs::rename(a, b)).ok(),
            Some(None)
        );
        assert!(from.join("clusters.json").exists());
        assert!(!to.join("clusters.json").exists());
    }

    #[test]
    fn nothing_happens_without_a_previous_dir() {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        let to = dir.path().join(APP_ID);
        let from = dir.path().join(PREVIOUS_APP_ID);
        assert_eq!(
            migrate(&from, &to, |a, b| fs::rename(a, b)).ok(),
            Some(None)
        );
        assert!(!to.exists());
    }

    #[test]
    fn data_and_caches_both_move_to_the_new_app_id() {
        let Ok(home) = tempfile::tempdir() else {
            panic!("tempdir");
        };
        for library in [APP_SUPPORT, CACHES] {
            seed(&home.path().join(library).join(PREVIOUS_APP_ID));
        }
        previous_app_dirs(home.path());
        for library in [APP_SUPPORT, CACHES] {
            let base = home.path().join(library);
            assert!(!base.join(PREVIOUS_APP_ID).exists());
            assert!(base.join(APP_ID).join("clusters.json").exists());
        }
    }
}
