use semver::Version;

use crate::{Error, Result};

const TAG_PREFIX: char = 'v';

pub fn parse(text: &str) -> Result<Version> {
    let trimmed = text.trim();
    let bare = trimmed.strip_prefix(TAG_PREFIX).unwrap_or(trimmed);
    Version::parse(bare).map_err(|_| Error::Version(trimmed.to_owned()))
}

pub fn is_prerelease(version: &Version) -> bool {
    !version.pre.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    #[test]
    fn tags_parse_with_or_without_the_v() {
        assert_eq!(version("v0.2.0"), Version::new(0, 2, 0));
        assert_eq!(version("1.10.3"), Version::new(1, 10, 3));
        assert!(matches!(parse("v1.2"), Err(Error::Version(_))));
        assert!(matches!(parse("latest"), Err(Error::Version(_))));
    }

    #[test]
    fn versions_compare_numerically_and_prereleases_sort_before_their_release() {
        assert!(version("0.10.0") > version("0.9.9"));
        assert!(version("1.0.0") > version("0.99.99"));
        assert!(version("0.2.0") > version("0.2.0-beta.2"));
        assert!(version("0.2.0-beta.10") > version("0.2.0-beta.2"));
        assert!(version("0.2.0-beta.1") > version("0.1.9"));
        assert_eq!(version("v0.2.0"), version("0.2.0"));
    }

    #[test]
    fn prereleases_are_recognised_by_their_suffix() {
        assert!(is_prerelease(&version("0.3.0-rc.1")));
        assert!(!is_prerelease(&version("0.3.0")));
    }
}
