//! Compiles `storycraft-player` to WebAssembly so the exporter can embed it.
//!
//! Set `STORYCRAFT_PLAYER_WASM=/path/to/storycraft_player.wasm` to use a
//! prebuilt runtime instead (for example in packaging pipelines).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const WASM_TARGET: &str = "wasm32-unknown-unknown";

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let dest = out_dir.join("storycraft_player.wasm");

    println!("cargo:rerun-if-env-changed=STORYCRAFT_PLAYER_WASM");
    if let Some(prebuilt) = env::var_os("STORYCRAFT_PLAYER_WASM") {
        println!("cargo:rerun-if-changed={}", Path::new(&prebuilt).display());
        std::fs::copy(&prebuilt, &dest).expect("copy STORYCRAFT_PLAYER_WASM");
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir.parent().and_then(Path::parent).unwrap().to_path_buf();
    for krate in ["storycraft-player", "storycraft-engine", "storycraft-model"] {
        println!("cargo:rerun-if-changed={}", workspace.join("crates").join(krate).join("src").display());
        println!("cargo:rerun-if-changed={}", workspace.join("crates").join(krate).join("Cargo.toml").display());
    }

    // A separate target directory avoids contending for the lock held by the
    // outer build.
    let target_dir = workspace.join("target").join("player-wasm");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(&workspace)
        .args(["build", "--package", "storycraft-player", "--target", WASM_TARGET, "--profile", "player"])
        .arg("--target-dir")
        .arg(&target_dir);
    // Flags meant for the host build must not leak into the wasm build.
    for var in
        ["CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS", "CARGO_BUILD_TARGET", "CARGO_TARGET_DIR", "CARGO_BUILD_RUSTFLAGS"]
    {
        cmd.env_remove(var);
    }

    let built = target_dir.join(WASM_TARGET).join("player").join("storycraft_player.wasm");
    match cmd.status() {
        Ok(status) if status.success() && built.exists() => {
            std::fs::copy(&built, &dest).expect("copy player wasm");
        }
        result => {
            // Keep the rest of the workspace buildable; exporting reports the
            // problem at runtime instead.
            std::fs::write(&dest, []).unwrap();
            let why = match result {
                Ok(status) => format!("cargo exited with {status}"),
                Err(e) => e.to_string(),
            };
            println!(
                "cargo:warning=could not build the WebAssembly player ({why}); publishing will be unavailable. \
                 Install the target with `rustup target add {WASM_TARGET}`."
            );
        }
    }
}
