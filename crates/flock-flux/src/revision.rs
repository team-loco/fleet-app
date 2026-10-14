const SHORT_LEN: usize = 7;
const SHA1_MARKER: &str = "@sha1:";
const DIGEST_SEPARATOR: char = ':';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitRevision<'a> {
    pub reference: &'a str,
    pub sha: &'a str,
}

pub fn git(revision: &str) -> Option<GitRevision<'_>> {
    let (reference, sha) = revision.split_once(SHA1_MARKER)?;
    let valid = !sha.is_empty() && sha.chars().all(|c| c.is_ascii_hexdigit());
    valid.then_some(GitRevision { reference, sha })
}

pub fn short(revision: &str) -> String {
    let tail = match git(revision) {
        Some(parsed) => parsed.sha,
        None => revision
            .rsplit_once(DIGEST_SEPARATOR)
            .map_or(revision, |(_, digest)| digest),
    };
    let hashed = tail.len() > SHORT_LEN && tail.chars().all(|c| c.is_ascii_hexdigit());
    if hashed {
        tail.chars().take(SHORT_LEN).collect()
    } else {
        tail.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_revisions_split_reference_and_sha() {
        let parsed = git("master@sha1:3e0ff8ae123b710bc91de1315cba0f996a8896c2");
        assert_eq!(
            parsed,
            Some(GitRevision {
                reference: "master",
                sha: "3e0ff8ae123b710bc91de1315cba0f996a8896c2",
            })
        );
        assert_eq!(
            git("refs/tags/v1@sha1:abc123").map(|r| r.reference),
            Some("refs/tags/v1")
        );
        assert_eq!(git("6.15.0"), None);
        assert_eq!(git("main@sha1:"), None);
        assert_eq!(git("main@sha1:zz"), None);
    }

    #[test]
    fn short_revisions_trim_hashes_but_keep_versions() {
        assert_eq!(short("master@sha1:3e0ff8ae123b710bc91de"), "3e0ff8a");
        assert_eq!(short("sha256:e7dc68a4dec90a35c2c6"), "e7dc68a");
        assert_eq!(short("6.15.0"), "6.15.0");
        assert_eq!(short("v1.2@sha256:abcdef0123456789"), "abcdef0");
        assert_eq!(short(""), "");
    }
}
