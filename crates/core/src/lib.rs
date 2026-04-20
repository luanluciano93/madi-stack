#![forbid(unsafe_code)]
//! Shared types and traits for MadiStack.
//!
//! This crate has no I/O — it only defines the data model used across the
//! other internal crates and the Tauri binary.

use std::fmt;
use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

/// A component in the MadiStack — one of the external binaries we manage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Component {
    Nginx,
    Php,
    MariaDb,
    PhpMyAdmin,
}

impl Component {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Nginx => "nginx",
            Self::Php => "php",
            Self::MariaDb => "mariadb",
            Self::PhpMyAdmin => "phpmyadmin",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Nginx => "Nginx",
            Self::Php => "PHP",
            Self::MariaDb => "MariaDB",
            Self::PhpMyAdmin => "phpMyAdmin",
        }
    }

    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Nginx, Self::Php, Self::MariaDb, Self::PhpMyAdmin]
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Runtime status of a managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

/// Describes a released version of a component, including where to download it
/// and how to verify its integrity.
///
/// `sha256` is optional because not every upstream publishes checksums in a
/// parseable form (notably nginx.org). When absent, the downloader should
/// log a warning but still proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub component: Component,
    pub version: semver::Version,
    pub download_url: String,
    pub sha256: Option<String>,
    pub filename: String,
}

/// User-configurable ports for the stack.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PortConfig {
    pub http: u16,
    pub mariadb: u16,
    pub php_fcgi: u16,
    /// Address the public services (HTTP, MariaDB) bind to.
    ///
    /// Defaults to `127.0.0.1` (local only). Set to `0.0.0.0` to expose the
    /// stack on the LAN — equivalent to USBWebServer's `local`/`slocal`
    /// toggles. The FastCGI listener stays on `127.0.0.1` regardless: nginx
    /// and `php-cgi` always colocate, and exposing FastCGI is never right.
    #[serde(default = "default_bind_address")]
    pub bind_address: Ipv4Addr,
}

fn default_bind_address() -> Ipv4Addr {
    Ipv4Addr::LOCALHOST
}

impl Default for PortConfig {
    fn default() -> Self {
        Self {
            http: 80,
            mariadb: 3306,
            php_fcgi: 9000,
            bind_address: default_bind_address(),
        }
    }
}
