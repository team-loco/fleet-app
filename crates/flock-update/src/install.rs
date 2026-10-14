use std::fs;
use std::path::{Path, PathBuf};

use crate::stage::{Layout, run};
use crate::{Error, Result};

const BUNDLE_EXTENSION: &str = "app";
const PROBE: &str = ".flock-write-probe";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Bundle(PathBuf),
    Manual,
}

pub fn target(executable: &Path) -> Target {
    let bundle = executable
        .ancestors()
        .find(|dir| dir.extension().is_some_and(|ext| ext == BUNDLE_EXTENSION));
    let Some(bundle) = bundle else {
        return Target::Manual;
    };
    match bundle.parent() {
        Some(parent) if writable(parent) => Target::Bundle(bundle.to_path_buf()),
        _ => Target::Manual,
    }
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!("{PROBE}-{}", std::process::id()));
    let created = fs::File::create(&probe).is_ok();
    if created {
        let _ = fs::remove_file(&probe);
    }
    created
}

pub fn swap(layout: &Layout, staged: &Path) -> Result<()> {
    if layout.previous.exists() {
        fs::remove_dir_all(&layout.previous).map_err(Error::io(&layout.previous))?;
    }
    fs::rename(&layout.bundle, &layout.previous).map_err(|error| Error::Swap(error.to_string()))?;
    let Err(error) = fs::rename(staged, &layout.bundle) else {
        return Ok(());
    };
    match fs::rename(&layout.previous, &layout.bundle) {
        Ok(()) => Err(Error::Swap(error.to_string())),
        Err(rollback) => Err(Error::Rollback {
            swap: error.to_string(),
            rollback: rollback.to_string(),
            previous: layout.previous.clone(),
        }),
    }
}

pub fn relaunch(bundle: &Path) -> Result<()> {
    run("open", &["-n".as_ref(), bundle.as_os_str()]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;
    use crate::testing::{ID, bundle_version, fake_bundle, temp};

    const READ_ONLY: u32 = 0o555;
    const WRITABLE: u32 = 0o755;

    fn layout(bundle: &Path) -> Layout {
        Layout::beside(bundle).unwrap_or_else(|error| panic!("layout: {error}"))
    }

    #[test]
    fn swapping_moves_the_old_bundle_aside_and_the_new_one_in() {
        let dir = temp();
        let running = fake_bundle(&dir.path().join("Applications"), ID, "0.1.0");
        let staged = fake_bundle(
            &dir.path().join("Applications/.flock-update/unpacked"),
            ID,
            "0.2.0",
        );
        let layout = layout(&running);
        assert!(swap(&layout, &staged).is_ok());
        assert_eq!(bundle_version(&running), "0.2.0");
        assert_eq!(bundle_version(&layout.previous), "0.1.0");
        assert!(!staged.exists());
    }

    #[test]
    fn a_failed_swap_puts_the_running_bundle_back() {
        let dir = temp();
        let running = fake_bundle(&dir.path().join("Applications"), ID, "0.1.0");
        let missing = dir
            .path()
            .join("Applications/.flock-update/unpacked/Flock.app");
        let layout = layout(&running);
        let result = swap(&layout, &missing);
        assert!(matches!(result, Err(Error::Swap(_))), "{result:?}");
        assert_eq!(bundle_version(&running), "0.1.0");
        assert!(!layout.previous.exists());
    }

    #[test]
    fn only_a_bundle_in_a_writable_folder_updates_itself() {
        let dir = temp();
        let apps = dir.path().join("Applications");
        let bundle = fake_bundle(&apps, ID, "0.1.0");
        let exe = bundle.join("Contents/MacOS/flock");
        assert_eq!(target(&exe), Target::Bundle(bundle));
        assert_eq!(
            target(Path::new("/Users/me/flock/target/debug/flock")),
            Target::Manual
        );
        assert!(fs::set_permissions(&apps, fs::Permissions::from_mode(READ_ONLY)).is_ok());
        let locked = target(&exe);
        assert!(fs::set_permissions(&apps, fs::Permissions::from_mode(WRITABLE)).is_ok());
        assert_eq!(locked, Target::Manual);
    }
}
