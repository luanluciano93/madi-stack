#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

//! Elevated helper invoked by the main MadiStack binary to apply firewall
//! rules in a single UAC prompt. The embedded manifest requests
//! `requireAdministrator`, so Windows raises UAC on launch.
//!
//! Protocol: one JSON file path is passed as `argv[1]`. The file contains
//! both the rules to install and the path where the helper should write its
//! result, so the main process only has to wait for process exit and then
//! read the output file. Exit code is 0 on success, 1 on failure — the
//! detailed error goes into the output file.
//!
//! This binary is intentionally tiny (no logging, no stdin/stdout): UAC'd
//! helpers that do anything beyond one action are a security smell.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct RuleSpec {
    name: String,
    description: String,
    program: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Request {
    rules: Vec<RuleSpec>,
    output: PathBuf,
}

#[derive(Debug, Serialize)]
struct Response {
    ok: bool,
    error: Option<String>,
}

fn main() -> ExitCode {
    let Some(input_path) = std::env::args_os().nth(1) else {
        eprintln!("usage: madistack-firewall-helper <request.json>");
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

    let rules: Vec<madi_firewall::FirewallRule> = req
        .rules
        .into_iter()
        .map(|r| madi_firewall::FirewallRule::new(r.name, r.description, r.program))
        .collect();

    let resp = match madi_firewall::ensure_inbound_rules(&rules) {
        Ok(()) => Response {
            ok: true,
            error: None,
        },
        Err(e) => Response {
            ok: false,
            error: Some(e.to_string()),
        },
    };

    let output = req.output;
    let body = serde_json::to_vec(&resp).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    if let Err(e) = std::fs::write(&output, &body) {
        eprintln!("failed to write {}: {e}", output.display());
        return ExitCode::from(2);
    }

    if resp.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
