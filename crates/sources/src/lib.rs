#![forbid(unsafe_code)]
//! Release-info clients for the 4 managed components.
//!
//! Each submodule knows how to talk to its upstream source and return a
//! [`ReleaseInfo`] for the latest stable Windows x64 build.
//!
//! The entry point is [`latest`] — dispatches to the right submodule based
//! on the [`Component`] asked for.

use std::time::Duration;

use madi_core::{Component, ReleaseInfo};

pub mod mariadb;
pub mod nginx;
pub mod php;
pub mod phpmyadmin;

/// User-Agent we send upstream. Some CDN endpoints reject the default reqwest
/// UA.
pub const USER_AGENT: &str = concat!(
    "MadiStack/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/luanluciano93/madi-stack)"
);

/// Errors returned by a source client.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("no matching release found on {0}")]
    NoRelease(&'static str),

    #[error("invalid version string {0:?}")]
    Version(String),
}

pub type SourceResult<T> = Result<T, SourceError>;

/// Build an HTTP client tuned for our use: short connect timeout, longer read
/// timeout (upstreams can be slow), rustls, compression on.
pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("reqwest client build should not fail with static config")
}

/// Convenience: resolve the latest release for any component.
pub async fn latest(client: &reqwest::Client, component: Component) -> SourceResult<ReleaseInfo> {
    match component {
        Component::Nginx => nginx::latest(client).await,
        Component::Php => php::latest(client).await,
        Component::MariaDb => mariadb::latest(client).await,
        Component::PhpMyAdmin => phpmyadmin::latest(client).await,
    }
}
