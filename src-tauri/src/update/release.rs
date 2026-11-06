//! This app's releases on GitHub, and which of them to offer.

use super::verify::{ExpectedFile, MAX_DOWNLOAD};
use crate::Error;
use reqwest::blocking::{Client, Response};
use semver::Version;
use serde::Deserialize;
use std::{io::Read, time::Duration};

const RELEASE_API: &str = "https://api.github.com/repos/Coldobird/RbxTools/releases/latest";
const RELEASE_DOWNLOADS: &str = "https://github.com/Coldobird/RbxTools/releases/download/";
const ASSET_NAME: &str = "RBX-Tools.exe";
const MAX_RELEASE_JSON: u64 = 1024 * 1024;

/// A release as GitHub's API describes it.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    state: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

/// A newer, verified-to-be-complete release download.
#[derive(Debug)]
pub struct Download {
    pub version: String,
    pub url: String,
    pub expected: ExpectedFile,
}

impl Release {
    /// The download this release offers over `current`, if it is a newer stable
    /// version with an uploaded executable and a SHA-256 digest from GitHub.
    fn newer_than(self, current: &str) -> Result<Option<Download>, Error> {
        if self.draft || self.prerelease {
            return Ok(None);
        }
        let version = self
            .tag_name
            .strip_prefix('v')
            .and_then(|value| Version::parse(value).ok())
            .filter(|version| version.pre.is_empty() && version.build.is_empty())
            .ok_or("GitHub's release tag is not a stable application version.")?;
        let current = Version::parse(current).map_err(|_| "The application version is invalid.")?;
        if version <= current {
            return Ok(None);
        }
        let asset = self
            .assets
            .into_iter()
            .find(|asset| asset.name == ASSET_NAME && asset.state == "uploaded")
            .ok_or("The latest release's Windows download is not ready yet. Try again shortly.")?;
        if asset.size == 0 || asset.size > MAX_DOWNLOAD {
            return Err("The GitHub update has an invalid download size.".into());
        }
        if asset.browser_download_url
            != format!("{RELEASE_DOWNLOADS}{}/{ASSET_NAME}", self.tag_name)
        {
            return Err(
                "The update download is not from this application's GitHub release.".into(),
            );
        }
        let sha256 = asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .ok_or("GitHub has not supplied a SHA-256 checksum for this update.")?;
        let expected = ExpectedFile::new(asset.size, sha256)
            .ok_or("GitHub has not supplied a SHA-256 checksum for this update.")?;
        Ok(Some(Download {
            version: version.to_string(),
            url: asset.browser_download_url,
            expected,
        }))
    }
}

/// This app's releases on GitHub.
pub struct GitHubReleases {
    client: Client,
}

impl GitHubReleases {
    pub fn new() -> Result<Self, Error> {
        let client = Client::builder()
            .user_agent(concat!("RBX-Tools/", env!("CARGO_PKG_VERSION")))
            .https_only(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|e| format!("Could not connect to GitHub: {e}"))?;
        Ok(Self { client })
    }

    /// The latest release, if it is newer than `current`.
    pub fn newer_than(&self, current: &str) -> Result<Option<Download>, Error> {
        let response = self
            .client
            .get(RELEASE_API)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .timeout(Duration::from_secs(20))
            .send()
            .map_err(|e| format!("Could not check GitHub for updates: {e}"))?;
        if matches!(response.status().as_u16(), 403 | 429) {
            return Err("GitHub is temporarily limiting update checks. Try again later.".into());
        }
        let response = response
            .error_for_status()
            .map_err(|e| format!("Could not check GitHub for updates: {e}"))?;
        let mut bytes = Vec::new();
        response
            .take(MAX_RELEASE_JSON + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Could not read the GitHub release: {e}"))?;
        if bytes.len() as u64 > MAX_RELEASE_JSON {
            return Err("The GitHub release response is too large.".into());
        }
        let release: Release = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Could not read the GitHub release: {e}"))?;
        release.newer_than(current)
    }

    /// Starts downloading a release asset.
    pub fn open(&self, download: &Download) -> Result<Response, Error> {
        self.client
            .get(&download.url)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|e| format!("Could not download the GitHub update: {e}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            assets: vec![Asset {
                name: ASSET_NAME.into(),
                state: "uploaded".into(),
                size: 128,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
                browser_download_url: format!("{RELEASE_DOWNLOADS}v{version}/{ASSET_NAME}"),
            }],
        }
    }

    #[test]
    fn only_newer_stable_versions_are_offered() {
        assert_eq!(
            release("0.2.10")
                .newer_than("0.2.9")
                .unwrap()
                .unwrap()
                .version,
            "0.2.10"
        );
        assert!(release("0.2.14").newer_than("0.2.14").unwrap().is_none());
        assert!(release("0.2.13").newer_than("0.2.14").unwrap().is_none());
        let mut draft = release("0.3.0");
        draft.draft = true;
        assert!(draft.newer_than("0.2.14").unwrap().is_none());
        let mut preview = release("0.3.0-beta.1");
        preview.prerelease = true;
        assert!(preview.newer_than("0.2.14").unwrap().is_none());
        for tag in ["latest", "0.3.0", "v0.3", "v0.3.0-beta.1", "v0.3.0+build"] {
            let mut invalid = release("0.3.0");
            invalid.tag_name = tag.into();
            assert!(invalid.newer_than("0.2.14").is_err(), "{tag}");
        }
    }

    #[test]
    fn missing_or_unverified_assets_are_not_offered() {
        let mut missing = release("0.3.0");
        missing.assets.clear();
        assert!(missing
            .newer_than("0.2.14")
            .unwrap_err()
            .message()
            .contains("not ready"));
        for url in [
            "http://github.com/Coldobird/RbxTools/releases/download/v0.3.0/RBX-Tools.exe",
            "https://github.com/another/repo/releases/download/v0.3.0/RBX-Tools.exe",
            "https://github.com.evil.example/Coldobird/RbxTools/releases/download/v0.3.0/RBX-Tools.exe",
            "https://github.com/Coldobird/RbxTools/releases/download/v0.2.14/RBX-Tools.exe",
        ] {
            let mut invalid = release("0.3.0");
            invalid.assets[0].browser_download_url = url.into();
            assert!(invalid.newer_than("0.2.14").is_err());
        }
        for digest in [
            None,
            Some("sha256:wrong".into()),
            Some(format!("sha1:{}", "a".repeat(64))),
        ] {
            let mut invalid = release("0.3.0");
            invalid.assets[0].digest = digest;
            assert!(invalid.newer_than("0.2.14").is_err());
        }
        let mut oversized = release("0.3.0");
        oversized.assets[0].size = MAX_DOWNLOAD + 1;
        assert!(oversized.newer_than("0.2.14").is_err());
    }

    #[test]
    #[ignore = "Reads and downloads the actual public GitHub release; never installs or launches it"]
    fn live_github_download_verifies_without_installing() {
        let releases = GitHubReleases::new().unwrap();
        let download = releases.newer_than("0.0.0").unwrap().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("download.exe");
        let mut file = std::fs::File::create(&path).unwrap();
        download
            .expected
            .copy(releases.open(&download).unwrap(), &mut file)
            .unwrap();
        drop(file);
        crate::update::verify::check_windows_x64(&path).unwrap();
        println!(
            "Verified GitHub v{}: {:?}",
            download.version, download.expected
        );
    }
}
