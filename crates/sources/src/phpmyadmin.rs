//! Reads <https://www.phpmyadmin.net/home_page/version.json> and returns the
//! latest stable release.
//!
//! Schema:
//! ```json
//! { "version": "5.2.1", "date": "...", "releases": [...] }
//! ```
//!
//! The download URL follows a predictable pattern:
//! `https://files.phpmyadmin.net/phpMyAdmin/{version}/phpMyAdmin-{version}-all-languages.zip`
//!
//! version.json itself does not include a SHA256 for the zip, but the project
//! publishes `.sha256` sidecar files next to each release archive.

use std::str::FromStr;

use madi_core::{Component, ReleaseInfo};
use serde::Deserialize;

use crate::{SourceError, SourceResult};

const VERSION_JSON: &str = "https://www.phpmyadmin.net/home_page/version.json";
const DOWNLOAD_BASE: &str = "https://files.phpmyadmin.net/phpMyAdmin";

#[derive(Debug, Deserialize)]
struct VersionDoc {
    version: String,
}

pub async fn latest(client: &reqwest::Client) -> SourceResult<ReleaseInfo> {
    let doc: VersionDoc = client
        .get(VERSION_JSON)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    build_release(&doc.version, client).await
}

async fn build_release(version_str: &str, client: &reqwest::Client) -> SourceResult<ReleaseInfo> {
    let version = semver::Version::from_str(version_str)
        .map_err(|_| SourceError::Version(version_str.to_string()))?;
    let filename = format!("phpMyAdmin-{version_str}-all-languages.zip");
    let download_url = format!("{DOWNLOAD_BASE}/{version_str}/{filename}");
    let sha256_url = format!("{download_url}.sha256");

    // Best-effort SHA256: fetch the sidecar file. If it fails, proceed without.
    let sha256 = client.get(&sha256_url).send().await.ok().and_then(|r| {
        if r.status().is_success() {
            Some(r)
        } else {
            None
        }
    });

    let sha256 = if let Some(resp) = sha256 {
        let body = resp.text().await.ok();
        body.and_then(parse_sha256_line)
    } else {
        None
    };

    Ok(ReleaseInfo {
        component: Component::PhpMyAdmin,
        version,
        download_url,
        sha256,
        filename,
    })
}

/// Parse a `sha256sum`-style line: `<hex>  <filename>`.
fn parse_sha256_line(body: String) -> Option<String> {
    body.split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|s| s.len() == 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sha256_line() {
        let line = format!("{}  phpMyAdmin-5.2.1-all-languages.zip", "a".repeat(64));
        assert_eq!(parse_sha256_line(line), Some("a".repeat(64)));
    }

    #[test]
    fn rejects_too_short_hash() {
        assert_eq!(parse_sha256_line("deadbeef file.zip".into()), None);
    }
}
