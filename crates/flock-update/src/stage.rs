use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use semver::Version;

use crate::{Error, Result, version};

const STAGING_DIR: &str = ".flock-update";
const PREVIOUS: &str = ".flock-previous.app";
const ZIP_FILE: &str = "update.zip";
const UNPACKED: &str = "unpacked";
const INFO_PLIST: &str = "Contents/Info.plist";
const IDENTIFIER_KEY: &str = "CFBundleIdentifier";
const VERSION_KEY: &str = "CFBundleShortVersionString";
const QUARANTINE: &str = "com.apple.quarantine";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub bundle: PathBuf,
    pub staging: PathBuf,
    pub previous: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub struct Expected<'a> {
    pub bundle_name: &'a str,
    pub identifier: &'a str,
    pub version: &'a Version,
    pub current: &'a Version,
}

pub(crate) fn run(tool: &'static str, args: &[&std::ffi::OsStr]) -> Result<String> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .map_err(Error::io(tool))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(Error::Tool { tool, detail });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn remove(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io(path)(error)),
    }
}

impl Layout {
    pub fn beside(bundle: &Path) -> Result<Self> {
        let parent = bundle
            .parent()
            .ok_or_else(|| Error::NotABundle(bundle.to_path_buf()))?;
        Ok(Self {
            bundle: bundle.to_path_buf(),
            staging: parent.join(STAGING_DIR),
            previous: parent.join(PREVIOUS),
        })
    }

    pub fn clean(&self) -> Result<()> {
        remove(&self.staging)?;
        remove(&self.previous)
    }

    pub fn stage(&self, zip: &[u8], expected: Expected<'_>) -> Result<PathBuf> {
        remove(&self.staging)?;
        fs::create_dir_all(&self.staging).map_err(Error::io(&self.staging))?;
        let archive = self.staging.join(ZIP_FILE);
        fs::write(&archive, zip).map_err(Error::io(&archive))?;
        let unpacked = self.staging.join(UNPACKED);
        run(
            "ditto",
            &[
                "-x".as_ref(),
                "-k".as_ref(),
                archive.as_os_str(),
                unpacked.as_os_str(),
            ],
        )?;
        fs::remove_file(&archive).map_err(Error::io(&archive))?;
        let staged = unpacked.join(expected.bundle_name);
        if !staged.is_dir() {
            return Err(Error::NotABundle(staged));
        }
        run(
            "xattr",
            &["-dr".as_ref(), QUARANTINE.as_ref(), staged.as_os_str()],
        )?;
        validate(&staged, expected)?;
        run(
            "codesign",
            &[
                "--verify".as_ref(),
                "--deep".as_ref(),
                "--strict".as_ref(),
                staged.as_os_str(),
            ],
        )?;
        Ok(staged)
    }
}

fn plist_value(bundle: &Path, key: &str) -> Result<String> {
    let plist = bundle.join(INFO_PLIST);
    run(
        "plutil",
        &[
            "-extract".as_ref(),
            key.as_ref(),
            "raw".as_ref(),
            "-o".as_ref(),
            "-".as_ref(),
            plist.as_os_str(),
        ],
    )
}

pub fn validate(bundle: &Path, expected: Expected<'_>) -> Result<()> {
    let identifier = plist_value(bundle, IDENTIFIER_KEY)?;
    if identifier != expected.identifier {
        return Err(Error::BundleId(identifier));
    }
    let found = version::parse(&plist_value(bundle, VERSION_KEY)?)?;
    if found != *expected.version {
        return Err(Error::BundleVersion {
            found: found.to_string(),
            expected: expected.version.to_string(),
        });
    }
    if found <= *expected.current {
        return Err(Error::NotNewer {
            found: found.to_string(),
            current: expected.current.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        BUNDLE, ID, bundle_version, fake_bundle, quarantine, quarantined, temp, zip,
    };

    fn expected<'a>(version: &'a Version, current: &'a Version) -> Expected<'a> {
        Expected {
            bundle_name: BUNDLE,
            identifier: ID,
            version,
            current,
        }
    }

    #[test]
    fn a_signed_zip_of_a_newer_flock_stages_beside_the_bundle() {
        let dir = temp();
        let running = fake_bundle(&dir.path().join("Applications"), ID, "0.1.0");
        let update = fake_bundle(&dir.path().join("build"), ID, "0.2.0");
        let plist = update.join(INFO_PLIST);
        quarantine(&plist);
        let bytes = zip(&update);
        let layout = Layout::beside(&running).unwrap_or_else(|error| panic!("{error}"));
        let new = Version::new(0, 2, 0);
        let current = Version::new(0, 1, 0);
        let staged = layout.stage(&bytes, expected(&new, &current));
        let staged = staged.unwrap_or_else(|error| panic!("stage: {error}"));
        assert!(staged.starts_with(dir.path().join("Applications").join(STAGING_DIR)));
        assert_eq!(bundle_version(&staged), "0.2.0");
        assert!(!quarantined(&staged.join(INFO_PLIST)));
        assert!(!layout.staging.join(ZIP_FILE).exists());
    }

    #[test]
    fn a_bundle_with_another_identifier_is_rejected() {
        let dir = temp();
        let other = fake_bundle(dir.path(), "com.example.other", "0.2.0");
        let new = Version::new(0, 2, 0);
        let current = Version::new(0, 1, 0);
        let result = validate(&other, expected(&new, &current));
        assert!(matches!(result, Err(Error::BundleId(id)) if id == "com.example.other"));
    }

    #[test]
    fn a_bundle_older_than_the_running_one_is_rejected() {
        let dir = temp();
        let old = fake_bundle(dir.path(), ID, "0.0.9");
        let claimed = Version::new(0, 2, 0);
        let current = Version::new(0, 1, 0);
        let mismatch = validate(&old, expected(&claimed, &current));
        assert!(matches!(mismatch, Err(Error::BundleVersion { .. })));
        let older = Version::new(0, 0, 9);
        let not_newer = validate(&old, expected(&older, &current));
        assert!(matches!(not_newer, Err(Error::NotNewer { .. })));
    }

    #[test]
    fn a_zip_without_flock_app_is_not_a_bundle() {
        let dir = temp();
        let running = fake_bundle(&dir.path().join("Applications"), ID, "0.1.0");
        let other = dir.path().join("build").join("Other.app");
        assert!(fs::create_dir_all(other.join("Contents")).is_ok());
        let bytes = zip(&other);
        let layout = Layout::beside(&running).unwrap_or_else(|error| panic!("{error}"));
        let new = Version::new(0, 2, 0);
        let current = Version::new(0, 1, 0);
        let result = layout.stage(&bytes, expected(&new, &current));
        assert!(matches!(result, Err(Error::NotABundle(_))));
    }
}
