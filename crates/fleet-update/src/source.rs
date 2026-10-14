use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig};

use crate::release::{self, Release};
use crate::{Error, Result};

const GITHUB: &str = "https://github.com/";
const API: &str = "https://api.github.com/repos/";
const LATEST: &str = "/releases/latest";
const LIST: &str = "/releases?per_page=30";
const ACCEPT: &str = "application/vnd.github+json";
const API_LIMIT: u64 = 1 << 20;
const NOT_FOUND: u16 = 404;

pub trait Source: Send + 'static {
    fn releases(&self, prereleases: bool) -> Result<Vec<Release>>;
    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>>;
}

pub struct GitHub {
    agent: ureq::Agent,
    repo: String,
}

pub fn repo_api(repository: &str) -> Option<String> {
    let slug = repository.strip_prefix(GITHUB)?.trim_end_matches('/');
    let slug = slug.strip_suffix(".git").unwrap_or(slug);
    let valid = slug.split('/').count() == 2 && slug.split('/').all(|part| !part.is_empty());
    valid.then(|| format!("{API}{slug}"))
}

impl GitHub {
    pub fn new(repository: &str, timeout: Duration, user_agent: &str) -> Result<Self> {
        let repo = repo_api(repository).ok_or_else(|| Error::Repository(repository.to_owned()))?;
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(timeout))
            .user_agent(user_agent)
            .build()
            .new_agent();
        Ok(Self { agent, repo })
    }

    fn get(&self, url: &str, limit: u64, accept: Option<&str>) -> Result<Vec<u8>> {
        let request = self.agent.get(url);
        let request = match accept {
            Some(accept) => request.header("Accept", accept),
            None => request,
        };
        let mut response = request.call().map_err(network)?;
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(network)
    }
}

fn network(error: ureq::Error) -> Error {
    match error {
        ureq::Error::StatusCode(code) => Error::Http(code),
        ureq::Error::BodyExceedsLimit(limit) => Error::TooLarge(limit),
        other => Error::Network(other.to_string()),
    }
}

impl Source for GitHub {
    fn releases(&self, prereleases: bool) -> Result<Vec<Release>> {
        let path = if prereleases { LIST } else { LATEST };
        let url = format!("{}{path}", self.repo);
        let body = match self.get(&url, API_LIMIT, Some(ACCEPT)) {
            Ok(body) => body,
            Err(Error::Http(NOT_FOUND)) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let text = String::from_utf8(body).map_err(|error| Error::Json(error.to_string()))?;
        if prereleases {
            release::parse_list(&text)
        } else {
            release::parse_latest(&text)
        }
    }

    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>> {
        self.get(url, limit, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_url_comes_from_the_repository_url() {
        assert_eq!(
            repo_api("https://github.com/team-loco/fleet-app").as_deref(),
            Some("https://api.github.com/repos/team-loco/fleet-app")
        );
        assert_eq!(
            repo_api("https://github.com/team-loco/fleet-app.git/").as_deref(),
            Some("https://api.github.com/repos/team-loco/fleet-app")
        );
        assert_eq!(repo_api("https://gitlab.com/team-loco/fleet-app"), None);
        assert_eq!(repo_api("https://github.com/team-loco"), None);
    }
}
