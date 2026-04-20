//! `madistack` — a CLI harness for the MadiStack internal crates.
//!
//! During Sprint 1 it lets us exercise each crate end-to-end without the
//! Tauri GUI. Commands grow as the crates mature.
//!
//! Usage:
//!   madistack sources [component]     # show latest release info
//!   madistack fetch   <component>     # download + verify + extract
//!   madistack init                    # render nginx/php/mariadb configs
//!   madistack start   <component>     # spawn a managed service
//!   madistack stop    <component>     # stop a managed service
//!   madistack status  [component]     # print current service status
//!   madistack logs    <component>     # snapshot the in-memory log buffer

use std::{env, path::PathBuf, time::Instant};

use anyhow::{bail, Context, Result};
use madi_config_gen::{render_all, RenderContext, DEFAULT_PHP_EXTENSIONS};
use madi_core::Component;
use madi_downloader::{download_verified, extract_zip, Progress};
use madi_services::Supervisor;
use madi_sources::{build_client, latest};
use madi_state_store::{load_or_default, AppState as StoredState};
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
        Some("init") => cmd_init().await,
        Some("start") => cmd_start(&args[1..]).await,
        Some("stop") => cmd_stop(&args[1..]).await,
        Some("status") => cmd_status(&args[1..]),
        Some("logs") => cmd_logs(&args[1..]),
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
    println!("    madistack init");
    println!("    madistack start   <nginx|php|mariadb>");
    println!("    madistack stop    <nginx|php|mariadb>");
    println!("    madistack status  [nginx|php|mariadb]");
    println!("    madistack logs    <nginx|php|mariadb>");
    println!("    madistack help");
    println!();
    println!("`fetch` downloads the latest release into ./tmp/, verifies SHA256 when");
    println!("available, and extracts into ./bin/<component>/.");
    println!("`init`  renders nginx.conf + php.ini + my.ini into ./config/ using");
    println!("        the ports from ./madistack.toml (falls back to defaults).");
}

/// Resolve the install root. The CLI assumes the current working directory
/// is the MadiStack install folder — this matches how the portable .exe runs.
fn install_dir() -> Result<PathBuf> {
    env::current_dir().context("cannot read current directory")
}

fn load_stored() -> Result<StoredState> {
    let path = install_dir()?.join("madistack.toml");
    load_or_default(&path).with_context(|| format!("reading {}", path.display()))
}

fn supervisor_with_state(state: &StoredState) -> Result<Supervisor> {
    Ok(Supervisor::new(install_dir()?, state.ports))
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

async fn cmd_init() -> Result<()> {
    let install = install_dir()?;
    let stored = load_stored().unwrap_or_default();
    let ports = stored.ports;

    let doc_root = install.join("www");
    tokio::fs::create_dir_all(&doc_root).await.ok();
    // nginx needs its log dirs to exist before spawn — it won't mkdir them.
    tokio::fs::create_dir_all(install.join("logs").join("nginx"))
        .await
        .ok();

    // Seed a welcome page so the first hit to `/` shows something real.
    let index = doc_root.join("index.html");
    if !index.exists() {
        tokio::fs::write(
            &index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>MadiStack</title>\
             </head><body style=\"font-family:sans-serif;margin:3em auto;max-width:40em\">\
             <h1>MadiStack</h1><p>Se você está vendo isso, o nginx subiu.</p>\
             <p>Coloque seus projetos em <code>www/</code>.</p></body></html>",
        )
        .await
        .ok();
    }

    let config_dir = install.join("config");
    let ctx = RenderContext {
        install_dir: &install,
        ports,
        document_root: &doc_root,
        php_extensions: DEFAULT_PHP_EXTENSIONS,
    };
    render_all(&ctx, &config_dir).context("rendering configs")?;

    println!("Rendered configs into {}", config_dir.display());
    println!("  ports http={} mariadb={} php_fcgi={} bind={}", ports.http, ports.mariadb, ports.php_fcgi, ports.bind_address);
    Ok(())
}

fn require_component_arg(args: &[String], usage: &str) -> Result<Component> {
    let slug = args.first().with_context(|| usage.to_string())?;
    parse_component(slug)
}

async fn cmd_start(args: &[String]) -> Result<()> {
    let c = require_component_arg(args, "usage: madistack start <component>")?;
    let stored = load_stored().unwrap_or_default();
    let sup = supervisor_with_state(&stored)?;
    let handle = sup.start(c).await.context("starting service")?;
    println!("{} started (pid {})", c.display_name(), handle.pid);
    println!("note: this CLI exits immediately; the Job Object is released on exit,");
    println!("      which kills the child. Use the GUI for long-lived supervision.");
    Ok(())
}

async fn cmd_stop(args: &[String]) -> Result<()> {
    let c = require_component_arg(args, "usage: madistack stop <component>")?;
    let stored = load_stored().unwrap_or_default();
    let sup = supervisor_with_state(&stored)?;
    match sup.stop(c).await {
        Ok(()) => {
            println!("{} stopped", c.display_name());
            Ok(())
        }
        Err(e) => {
            // NotRunning is the common case after the CLI exited — report
            // but don't bubble as a hard error.
            println!("{}: {e}", c.display_name());
            Ok(())
        }
    }
}

fn cmd_logs(args: &[String]) -> Result<()> {
    let c = require_component_arg(args, "usage: madistack logs <component>")?;
    let stored = load_stored().unwrap_or_default();
    let sup = supervisor_with_state(&stored)?;
    // Note: this CLI invocation owns a fresh Supervisor — it only sees logs
    // from processes it started itself in this same run. Useful for
    // debugging from `madistack start <c>; madistack logs <c>` in the
    // same shell session; the GUI keeps its buffer across the whole
    // lifetime of the app.
    let snap = sup.logs(c).snapshot_since(0);
    for line in snap {
        println!("[{:?}] seq={} {}", line.stream, line.seq, line.text);
    }
    Ok(())
}

fn cmd_status(args: &[String]) -> Result<()> {
    let stored = load_stored().unwrap_or_default();
    let sup = supervisor_with_state(&stored)?;

    let components: Vec<Component> = match args.first().map(String::as_str) {
        None => vec![Component::Nginx, Component::Php, Component::MariaDb],
        Some(slug) => vec![parse_component(slug)?],
    };
    for c in components {
        let status = sup.status(c);
        println!("{:<10} {:?}", c.display_name(), status);
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
