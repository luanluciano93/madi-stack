//! Windows Firewall rules management.
//!
//! Adds inbound-allow rules via the `INetFwPolicy2` COM interface
//! (`windows-rs`). All rules requested in a single call are added inside one
//! COM session — so when UAC elevation is required, the user only sees one
//! prompt, not three.
//!
//! `unsafe` is explicitly allowed here because COM calls through `windows-rs`
//! are inherently unsafe. Each `unsafe` block carries a SAFETY comment.

#![allow(unsafe_code)]
#![cfg_attr(not(windows), allow(unused))]

pub mod elevated;

pub use elevated::{
    run_elevated_ensure, run_elevated_hosts_edit, run_elevated_mkcert_install, ElevatedError,
    ElevatedResult, HostEntry,
};

use std::path::{Path, PathBuf};

#[cfg(windows)]
use windows::core::BSTR;
#[cfg(windows)]
use windows::Win32::Foundation::VARIANT_TRUE;
#[cfg(windows)]
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, INetFwRules, NetFwPolicy2, NetFwRule, NET_FW_ACTION_ALLOW,
    NET_FW_IP_PROTOCOL_TCP, NET_FW_PROFILE2_DOMAIN, NET_FW_PROFILE2_PRIVATE,
    NET_FW_PROFILE2_PUBLIC, NET_FW_RULE_DIR_IN,
};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};

/// `RPC_E_CHANGED_MODE` as a raw i32. Means the current thread was already
/// initialized with a different COM apartment — not an error for us, we just
/// piggy-back on whatever apartment is there.
#[cfg(windows)]
const RPC_E_CHANGED_MODE: i32 = -2_147_417_850; // 0x80010106

#[derive(Debug, thiserror::Error)]
pub enum FirewallError {
    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(#[from] windows::core::Error),

    #[error("firewall operations are only supported on Windows")]
    Unsupported,
}

pub type FirewallResult<T> = Result<T, FirewallError>;

/// A single inbound rule to ensure.
#[derive(Debug, Clone)]
pub struct FirewallRule {
    /// Rule name as it will appear in `wf.msc`. Also the idempotency key —
    /// re-running with the same name is a no-op.
    pub name: String,
    pub description: String,
    /// Full path to the `.exe` the rule grants inbound access to.
    pub program: PathBuf,
}

impl FirewallRule {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        program: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            program: program.into(),
        }
    }
}

/// Ensure each inbound-allow rule exists.
///
/// Rules already present (matched by name) are skipped — the call is
/// idempotent. All additions happen inside a single COM session, so UAC (if
/// the process isn't already elevated) prompts at most once.
pub fn ensure_inbound_rules(rules: &[FirewallRule]) -> FirewallResult<()> {
    #[cfg(not(windows))]
    {
        let _ = rules;
        Err(FirewallError::Unsupported)
    }
    #[cfg(windows)]
    {
        let _com = ComGuard::new()?;
        let (_policy, list) = open_rules()?;

        for r in rules {
            let name = BSTR::from(r.name.as_str());

            // SAFETY: `list` is a valid COM pointer from `open_rules`. `Item`
            // returns an error when the rule is absent, which we treat as
            // "needs insert".
            if unsafe { list.Item(&name) }.is_ok() {
                tracing::debug!(name = %r.name, "firewall rule already present");
                continue;
            }

            // SAFETY: `NetFwRule` is a documented CLSID; `CoCreateInstance`
            // returns a fresh `INetFwRule` pointer.
            let rule: INetFwRule =
                unsafe { CoCreateInstance(&NetFwRule, None, CLSCTX_INPROC_SERVER)? };

            let program = r.program.to_string_lossy();
            let desc = BSTR::from(r.description.as_str());
            let app = BSTR::from(program.as_ref());

            // SAFETY: all inputs are valid BSTRs / documented i32 constants
            // from the Windows Firewall SDK.
            unsafe {
                rule.SetName(&name)?;
                rule.SetDescription(&desc)?;
                rule.SetApplicationName(&app)?;
                rule.SetProtocol(NET_FW_IP_PROTOCOL_TCP.0)?;
                rule.SetDirection(NET_FW_RULE_DIR_IN)?;
                rule.SetAction(NET_FW_ACTION_ALLOW)?;
                rule.SetEnabled(VARIANT_TRUE)?;
                rule.SetProfiles(
                    NET_FW_PROFILE2_DOMAIN.0 | NET_FW_PROFILE2_PRIVATE.0 | NET_FW_PROFILE2_PUBLIC.0,
                )?;
                list.Add(&rule)?;
            }
            tracing::info!(name = %r.name, program = %program, "firewall rule added");
        }
        Ok(())
    }
}

/// Convenience wrapper: ensure a single inbound rule.
pub fn ensure_inbound_rule(name: &str, program: &Path) -> FirewallResult<()> {
    ensure_inbound_rules(&[FirewallRule::new(
        name.to_string(),
        format!("MadiStack inbound rule for {name}"),
        program.to_path_buf(),
    )])
}

/// Ensure the three MadiStack-managed services have inbound rules.
///
/// Resolves the standard layout under `install_dir` (`bin/<component>/...`)
/// and batches nginx + php-cgi + mysqld in a single COM session, so the user
/// sees at most one UAC prompt.
pub fn ensure_madistack_rules(install_dir: &Path) -> FirewallResult<()> {
    let bin = install_dir.join("bin");
    let rules = [
        FirewallRule::new(
            "MadiStack — Nginx",
            "Allow inbound HTTP traffic to the MadiStack Nginx server.",
            bin.join("nginx").join("nginx.exe"),
        ),
        FirewallRule::new(
            "MadiStack — MariaDB",
            "Allow inbound connections to the MadiStack MariaDB server.",
            bin.join("mariadb").join("bin").join("mysqld.exe"),
        ),
        FirewallRule::new(
            "MadiStack — PHP FastCGI",
            "Allow inbound FastCGI traffic to the MadiStack PHP runtime.",
            bin.join("php").join("php-cgi.exe"),
        ),
    ];
    ensure_inbound_rules(&rules)
}

/// Build the default MadiStack rule set without applying it. Used by the
/// elevated helper path so the main process can hand a pre-built rule list
/// to `run_elevated_ensure`.
#[must_use]
pub fn madistack_rules(install_dir: &Path) -> Vec<FirewallRule> {
    let bin = install_dir.join("bin");
    vec![
        FirewallRule::new(
            "MadiStack — Nginx",
            "Allow inbound HTTP traffic to the MadiStack Nginx server.",
            bin.join("nginx").join("nginx.exe"),
        ),
        FirewallRule::new(
            "MadiStack — MariaDB",
            "Allow inbound connections to the MadiStack MariaDB server.",
            bin.join("mariadb").join("bin").join("mysqld.exe"),
        ),
        FirewallRule::new(
            "MadiStack — PHP FastCGI",
            "Allow inbound FastCGI traffic to the MadiStack PHP runtime.",
            bin.join("php").join("php-cgi.exe"),
        ),
    ]
}

/// Remove a rule previously added by us. Absent rules are silently ignored.
pub fn remove_rule(name: &str) -> FirewallResult<()> {
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(FirewallError::Unsupported)
    }
    #[cfg(windows)]
    {
        let _com = ComGuard::new()?;
        let (_policy, list) = open_rules()?;
        let bname = BSTR::from(name);
        // SAFETY: `list` is valid; `Remove` fails when the rule is absent,
        // which we deliberately swallow to keep the call idempotent.
        let _ = unsafe { list.Remove(&bname) };
        Ok(())
    }
}

/// Check whether a rule with the given name exists.
pub fn rule_exists(name: &str) -> FirewallResult<bool> {
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(FirewallError::Unsupported)
    }
    #[cfg(windows)]
    {
        let _com = ComGuard::new()?;
        let (_policy, list) = open_rules()?;
        // SAFETY: `list` is valid; `Item` is the supported probe.
        Ok(unsafe { list.Item(&BSTR::from(name)) }.is_ok())
    }
}

#[cfg(windows)]
fn open_rules() -> FirewallResult<(INetFwPolicy2, INetFwRules)> {
    // SAFETY: `NetFwPolicy2` is a documented CLSID; `INetFwPolicy2` is its
    // default interface.
    let policy: INetFwPolicy2 =
        unsafe { CoCreateInstance(&NetFwPolicy2, None, CLSCTX_INPROC_SERVER)? };
    // SAFETY: `policy` was just created; `Rules()` is a documented accessor.
    let list = unsafe { policy.Rules()? };
    Ok((policy, list))
}

/// Scoped COM apartment initialization. Tolerates `RPC_E_CHANGED_MODE` so it
/// can be called from threads already in a different apartment (e.g. tokio
/// workers that have been touched by another crate).
#[cfg(windows)]
struct ComGuard {
    /// Whether we actually initialized COM on this thread. If we piggy-backed
    /// on an existing apartment, we must NOT call `CoUninitialize`.
    initialized: bool,
}

#[cfg(windows)]
impl ComGuard {
    fn new() -> FirewallResult<Self> {
        // SAFETY: `CoInitializeEx` is safe to call; we track whether we
        // actually initialized so Drop pairs correctly.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr.0 < 0 && hr.0 != RPC_E_CHANGED_MODE {
            return Err(windows::core::Error::from_hresult(hr).into());
        }
        Ok(Self {
            initialized: hr.0 != RPC_E_CHANGED_MODE,
        })
    }
}

#[cfg(windows)]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: paired with the successful `CoInitializeEx` above.
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firewall_rule_builder_keeps_fields() {
        let r = FirewallRule::new("n", "d", PathBuf::from("/x"));
        assert_eq!(r.name, "n");
        assert_eq!(r.description, "d");
        assert_eq!(r.program, PathBuf::from("/x"));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_returns_unsupported() {
        let err = ensure_inbound_rule("x", Path::new("/x")).unwrap_err();
        assert!(matches!(err, FirewallError::Unsupported));
    }
}
