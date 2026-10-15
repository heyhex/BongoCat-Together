//! Anonymous version notifications for Together; never downloads or installs payloads.

use std::{io::Read, time::Duration};

use cargo_packager_updater::{reqwest, semver::Version};
use serde::Deserialize;

use crate::{
    HOST_TARGET_TRIPLE, RELEASE_REPOSITORY_NAME, RELEASE_REPOSITORY_OWNER,
    UpdateDiagnosticsTracker, UpdateError, UpdateErrorCode, UpdateOutcome, UpdateRelease,
    UpdateTargetTriple,
};

const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 3;
const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Read-only checker, shared by Development and Production builds.
pub struct TogetherReleaseChecker {
    current_version: &'static str,
    diagnostics: UpdateDiagnosticsTracker,
}

impl TogetherReleaseChecker {
    pub fn new(current_version: &'static str, diagnostics: UpdateDiagnosticsTracker) -> Self {
        Self {
            current_version,
            diagnostics,
        }
    }

    pub fn release_page_url(version: &str) -> Option<String> {
        let version = Version::parse(version).ok()?;
        Some(format!(
            "{}/releases/tag/together-v{version}",
            repository_url()
        ))
    }

    pub fn check(&self) -> Result<UpdateOutcome, UpdateError> {
        self.diagnostics.record_check_started();
        let result = self.fetch();
        match &result {
            Ok(_) => self.diagnostics.record_check_succeeded(),
            Err(error) => self.diagnostics.record_check_failed(error.code().as_str()),
        }
        result
    }

    fn fetch(&self) -> Result<UpdateOutcome, UpdateError> {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("BongoCat-Together/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| failure(UpdateErrorCode::ReleaseFetchFailed))?;
        let mut releases = Vec::new();
        for page in 1..=MAX_PAGES {
            let response = client
                .get(format!(
                    "https://api.github.com/repos/{RELEASE_REPOSITORY_OWNER}/{RELEASE_REPOSITORY_NAME}/releases?per_page={PAGE_SIZE}&page={page}"
                ))
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
                .map_err(|_| failure(UpdateErrorCode::ReleaseFetchFailed))?;
            let entries = read_page(response)?;
            let complete = entries.len() < PAGE_SIZE;
            releases.extend(entries);
            if complete {
                return select_release(&releases, self.current_version, HOST_TARGET_TRIPLE);
            }
        }
        // Incomplete history cannot prove that the running version is current.
        Err(failure(UpdateErrorCode::ReleaseManifestInvalid))
    }
}

fn repository_url() -> String {
    format!("https://github.com/{RELEASE_REPOSITORY_OWNER}/{RELEASE_REPOSITORY_NAME}")
}

fn failure(code: UpdateErrorCode) -> UpdateError {
    UpdateError::at(crate::UpdateStage::Check, code)
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    html_url: String,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn read_page(reader: impl Read) -> Result<Vec<GithubRelease>, UpdateError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure(UpdateErrorCode::ReleaseFetchFailed))?;
    if bytes.len() as u64 > MAX_BODY_BYTES {
        return Err(failure(UpdateErrorCode::ReleaseManifestInvalid));
    }
    let entries: Vec<GithubRelease> = serde_json::from_slice(&bytes)
        .map_err(|_| failure(UpdateErrorCode::ReleaseManifestInvalid))?;
    if entries.len() > PAGE_SIZE {
        return Err(failure(UpdateErrorCode::ReleaseManifestInvalid));
    }
    Ok(entries)
}

fn select_release(
    releases: &[GithubRelease],
    current_version: &str,
    target: UpdateTargetTriple,
) -> Result<UpdateOutcome, UpdateError> {
    let current =
        Version::parse(current_version).map_err(|_| failure(UpdateErrorCode::NotConfigured))?;
    let (version, release) = releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let text = release.tag_name.strip_prefix("together-v")?;
            let version = Version::parse(text).ok()?;
            (version.to_string() == text).then_some((version, release))
        })
        .max_by(|(a, _), (b, _)| a.cmp_precedence(b))
        .ok_or_else(|| failure(UpdateErrorCode::ReleaseFetchFailed))?;
    if release.html_url
        != TogetherReleaseChecker::release_page_url(&version.to_string()).unwrap_or_default()
    {
        return Err(failure(UpdateErrorCode::ReleaseManifestInvalid));
    }
    let asset_name = match target {
        UpdateTargetTriple::X86_64PcWindowsMsvc => format!("BongoCat-Together_{version}_x64.exe"),
        UpdateTargetTriple::Aarch64AppleDarwin | UpdateTargetTriple::X86_64AppleDarwin => {
            return Err(failure(UpdateErrorCode::NoMatchingAsset));
        }
    };
    let asset_url = format!(
        "{}/releases/download/{}/{asset_name}",
        repository_url(),
        release.tag_name
    );
    if !release
        .assets
        .iter()
        .any(|asset| asset.name == asset_name && asset.browser_download_url == asset_url)
    {
        return Err(failure(UpdateErrorCode::NoMatchingAsset));
    }
    if !version.cmp_precedence(&current).is_gt() {
        return Ok(UpdateOutcome::UpToDate);
    }
    Ok(UpdateOutcome::Available {
        release: UpdateRelease {
            version: version.to_string(),
            notes: release.body.clone(),
        },
    })
}

#[cfg(test)]
mod tests;
