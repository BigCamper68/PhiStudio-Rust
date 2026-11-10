//! Bounded, anonymous reads of the repository's latest stable GitHub release.
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

pub(super) const API: &str =
    "https://api.github.com/repos/BigCamper68/PhiStudio-Rust/releases/latest";
pub(super) const RELEASES: &str = "https://github.com/BigCamper68/PhiStudio-Rust/releases";
const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Release {
    pub version: Version,
    pub url: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CheckError {
    Connection,
    Timeout,
    RateLimited,
    NoRelease,
    InvalidResponse,
    Server,
    Worker,
}
impl CheckError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Connection => {
                "Could not connect to GitHub. Check your internet connection and try again."
            }
            Self::Timeout => "Update check timed out. Try again later.",
            Self::RateLimited => "GitHub's request limit was reached. Try again later.",
            Self::NoRelease => "No stable release has been published yet.",
            Self::InvalidResponse => {
                "The release information could not be read. Open the releases page to check manually."
            }
            Self::Server => "GitHub is temporarily unavailable. Try again later.",
            Self::Worker => "Update check interrupted. Try again.",
        }
    }
}
fn network_error(error: reqwest::Error) -> CheckError {
    if error.is_timeout() {
        CheckError::Timeout
    } else {
        CheckError::Connection
    }
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}
fn parse(bytes: &[u8]) -> Result<Release, CheckError> {
    let raw: GithubRelease =
        serde_json::from_slice(bytes).map_err(|_| CheckError::InvalidResponse)?;
    if raw.draft || raw.prerelease {
        return Err(CheckError::NoRelease);
    }
    let tag = raw.tag_name.trim();
    if tag.len() > 128 {
        return Err(CheckError::InvalidResponse);
    }
    let version = Version::parse(tag.strip_prefix(['v', 'V']).unwrap_or(tag))
        .map_err(|_| CheckError::InvalidResponse)?;
    if !version.pre.is_empty() {
        return Err(CheckError::NoRelease);
    }
    let url = reqwest::Url::parse(&raw.html_url).map_err(|_| CheckError::InvalidResponse)?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !url
            .path()
            .starts_with("/BigCamper68/PhiStudio-Rust/releases/tag/")
    {
        return Err(CheckError::InvalidResponse);
    }
    Ok(Release {
        version,
        url: url.into(),
    })
}
pub(super) fn installed_version() -> &'static str {
    option_env!("PHISTUDIO_ANDROID_BUILD")
        .and_then(|v| v.split_whitespace().next())
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}
pub(super) fn newer(release: &Release, installed: &str) -> Result<bool, CheckError> {
    let installed = Version::parse(installed).map_err(|_| CheckError::InvalidResponse)?;
    Ok(release.version.cmp_precedence(&installed).is_gt())
}
pub(super) fn check() -> Result<Release, CheckError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CheckError::Worker)?;
    runtime.block_on(fetch(API))
}
async fn fetch(endpoint: &str) -> Result<Release, CheckError> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("PhiStudio-Rust/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| CheckError::Connection)?;
    let mut response = client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(network_error)?;
    match response.status().as_u16() {
        200 => {}
        404 => return Err(CheckError::NoRelease),
        403 | 429 => return Err(CheckError::RateLimited),
        _ => return Err(CheckError::Server),
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_BYTES as u64)
    {
        return Err(CheckError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(CheckError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    parse(&bytes)
}

#[cfg(test)]
mod tests;
