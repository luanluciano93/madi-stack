//! Uses the MariaDB REST API to find the latest stable Windows x64 zip.
//!
//! The top-level endpoint at <https://downloads.mariadb.org/rest-api/mariadb/>
//! returns metadata for every major version. For each version we pick
//! `release_status == "Stable"` and then drill into the file list asking for
//! the `winx64` ZIP archive.
//!
//! To avoid loading the entire catalog we first list major_releases, sort them
//! descending, and fetch files for the newest stable major until we find one
//! with a winx64 ZIP.

use std::str::FromStr;

use madi_core::{Component, ReleaseInfo};
use serde::Deserialize;

use crate::{SourceError, SourceResult};

const LIST_URL: &str = "https://downloads.mariadb.org/rest-api/mariadb/";

#[derive(Debug, Deserialize)]
struct Listing {
    major_releases: Vec<MajorRelease>,
}

#[derive(Debug, Deserialize)]
struct MajorRelease {
    release_id: String,
    release_status: String,
}

#[derive(Debug, Deserialize)]
struct MajorDetail {
    releases: std::collections::HashMap<String, Release>,
}

#[derive(Debug, Deserialize)]
struct Release {
    release_id: String,
    files: Vec<FileEntry>,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    file_name: String,
    file_download_url: String,
    // null for non-file entries like "yum/" or "repo/"
    package_type: Option<String>,
    #[serde(default)]
    checksum: Checksum,
    // null for source entries and directory-like rows
    os: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Checksum {
    // all four hash fields can be null for directory-like entries (yum/, repo/)
    sha256sum: Option<String>,
}

pub async fn latest(client: &reqwest::Client) -> SourceResult<ReleaseInfo> {
    let listing: Listing = client
        .get(LIST_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut stable_majors: Vec<&str> = listing
        .major_releases
        .iter()
        .filter(|m| m.release_status.eq_ignore_ascii_case("Stable"))
        .map(|m| m.release_id.as_str())
        .collect();
    // release_ids look like "11.6", "11.4", "10.11" — string sort is wrong
    // for 2-digit majors, use natural ordering on parsed parts
    stable_majors.sort_by(|a, b| natural_version_cmp(b, a));

    for major in stable_majors {
        let url = format!("{LIST_URL}{major}/");
        let detail: MajorDetail = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(info) = pick_winx64_zip(&detail) {
            return Ok(info);
        }
    }

    Err(SourceError::NoRelease("downloads.mariadb.org"))
}

fn pick_winx64_zip(detail: &MajorDetail) -> Option<ReleaseInfo> {
    // The detail endpoint already only lists releases under a stable major,
    // so we don't need to re-filter by status (which isn't present anyway).
    let best = detail
        .releases
        .values()
        .filter_map(|r| {
            let version = semver::Version::from_str(&r.release_id).ok()?;
            let file = r.files.iter().find(|f| {
                let pkg = f.package_type.as_deref().unwrap_or("").to_ascii_lowercase();
                let os = f.os.as_deref().unwrap_or("");
                pkg.contains("zip")
                    && (f.file_name.contains("winx64") || os.eq_ignore_ascii_case("Windows"))
                    && f.file_name.to_ascii_lowercase().ends_with(".zip")
            })?;
            Some((version, file))
        })
        .max_by(|a, b| a.0.cmp(&b.0));

    best.map(|(version, file)| ReleaseInfo {
        component: Component::MariaDb,
        version,
        download_url: file.file_download_url.clone(),
        sha256: file.checksum.sha256sum.clone(),
        filename: file.file_name.clone(),
    })
}

fn natural_version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a).cmp(&parse(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn picks_winx64_zip_from_files() {
        let detail = MajorDetail {
            releases: HashMap::from([(
                "11.6.2".into(),
                Release {
                    release_id: "11.6.2".into(),
                    files: vec![
                        FileEntry {
                            file_name: "mariadb-11.6.2-linux-x86_64.tar.gz".into(),
                            file_download_url: "https://example/linux.tgz".into(),
                            package_type: Some("gzipped tar file".into()),
                            os: Some("Linux".into()),
                            checksum: Checksum::default(),
                        },
                        FileEntry {
                            file_name: "mariadb-11.6.2-winx64.zip".into(),
                            file_download_url: "https://example/winx64.zip".into(),
                            package_type: Some("ZIP file".into()),
                            os: Some("Windows".into()),
                            checksum: Checksum {
                                sha256sum: Some("deadbeef".into()),
                            },
                        },
                    ],
                },
            )]),
        };

        let info = pick_winx64_zip(&detail).expect("should find one");
        assert_eq!(info.version.to_string(), "11.6.2");
        assert_eq!(info.filename, "mariadb-11.6.2-winx64.zip");
        assert_eq!(info.sha256.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn natural_cmp_beats_lexicographic() {
        assert!(natural_version_cmp("11.6", "10.11").is_gt());
        assert!(natural_version_cmp("11.4", "11.10").is_lt());
    }
}
