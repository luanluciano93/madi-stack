//! `madistack` — a CLI harness for the MadiStack internal crates.
//!
//! During Sprint 1 it lets us exercise each crate end-to-end without the
//! Tauri GUI. Commands grow as the crates mature.
//!
//! Usage:
//!   madistack sources           # show latest release info for every component
//!   madistack sources nginx     # just one component

use std::env;

use anyhow::{bail, Context, Result};
use madi_core::Component;
use madi_sources::{build_client, latest};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("MADISTACK_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("sources") => cmd_sources(&args[1..]).await,
        Some("help" | "-h" | "--help") | None => {
            print_help();
            Ok(())
        }
        Some(other) => {
            print_help();
            bail!("unknown command: {other}");
        }
    }
}

fn print_help() {
    println!("madistack — MadiStack developer CLI");
    println!();
    println!("USAGE:");
    println!("    madistack sources [nginx|php|mariadb|phpmyadmin]");
    println!("    madistack help");
}

async fn cmd_sources(args: &[String]) -> Result<()> {
    let client = build_client();

    let components: Vec<Component> = match args.first().map(String::as_str) {
        None => Component::all().to_vec(),
        Some(slug) => vec![parse_component(slug)?],
    };

    println!("{:<12} {:<10} {:<5} FILE", "COMPONENT", "VERSION", "SHA?");
    println!("{:-<12} {:-<10} {:-<5} {:-<60}", "", "", "", "");

    for c in components {
        match latest(&client, c).await {
            Ok(r) => {
                println!(
                    "{:<12} {:<10} {:<5} {}",
                    r.component.display_name(),
                    r.version.to_string(),
                    if r.sha256.is_some() { "yes" } else { "no" },
                    r.filename,
                );
                tracing::debug!(url = %r.download_url, "resolved");
            }
            Err(e) => {
                println!("{:<12} ERROR: {e}", c.display_name());
            }
        }
    }

    Ok(())
}

fn parse_component(slug: &str) -> Result<Component> {
    Component::all()
        .iter()
        .copied()
        .find(|c| c.slug().eq_ignore_ascii_case(slug))
        .with_context(|| format!("unknown component: {slug}"))
}
