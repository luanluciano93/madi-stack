//! Detect the version of an installed component by invoking the binary
//! with `-v`/`--version` and parsing the first SemVer-looking token out
//! of its output. Only called when the installed-versions map has no
//! entry for a component whose signature binary is on disk — normal
//! install/update flows persist the version directly, so this is the
//! backfill path for users whose install predates the persistence code.

use std::path::Path;

use madi_core::Component;
use semver::Version;

/// Best-effort probe. Returns `None` on any failure (missing binary,
/// spawn error, output not parseable). Never logs at error level —
/// this is optional enrichment, not a requirement.
pub async fn probe(install_dir: &Path, component: Component) -> Option<Version> {
    match component {
        Component::Nginx => probe_nginx(install_dir).await,
        Component::Php => probe_php(install_dir).await,
        Component::MariaDb => probe_mariadb(install_dir).await,
        Component::PhpMyAdmin => probe_phpmyadmin(install_dir),
    }
}

async fn probe_nginx(install_dir: &Path) -> Option<Version> {
    let exe = install_dir.join("bin").join("nginx").join("nginx.exe");
    // `nginx -v` writes to stderr; `-V` would also work but adds noise.
    let out = tokio::process::Command::new(&exe)
        .arg("-v")
        .output()
        .await
        .ok()?;
    // Format: `nginx version: nginx/1.29.8\n`
    let text = String::from_utf8_lossy(&out.stderr);
    parse_semver_after(&text, "nginx/")
}

async fn probe_php(install_dir: &Path) -> Option<Version> {
    // Prefer php.exe if bundled, fall back to php-cgi -v (works too).
    let bin = install_dir.join("bin").join("php");
    let exe = [bin.join("php.exe"), bin.join("php-cgi.exe")]
        .into_iter()
        .find(|p| p.is_file())?;
    let out = tokio::process::Command::new(&exe)
        .arg("-v")
        .output()
        .await
        .ok()?;
    // Format: `PHP 8.5.5 (cgi-fcgi) ...`
    let text = String::from_utf8_lossy(&out.stdout);
    parse_semver_after(&text, "PHP ")
}

async fn probe_mariadb(install_dir: &Path) -> Option<Version> {
    let exe = install_dir
        .join("bin")
        .join("mariadb")
        .join("bin")
        .join("mysqld.exe");
    let out = tokio::process::Command::new(&exe)
        .arg("--version")
        .output()
        .await
        .ok()?;
    // Format: `mysqld.exe Ver 12.2.2-MariaDB for Win64 on x86_64 (...)`
    let text = String::from_utf8_lossy(&out.stdout);
    parse_semver_after(&text, "Ver ")
}

fn probe_phpmyadmin(install_dir: &Path) -> Option<Version> {
    // phpMyAdmin ships a `RELEASE-DATE-X.Y.Z` marker file; easiest and
    // most reliable version signal that doesn't require running PHP.
    let pma = install_dir.join("bin").join("phpmyadmin");
    let entries = std::fs::read_dir(&pma).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_str()?;
        if let Some(rest) = name_str.strip_prefix("RELEASE-DATE-") {
            if let Ok(v) = Version::parse(rest) {
                return Some(v);
            }
        }
    }
    None
}

/// Scan `text` for the first `X.Y.Z` token that appears after `marker`.
/// Tolerates any suffix (e.g. `12.2.2-MariaDB`) by truncating at the
/// first non-digit/dot character past the minor.patch triplet.
fn parse_semver_after(text: &str, marker: &str) -> Option<Version> {
    let idx = text.find(marker)?;
    let tail = &text[idx + marker.len()..];

    // Grab contiguous `[0-9.]` run, then trim any prerelease/build suffix
    // before handing off to semver. Avoids pulling `regex` just for this.
    let end = tail
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(tail.len());
    let core = &tail[..end];
    // Quick sanity: need at least major.minor.patch.
    if core.split('.').count() < 3 {
        return None;
    }
    Version::parse(core).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nginx_stderr() {
        let v = parse_semver_after("nginx version: nginx/1.29.8\n", "nginx/").unwrap();
        assert_eq!(v, Version::new(1, 29, 8));
    }

    #[test]
    fn parses_php_stdout() {
        let v = parse_semver_after("PHP 8.5.5 (cgi-fcgi) (built: ...)", "PHP ").unwrap();
        assert_eq!(v, Version::new(8, 5, 5));
    }

    #[test]
    fn parses_mariadb_stdout() {
        let v = parse_semver_after("mysqld.exe  Ver 12.2.2-MariaDB for Win64 on x86_64", "Ver ")
            .unwrap();
        assert_eq!(v, Version::new(12, 2, 2));
    }

    #[test]
    fn rejects_two_part_version() {
        assert!(parse_semver_after("nginx version: nginx/1.2", "nginx/").is_none());
    }

    #[test]
    fn rejects_missing_marker() {
        assert!(parse_semver_after("no marker here 1.2.3", "Ver ").is_none());
    }
}
