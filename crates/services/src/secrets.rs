//! Local-only secrets store: `madistack-secrets.toml`.
//!
//! Currently holds the auto-generated MariaDB root password. The file lives
//! next to `madistack.toml` in the install dir and is gitignored.
//!
//! Design notes:
//! - The password is generated **once**, on the first MariaDB bootstrap, and
//!   reused for every subsequent start. Regenerating it would require also
//!   running `ALTER USER` against an already-initialized server, which we'd
//!   rather not do silently.
//! - On Windows the file inherits NTFS ACLs from the install folder. We do
//!   NOT yet tighten the ACL programmatically — TODO before v1.0 (use
//!   `windows-rs` `SetNamedSecurityInfoW` or the `windows-acl` crate).
//! - The format is plain TOML. If you need to rotate the password manually,
//!   delete `data/mariadb/` to trigger a fresh bootstrap.

use std::path::{Path, PathBuf};

use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    De(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    Ser(#[from] toml::ser::Error),
}

pub type SecretsResult<T> = Result<T, SecretsError>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Secrets {
    /// Root password generated when MariaDB's data dir was first bootstrapped.
    /// Empty string means "no password set" (legacy / pre-secrets installs).
    #[serde(default)]
    pub mariadb_root_password: String,

    /// 32-char random used as phpMyAdmin's `blowfish_secret` for cookie auth
    /// encryption. Rotated only if the user deletes `madistack-secrets.toml` —
    /// regenerating invalidates anyone's active pma login.
    #[serde(default)]
    pub pma_blowfish_secret: String,
}

#[must_use]
pub fn secrets_path(install_dir: &Path) -> PathBuf {
    install_dir.join("madistack-secrets.toml")
}

pub fn load(install_dir: &Path) -> SecretsResult<Option<Secrets>> {
    let path = secrets_path(install_dir);
    match std::fs::read_to_string(&path) {
        Ok(raw) => Ok(Some(toml::from_str(&raw)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn save(install_dir: &Path, s: &Secrets) -> SecretsResult<()> {
    let path = secrets_path(install_dir);
    let raw = toml::to_string_pretty(s)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, raw)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// 24-char alphanumeric password (~143 bits of entropy). Long enough that we
/// don't need to worry about brute force, short enough to paste comfortably.
#[must_use]
pub fn generate_password() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

/// 32-char alphanumeric secret for phpMyAdmin cookie encryption. 32 chars is
/// the minimum pma complains about on the config check page.
#[must_use]
pub fn generate_blowfish_secret() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let s = Secrets {
            mariadb_root_password: "abcDEF123".into(),
            pma_blowfish_secret: "secret32charsXXXXXXXXXXXXXXXXXXX".into(),
        };
        save(dir.path(), &s).unwrap();
        let loaded = load(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.mariadb_root_password, "abcDEF123");
    }

    #[test]
    fn load_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn generate_password_is_24_alnum() {
        let pw = generate_password();
        assert_eq!(pw.len(), 24);
        assert!(pw.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
