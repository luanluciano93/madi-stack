//! `madistack` — a CLI harness for the MadiStack internal crates.
//!
//! During Sprint 1 it lets us exercise each crate end-to-end without the
//! Tauri GUI. Commands grow as the crates mature.
//!
//! Usage:
//!   madistack sources [component]     # show latest release info
//!   madistack fetch <component>       # download + verify + extract

use std::{env, path::PathBuf, time::Instant};

use anyhow::{bail, Context, Result};
use madi_core::Component;
use madi_downloader::{download_verified, extract_zip, Progress};
use madi_sources::{build_client, latest};
use tokio::sync::mpsc;
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
        Some("fetch") => cmd_fetch(&args[1..]).await,
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
    println!("    madistack fetch   <nginx|php|mariadb|phpmyadmin>");
    println!("    madistack help");
    println!();
    println!("`fetch` downloads the latest release into ./tmp/, verifies SHA256 when");
    println!("available, and extracts into ./bin/<component>/.");
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

async fn cmd_fetch(args: &[String]) -> Result<()> {
    let slug = args.first().context("usage: madistack fetch <component>")?;
    let component = parse_component(slug)?;

    let client = build_client();

    println!("Resolving latest {}…", component.display_name());
    let info = latest(&client, component).await?;
    println!("  version   {}", info.version);
    println!("  file      {}", info.filename);
    println!("  url       {}", info.download_url);
    println!(
        "  sha256    {}",
        info.sha256
            .as_deref()
            .unwrap_or("(not published — skip verification)")
    );
    println!();

    let tmp_dir = PathBuf::from("tmp");
    let zip_path = tmp_dir.join(&info.filename);
    let target = PathBuf::from("bin").join(component.slug());

    // Progress channel + dedicated printer task.
    let (tx, rx) = mpsc::channel::<Progress>(64);
    let printer = tokio::spawn(print_progress(rx));

    let started = Instant::now();
    download_verified(
        &client,
        &info.download_url,
        &zip_path,
        info.sha256.as_deref(),
        Some(tx.clone()),
        None,
    )
    .await?;

    let _ = tx.send(Progress::Extracting).await;
    // Clear target dir before extracting so repeat runs are deterministic.
    if target.exists() {
        tokio::fs::remove_dir_all(&target).await?;
    }
    extract_zip(&zip_path, &target).await?;

    drop(tx);
    printer.await?;

    let elapsed = started.elapsed();
    println!();
    println!(
        "Extracted into {} ({:.1}s total).",
        target.display(),
        elapsed.as_secs_f64()
    );
    Ok(())
}

async fn print_progress(mut rx: mpsc::Receiver<Progress>) {
    let mut total: Option<u64> = None;
    let mut last_print = Instant::now();
    while let Some(event) = rx.recv().await {
        match event {
            Progress::Started { total_bytes } => {
                total = total_bytes;
                match total_bytes {
                    Some(n) => println!("Downloading {} ({})…", fmt_bytes(n), n),
                    None => println!("Downloading (size unknown)…"),
                }
            }
            Progress::Downloaded { bytes } => {
                if last_print.elapsed().as_millis() > 200 {
                    render_line(bytes, total);
                    last_print = Instant::now();
                }
            }
            Progress::Verifying => {
                // Final line for the download phase.
                if let Some(t) = total {
                    render_line(t, total);
                }
                println!("\nVerifying SHA256…");
            }
            Progress::Extracting => {
                println!("Extracting archive…");
            }
            Progress::Done => {
                println!("Download + verification OK.");
            }
        }
    }
}

fn render_line(bytes: u64, total: Option<u64>) {
    use std::io::Write;
    match total {
        Some(t) if t > 0 => {
            // cast precision loss is irrelevant: result is printed to 1 decimal
            #[allow(clippy::cast_precision_loss)]
            let pct = (bytes as f64 / t as f64 * 100.0).min(100.0);
            print!("\r  {} / {}  ({pct:>5.1}%)", fmt_bytes(bytes), fmt_bytes(t));
        }
        _ => {
            print!("\r  {}", fmt_bytes(bytes));
        }
    }
    let _ = std::io::stdout().flush();
}

#[allow(clippy::cast_precision_loss)] // display formatting — rounds to 1 decimal
fn fmt_bytes(n: u64) -> String {
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1_024.0;
    let n = n as f64;
    if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.1} KB", n / KB)
    } else {
        format!("{n} B")
    }
}

fn parse_component(slug: &str) -> Result<Component> {
    Component::all()
        .iter()
        .copied()
        .find(|c| c.slug().eq_ignore_ascii_case(slug))
        .with_context(|| format!("unknown component: {slug}"))
}
