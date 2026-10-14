use semver::Version;
use serde::Deserialize;

use crate::signature::{CHECKSUM_EXTENSION, SIGNATURE_EXTENSION};
use crate::version::{self, is_prerelease};
use crate::{Error, Result};

const ASSET_PREFIX: &str = "Flock";
const ARCH: &str = "arm64";
const ZIP_EXTENSION: &str = "zip";

#[derive(Debug, Deserialize)]
struct RawRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub page: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifacts {
    pub zip: Asset,
    pub checksum: Asset,
    pub signature: Asset,
}

impl RawRelease {
    fn release(self) -> Option<Release> {
        let version = version::parse(&self.tag_name).ok()?;
        Some(Release {
            version,
            page: self.html_url,
            draft: self.draft,
            prerelease: self.prerelease,
            assets: self.assets,
        })
    }
}

pub fn parse_latest(json: &str) -> Result<Vec<Release>> {
    let raw: RawRelease = serde_json::from_str(json).map_err(|error| Error::json(&error))?;
    Ok(raw.release().into_iter().collect())
}

pub fn parse_list(json: &str) -> Result<Vec<Release>> {
    let raw: Vec<RawRelease> = serde_json::from_str(json).map_err(|error| Error::json(&error))?;
    Ok(raw.into_iter().filter_map(RawRelease::release).collect())
}

pub fn newest(releases: Vec<Release>, current: &Version, prereleases: bool) -> Option<Release> {
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter(|release| prereleases || !(release.prerelease || is_prerelease(&release.version)))
        .filter(|release| release.version > *current)
        .max_by(|a, b| a.version.cmp(&b.version))
}

impl Release {
    pub fn zip_name(&self) -> String {
        format!("{ASSET_PREFIX}-{}-{ARCH}.{ZIP_EXTENSION}", self.version)
    }

    fn asset(&self, name: &str) -> Result<Asset> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .cloned()
            .ok_or_else(|| Error::MissingAsset {
                version: self.version.to_string(),
                name: name.to_owned(),
            })
    }

    pub fn artifacts(&self) -> Result<Artifacts> {
        let zip = self.zip_name();
        Ok(Artifacts {
            zip: self.asset(&zip)?,
            checksum: self.asset(&format!("{zip}.{CHECKSUM_EXTENSION}"))?,
            signature: self.asset(&format!("{zip}.{SIGNATURE_EXTENSION}"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATEST: &str = include_str!("fixtures/latest.json");
    const LIST: &str = include_str!("fixtures/releases.json");

    fn current() -> Version {
        Version::new(0, 1, 0)
    }

    #[test]
    fn the_latest_release_parses_with_its_three_artifacts() {
        let releases = parse_latest(LATEST).unwrap_or_default();
        let [release] = releases.as_slice() else {
            panic!("expected one release, got {releases:?}");
        };
        assert_eq!(release.version, Version::new(0, 2, 0));
        assert_eq!(
            release.page,
            "https://github.com/team-loco/fleet-app/releases/tag/v0.2.0"
        );
        let artifacts = release
            .artifacts()
            .unwrap_or_else(|error| panic!("artifacts: {error}"));
        assert_eq!(artifacts.zip.name, "Flock-0.2.0-arm64.zip");
        assert_eq!(artifacts.zip.size, 41_234_567);
        assert!(
            artifacts
                .checksum
                .url
                .ends_with("/Flock-0.2.0-arm64.zip.sha256")
        );
        assert!(
            artifacts
                .signature
                .url
                .ends_with("/Flock-0.2.0-arm64.zip.sig")
        );
    }

    #[test]
    fn a_release_without_a_signature_has_no_artifacts() {
        let releases = parse_list(LIST).unwrap_or_default();
        let stable = releases
            .iter()
            .find(|release| release.version == Version::new(0, 2, 0));
        let missing = stable.map(Release::artifacts);
        assert!(matches!(missing, Some(Err(Error::MissingAsset { .. }))));
    }

    #[test]
    fn the_list_skips_tags_that_are_not_versions() {
        let releases = parse_list(LIST).unwrap_or_default();
        let tags: Vec<String> = releases.iter().map(|r| r.version.to_string()).collect();
        assert_eq!(tags, ["0.4.0", "0.3.0-beta.1", "0.2.0", "0.1.0"]);
    }

    #[test]
    fn drafts_are_never_offered_and_prereleases_only_on_request() {
        let releases = parse_list(LIST).unwrap_or_default();
        let stable = newest(releases.clone(), &current(), false);
        assert_eq!(stable.map(|r| r.version), Some(Version::new(0, 2, 0)));
        let beta = newest(releases, &current(), true);
        assert_eq!(
            beta.map(|r| r.version.to_string()),
            Some("0.3.0-beta.1".into())
        );
    }

    #[test]
    fn nothing_newer_than_the_running_version_means_up_to_date() {
        let releases = parse_latest(LATEST).unwrap_or_default();
        assert_eq!(
            newest(releases.clone(), &Version::new(0, 2, 0), false),
            None
        );
        assert_eq!(newest(releases, &Version::new(0, 3, 0), true), None);
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(matches!(
            parse_latest("{\"tag_name\": 3}"),
            Err(Error::Json(_))
        ));
        assert!(matches!(parse_list("<html>"), Err(Error::Json(_))));
    }
}
