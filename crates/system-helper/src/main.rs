#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

//! Elevated helper invoked by the main MadiStack binary to perform system-
//! level changes (firewall rules, `hosts` file edits) in a single UAC
//! prompt. The embedded manifest requests `requireAdministrator`, so Windows
//! raises UAC on launch.
//!
//! Protocol: one JSON file path is passed as `argv[1]`. The file is a
//! `Request` (see below) describing the op and where to drop the response.
//! Exit code is 0 on success, 1 on operation failure, 2 on argv/IO errors.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct RuleSpec {
    name: String,
    description: String,
    program: PathBuf,
}

/// A line we own in the hosts file. We identify ourselves with a trailing
/// `# madistack:<tag>` marker so we can update/remove only our own entries
/// without stomping on user-authored lines.
#[derive(Debug, Deserialize)]
struct HostEntry {
    /// IP to resolve to — usually `127.0.0.1`.
    ip: String,
    /// Hostname to add (e.g. `foo.test`).
    hostname: String,
    /// Ownership marker written as `# madistack:<tag>`.
    tag: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Operation {
    FirewallEnsure {
        rules: Vec<RuleSpec>,
    },
    HostsEdit {
        /// Entries to add or update (idempotent — keyed by `tag`).
        add: Vec<HostEntry>,
        /// Tags whose lines should be removed.
        remove_tags: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
struct Request {
    op: Operation,
    output: PathBuf,
}

#[derive(Debug, Serialize)]
struct Response {
    ok: bool,
    error: Option<String>,
}

fn main() -> ExitCode {
    let Some(input_path) = std::env::args_os().nth(1) else {
        eprintln!("usage: madistack-system-helper <request.json>");
        return ExitCode::from(2);
    };

    let raw = match std::fs::read(&input_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to read {}: {e}", Path::new(&input_path).display());
            return ExitCode::from(2);
        }
    };

    let req: Request = match serde_json::from_slice(&raw) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("malformed request: {e}");
            return ExitCode::from(2);
        }
    };

    let outcome = match req.op {
        Operation::FirewallEnsure { rules } => apply_firewall(&rules),
        Operation::HostsEdit { add, remove_tags } => apply_hosts(&add, &remove_tags),
    };

    let resp = match outcome {
        Ok(()) => Response {
            ok: true,
            error: None,
        },
        Err(e) => Response {
            ok: false,
            error: Some(e),
        },
    };

    let body = serde_json::to_vec(&resp).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    if let Err(e) = std::fs::write(&req.output, &body) {
        eprintln!("failed to write {}: {e}", req.output.display());
        return ExitCode::from(2);
    }

    if resp.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn apply_firewall(rules: &[RuleSpec]) -> Result<(), String> {
    let fw_rules: Vec<madi_firewall::FirewallRule> = rules
        .iter()
        .map(|r| {
            madi_firewall::FirewallRule::new(
                r.name.clone(),
                r.description.clone(),
                r.program.clone(),
            )
        })
        .collect();
    madi_firewall::ensure_inbound_rules(&fw_rules).map_err(|e| e.to_string())
}

/// The helper runs elevated and only ever edits the system hosts file.
#[cfg(windows)]
const HOSTS_PATH: &str = "C:\\Windows\\System32\\drivers\\etc\\hosts";
#[cfg(not(windows))]
const HOSTS_PATH: &str = "/etc/hosts";

fn apply_hosts(add: &[HostEntry], remove_tags: &[String]) -> Result<(), String> {
    let original = std::fs::read_to_string(HOSTS_PATH)
        .map_err(|e| format!("read hosts: {e}"))?;

    // Drop any line we previously owned whose tag is in `remove_tags` OR that
    // matches an `add` tag — the add branch re-inserts a single canonical
    // line below, so removing first makes the operation idempotent.
    let add_tags: std::collections::HashSet<&str> =
        add.iter().map(|e| e.tag.as_str()).collect();
    let remove_set: std::collections::HashSet<&str> =
        remove_tags.iter().map(String::as_str).collect();

    let mut out = String::with_capacity(original.len() + 256);
    for line in original.lines() {
        if let Some(tag) = extract_madistack_tag(line) {
            if remove_set.contains(tag) || add_tags.contains(tag) {
                continue;
            }
        }
        out.push_str(line);
        out.push_str("\r\n");
    }

    for entry in add {
        // Validate minimally — no whitespace that would split the line mid-
        // field. Trust the main app otherwise (it already validated input).
        if entry.hostname.contains(char::is_whitespace)
            || entry.tag.contains(char::is_whitespace)
            || entry.ip.contains(char::is_whitespace)
        {
            return Err(format!("invalid host entry: {}", entry.hostname));
        }
        let _ = writeln!(
            out,
            "{}\t{}\t# madistack:{}\r",
            entry.ip, entry.hostname, entry.tag
        );
    }

    // Windows can't rename over an existing file, so write via temp + copy.
    let tmp_path = format!("{HOSTS_PATH}.madistack.tmp");
    std::fs::write(&tmp_path, out).map_err(|e| format!("write tmp: {e}"))?;
    std::fs::copy(&tmp_path, HOSTS_PATH).map_err(|e| format!("replace hosts: {e}"))?;
    let _ = std::fs::remove_file(&tmp_path);

    // Flush the DNS cache so new entries resolve immediately. Best-effort —
    // if `ipconfig` isn't on PATH, the user will just wait for the normal
    // TTL to expire.
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("ipconfig")
            .arg("/flushdns")
            .output();
    }

    Ok(())
}

/// If `line` is a MadiStack-owned hosts entry, return the tag. Tags never
/// contain whitespace, so we scan to the first whitespace after the marker.
fn extract_madistack_tag(line: &str) -> Option<&str> {
    let idx = line.find("# madistack:")?;
    let rest = &line[idx + "# madistack:".len()..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(&rest[..end])
}
