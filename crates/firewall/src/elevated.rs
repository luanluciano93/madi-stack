//! Launch the bundled firewall helper elevated, triggering one UAC prompt.
//!
//! Flow:
//! 1. Main process writes a JSON request (rules + path where to drop the
//!    response) to a temp file.
//! 2. `ShellExecuteExW` with verb `runas` launches
//!    `madistack-firewall-helper.exe <request.json>` — Windows shows UAC,
//!    the helper runs as admin.
//! 3. We block on the returned process handle and then read the response
//!    file to decide success/failure.
//!
//! Why a helper instead of elevating the whole app: MadiStack is a long-
//! running GUI that doesn't need admin for 99% of what it does. Asking
//! users to run the whole thing as admin is a security regression and
//! breaks drag-and-drop from regular Explorer.
//!
//! Cancellation: if the user dismisses the UAC prompt, `ShellExecuteExW`
//! fails with `ERROR_CANCELLED` (1223) — we surface that as
//! [`ElevatedError::UserCancelled`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::FirewallRule;

#[derive(Debug, thiserror::Error)]
pub enum ElevatedError {
    #[error("helper binary not found at {0}")]
    HelperMissing(PathBuf),

    #[error("user dismissed the UAC prompt")]
    UserCancelled,

    #[error("helper exited with code {0}")]
    HelperFailed(i32),

    #[error("helper reported error: {0}")]
    HelperError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(#[from] windows::core::Error),
}

pub type ElevatedResult<T> = Result<T, ElevatedError>;

#[derive(Debug, Serialize)]
struct RuleSpec<'a> {
    name: &'a str,
    description: &'a str,
    program: &'a Path,
}

#[derive(Debug, Serialize)]
struct Request<'a> {
    rules: Vec<RuleSpec<'a>>,
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Response {
    ok: bool,
    error: Option<String>,
}

/// Launch `helper_exe` elevated with the given rules. Blocks until the helper
/// exits. The helper must be the compiled `madistack-firewall-helper` with
/// the `requireAdministrator` manifest — otherwise Windows won't elevate.
pub fn run_elevated_ensure(helper_exe: &Path, rules: &[FirewallRule]) -> ElevatedResult<()> {
    if !helper_exe.is_file() {
        return Err(ElevatedError::HelperMissing(helper_exe.to_path_buf()));
    }

    // Stage request/response next to each other so debugging is easy if the
    // helper misbehaves — both are auto-cleaned on success.
    let tmp = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let req_path = tmp.join(format!("madistack-fw-req-{stamp}.json"));
    let resp_path = tmp.join(format!("madistack-fw-resp-{stamp}.json"));

    let req = Request {
        rules: rules
            .iter()
            .map(|r| RuleSpec {
                name: &r.name,
                description: &r.description,
                program: &r.program,
            })
            .collect(),
        output: resp_path.clone(),
    };
    std::fs::write(&req_path, serde_json::to_vec(&req)?)?;

    let result = imp::shell_execute_runas_and_wait(helper_exe, &req_path);

    // Best-effort cleanup regardless of outcome.
    let _ = std::fs::remove_file(&req_path);

    let exit_code = result?;
    if exit_code != 0 {
        // Try to surface the helper's own error message if available.
        if let Ok(bytes) = std::fs::read(&resp_path) {
            let _ = std::fs::remove_file(&resp_path);
            if let Ok(resp) = serde_json::from_slice::<Response>(&bytes) {
                if !resp.ok {
                    return Err(match resp.error {
                        Some(msg) => ElevatedError::HelperError(msg),
                        None => ElevatedError::HelperFailed(exit_code),
                    });
                }
            }
        }
        return Err(ElevatedError::HelperFailed(exit_code));
    }

    let bytes = std::fs::read(&resp_path)?;
    let _ = std::fs::remove_file(&resp_path);
    let resp: Response = serde_json::from_slice(&bytes)?;
    if resp.ok {
        Ok(())
    } else {
        Err(ElevatedError::HelperError(
            resp.error.unwrap_or_else(|| "unknown".into()),
        ))
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    use super::{ElevatedError, ElevatedResult};

    fn to_wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Returns the process exit code.
    pub fn shell_execute_runas_and_wait(
        helper_exe: &Path,
        request_json: &Path,
    ) -> ElevatedResult<i32> {
        let verb = to_wide(OsStr::new("runas"));
        let file = to_wide(helper_exe.as_os_str());
        // Quote the arg — temp paths can contain spaces.
        let params_str = format!("\"{}\"", request_json.display());
        let params = to_wide(OsStr::new(&params_str));

        let mut info = SHELLEXECUTEINFOW {
            cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            // SW_HIDE = 0 — the helper has no UI, so don't flash a console.
            nShow: 0,
            ..Default::default()
        };

        // SAFETY: `info` is correctly sized and the PCWSTRs point into the
        // wide buffers we keep alive until after the call.
        let result = unsafe { ShellExecuteExW(&mut info) };
        if let Err(err) = result {
            // `ERROR_CANCELLED` (1223) = user clicked "No" on the UAC prompt.
            // HRESULT_FROM_WIN32 wraps it as 0x800704C7.
            // Reinterpret the i32 HRESULT as its unsigned bit pattern for
            // the canonical 0x8007_04C7 comparison. Both casts are lossless
            // on the underlying bits.
            #[allow(clippy::cast_sign_loss)]
            let hresult: u32 = err.code().0 as u32;
            if hresult == 0x8007_04C7 || (hresult & 0xFFFF) == ERROR_CANCELLED.0 {
                return Err(ElevatedError::UserCancelled);
            }
            return Err(err.into());
        }

        let process: HANDLE = info.hProcess;
        if process.is_invalid() {
            // SEE_MASK_NOCLOSEPROCESS should have given us one; if it didn't,
            // treat as a generic failure.
            return Err(ElevatedError::HelperFailed(-1));
        }

        // SAFETY: `process` is a valid handle we own until `CloseHandle`.
        let _ = unsafe { WaitForSingleObject(process, INFINITE) };

        let mut code: u32 = 0;
        // SAFETY: `process` is valid and `code` is a stack u32.
        let got = unsafe { GetExitCodeProcess(process, &mut code) };

        // SAFETY: always close the handle we opened via NOCLOSEPROCESS.
        let _ = unsafe { CloseHandle(process) };

        got?;
        // Exit codes above 0x7FFF_FFFF are unusual but legal; preserve the
        // bit pattern rather than clamping.
        #[allow(clippy::cast_possible_wrap)]
        let signed = code as i32;
        Ok(signed)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    use super::{ElevatedError, ElevatedResult};

    pub fn shell_execute_runas_and_wait(
        _helper_exe: &Path,
        _request_json: &Path,
    ) -> ElevatedResult<i32> {
        Err(ElevatedError::HelperFailed(-1))
    }
}
