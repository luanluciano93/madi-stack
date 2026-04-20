//! Reads <https://windows.php.net/downloads/releases/releases.json> and
//! returns the latest stable Non-Thread-Safe x64 VS17 release.
//!
//! Schema (abbreviated):
//! ```json
//! {
//!   "8.4": {
//!     "version": "8.4.2",
//!     "tags": ["stable"],
//!     "nts-vs17-x64": {
//!       "zip": { "path": "php-8.4.2-nts-Win32-vs17-x64.zip", "sha256": "..." }
//!     }
//!   },
//!   "8.3": { ... }
//! }
//! ```

use std::{collections::HashMap, str::FromStr};

use madi_core::{Component, ReleaseInfo};
use serde::Deserialize;

use crate::{SourceError, SourceResult};

// windows.php.net redirects here; we hit the canonical URL directly.
const RELEASES_JSON: &str = "https://downloads.php.net/~windows/releases/releases.json";
const DOWNLOAD_BASE: &str = "https://downloads.php.net/~windows/releases/";

#[derive(Debug, Deserialize)]
struct Release {
    version: String,
    #[serde(rename = "nts-vs17-x64")]
    nts_vs17_x64: Option<Build>,
}

#[derive(Debug, Deserialize)]
struct Build {
    zip: Asset,
}

#[derive(Debug, Deserialize)]
struct Asset {
    path: String,
    sha256: Option<String>,
}

pub async fn latest(client: &reqwest::Client) -> SourceResult<ReleaseInfo> {
    let body: HashMap<String, Release> = client
        .get(RELEASES_JSON)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    pick_latest(&body)
}

fn pick_latest(releases: &HashMap<String, Release>) -> SourceResult<ReleaseInfo> {
    // releases.json has every historical major (5.x through 8.x). We pick
    // the one with the highest semver version that still ships an nts-vs17-x64
    // zip. That's implicitly the latest stable — pre-releases go to separate
    // "qa"/"snapshots" endpoints, not this file.
    let (version, asset) = releases
        .values()
        .filter_map(|r| {
            let build = r.nts_vs17_x64.as_ref()?;
            let v = semver::Version::from_str(&r.version).ok()?;
            Some((v, &build.zip))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .ok_or(SourceError::NoRelease("downloads.php.net"))?;

    let filename = asset.path.clone();
    let download_url = if asset.path.starts_with("http") {
        asset.path.clone()
    } else {
        format!("{DOWNLOAD_BASE}{filename}")
    };

    Ok(ReleaseInfo {
        component: Component::Php,
        version,
        download_url,
        sha256: asset.sha256.clone(),
        filename,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_highest_with_nts_build() {
        let json = r#"{
            "8.4": {
                "version": "8.4.2",
                "nts-vs17-x64": { "zip": { "path": "php-8.4.2-nts-Win32-vs17-x64.zip", "sha256": "abc" } }
            },
            "8.3": {
                "version": "8.3.15",
                "nts-vs17-x64": { "zip": { "path": "php-8.3.15-nts-Win32-vs17-x64.zip", "sha256": "def" } }
            }
        }"#;
        let map: HashMap<String, Release> = serde_json::from_str(json).unwrap();
        let info = pick_latest(&map).unwrap();
        assert_eq!(info.version.to_string(), "8.4.2");
        assert_eq!(info.sha256.as_deref(), Some("abc"));
        assert!(info
            .download_url
            .ends_with("php-8.4.2-nts-Win32-vs17-x64.zip"));
    }

    #[test]
    fn ignores_majors_without_nts_vs17_x64_build() {
        let json = r#"{
            "7.4": { "version": "7.4.33",
                     "ts-vc15-x64": { "zip": { "path": "ts.zip" } } },
            "8.3": { "version": "8.3.15",
                     "nts-vs17-x64": { "zip": { "path": "php-8.3.15-nts-Win32-vs17-x64.zip" } } }
        }"#;
        let map: HashMap<String, Release> = serde_json::from_str(json).unwrap();
        let info = pick_latest(&map).unwrap();
        assert_eq!(info.version.to_string(), "8.3.15");
    }
}
