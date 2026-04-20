//! Scrapes <https://nginx.org/en/download.html> for the latest mainline build.
//!
//! The page has a stable structure with three sections in this order:
//! "Mainline version", "Stable version", "Legacy versions". Each section
//! contains a table with a link like `nginx-1.27.5.zip`. We prefer mainline
//! — that's the recommended version for modern deployments per nginx.org.
//!
//! nginx.org does not publish a parseable SHA256 checksum alongside the zip,
//! only a PGP signature (.asc). We accept that and return `None` — the
//! downloader will still verify the HTTPS connection.

use std::str::FromStr;

use madi_core::{Component, ReleaseInfo};
use scraper::{Html, Selector};

use crate::{SourceError, SourceResult};

const DOWNLOAD_PAGE: &str = "https://nginx.org/en/download.html";
const DOWNLOAD_BASE: &str = "https://nginx.org/download/";

pub async fn latest(client: &reqwest::Client) -> SourceResult<ReleaseInfo> {
    let body = client
        .get(DOWNLOAD_PAGE)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_latest_mainline(&body)
}

fn parse_latest_mainline(html: &str) -> SourceResult<ReleaseInfo> {
    let doc = Html::parse_document(html);

    // The download page has <h4> section headers followed by tables of
    // releases. The first <h4> is "Mainline version".
    let a_sel = Selector::parse("a").expect("static selector");

    // Walk every <a> under the document; the first one matching the mainline
    // pattern is the newest mainline zip (the page lists newest-first).
    let zip_link = doc
        .select(&a_sel)
        .filter_map(|a| a.value().attr("href"))
        .find(|href| {
            let name = href.rsplit('/').next().unwrap_or("");
            name.starts_with("nginx-")
                && std::path::Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        })
        .ok_or(SourceError::NoRelease("nginx.org"))?;

    // Normalize to the absolute zip URL.
    let filename = zip_link.rsplit('/').next().unwrap_or(zip_link).to_string();
    let download_url = if zip_link.starts_with("http") {
        zip_link.to_string()
    } else {
        format!("{DOWNLOAD_BASE}{filename}")
    };

    // Extract version from filename: "nginx-1.27.5.zip" -> "1.27.5"
    let version_str = filename
        .trim_start_matches("nginx-")
        .trim_end_matches(".zip");
    let version = semver::Version::from_str(version_str)
        .map_err(|_| SourceError::Version(version_str.to_string()))?;

    Ok(ReleaseInfo {
        component: Component::Nginx,
        version,
        download_url,
        sha256: None,
        filename,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        <html><body>
        <h4>Mainline version</h4>
        <table>
          <tr>
            <td>nginx-1.27.5</td>
            <td><a href="/download/nginx-1.27.5.zip">zip</a></td>
            <td><a href="/download/nginx-1.27.5.zip.asc">pgp</a></td>
          </tr>
        </table>
        <h4>Stable version</h4>
        <table>
          <tr>
            <td>nginx-1.26.3</td>
            <td><a href="/download/nginx-1.26.3.zip">zip</a></td>
          </tr>
        </table>
        </body></html>
    "#;

    #[test]
    fn picks_first_mainline_zip() {
        let info = parse_latest_mainline(SAMPLE).expect("should parse");
        assert_eq!(info.component, Component::Nginx);
        assert_eq!(info.version.to_string(), "1.27.5");
        assert_eq!(info.filename, "nginx-1.27.5.zip");
        assert_eq!(
            info.download_url,
            "https://nginx.org/download/nginx-1.27.5.zip"
        );
        assert!(info.sha256.is_none());
    }

    #[test]
    fn errors_when_no_zip_found() {
        let html = "<html><body>nothing here</body></html>";
        assert!(parse_latest_mainline(html).is_err());
    }
}
