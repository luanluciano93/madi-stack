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

    let dst_dir = PathBuf::from("binaries");
    if let Err(e) = fs::create_dir_all(&dst_dir) {
        println!("cargo:warning=could not create {}: {e}", dst_dir.display());
        return;
    }
    let dst = dst_dir.join(format!("madistack-system-helper-{target}.exe"));

    if src.is_file() {
        if let Err(e) = fs::copy(&src, &dst) {
            println!(
                "cargo:warning=failed to stage sidecar at {}: {e}",
                dst.display()
            );
        }
        return;
    }

    // Helper not built yet — happens during `cargo clippy`/`cargo check` in
    // CI before the helper is compiled. Tauri's `externalBin` validator
    // insists the file exists at build time, otherwise the build fails with
    // "resource path ... doesn't exist". Drop a harmless placeholder so
    // the workspace still compiles; real bundles (`cargo tauri build` after
    // `cargo build --release -p madistack-system-helper`) overwrite it
    // with the actual binary. The placeholder is never executed.
    if !dst.is_file() {
        if let Err(e) = fs::write(&dst, b"placeholder\n") {
            println!(
                "cargo:warning=could not create sidecar placeholder at {}: {e}",
                dst.display()
            );
            return;
        }
        println!(
            "cargo:warning=madistack-system-helper not built yet — staged a \
             placeholder at {}. Run `cargo build --profile {} -p \
             madistack-system-helper` before `cargo tauri build` for a real \
             bundle.",
            dst.display(),
            profile
        );
    }
}
