//! src-tauri build script.
//!
//! In addition to the standard `tauri_build::build()`, we stage the
//! already-compiled `madistack-system-helper` as a Tauri sidecar so that
//! `cargo tauri build` includes it in the NSIS/MSI bundle. The bundler
//! requires `binaries/<name>-<target-triple>.exe` — we copy from
//! `target/<profile>/` into `src-tauri/binaries/` with that naming.
//!
//! The helper must have been built first (`cargo build --release -p
//! madistack-system-helper`) — we emit a `cargo:warning` if missing so the
//! developer sees why the sidecar didn't ship.

use std::{env, fs, path::PathBuf};

fn main() {
    stage_system_helper_sidecar();
    tauri_build::build();
}

fn stage_system_helper_sidecar() {
    let Ok(target) = env::var("TARGET") else {
        println!("cargo:warning=TARGET env missing; skipping sidecar stage");
        return;
    };
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());

    let src = PathBuf::from("..")
        .join("target")
        .join(&profile)
        .join("madistack-system-helper.exe");

    println!("cargo:rerun-if-changed={}", src.display());

    if !src.is_file() {
        println!(
            "cargo:warning=madistack-system-helper not found at {}. Run \
             `cargo build --profile {} -p madistack-system-helper` before \
             `cargo tauri build` to include it in the bundle.",
            src.display(),
            profile
        );
        return;
    }

    let dst_dir = PathBuf::from("binaries");
    if let Err(e) = fs::create_dir_all(&dst_dir) {
        println!("cargo:warning=could not create {}: {e}", dst_dir.display());
        return;
    }
    let dst = dst_dir.join(format!("madistack-system-helper-{target}.exe"));
    if let Err(e) = fs::copy(&src, &dst) {
        println!(
            "cargo:warning=failed to stage sidecar at {}: {e}",
            dst.display()
        );
    }
}
