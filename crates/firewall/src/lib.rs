//! Windows Firewall rules management.
//!
//! `unsafe` is explicitly allowed here — `windows-rs` COM calls are
//! inherently unsafe. Keep the surface area tiny and each `unsafe` block must
//! carry a SAFETY comment.

#![allow(unsafe_code)]
#![cfg_attr(not(windows), allow(unused))]

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum FirewallError {
    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(#[from] windows::core::Error),

    #[error("firewall operations are only supported on Windows")]
    Unsupported,
}

pub type FirewallResult<T> = Result<T, FirewallError>;

/// Ensure an inbound allow-rule exists for `program` on 127.0.0.1.
///
/// TODO(sprint-3): implement via INetFwPolicy2 (windows-rs).
pub fn ensure_inbound_rule(_name: &str, _program: &Path) -> FirewallResult<()> {
    #[cfg(not(windows))]
    return Err(FirewallError::Unsupported);
    #[cfg(windows)]
    Ok(())
}

/// Remove a rule previously added by us.
pub fn remove_rule(_name: &str) -> FirewallResult<()> {
    #[cfg(not(windows))]
    return Err(FirewallError::Unsupported);
    #[cfg(windows)]
    Ok(())
}
