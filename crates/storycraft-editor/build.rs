//! On Windows with the GNU toolchain, WebView2 (used by the preview) links
//! against `WebView2Loader.dll` instead of statically, and Windows will not
//! start the editor unless that DLL sits next to the executable. Copy the
//! copy shipped with `webview2-com-sys` there. MSVC builds link the loader
//! statically and need nothing.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows_gnu = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    if !windows_gnu {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => return,
    };
    let Some(dll) = find_loader(arch) else {
        println!(
            "cargo:warning=WebView2Loader.dll was not found; copy it next to storycraft.exe or the editor will not start"
        );
        return;
    };
    // OUT_DIR is <target>/<profile>/build/<crate>-<hash>/out.
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let Some(profile_dir) = out_dir.ancestors().nth(3) else { return };
    for dir in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = std::fs::copy(&dll, dir.join("WebView2Loader.dll")) {
            println!("cargo:warning=could not copy WebView2Loader.dll to {}: {e}", dir.display());
        }
    }
}

/// Locate `webview2-com-sys/<arch>/WebView2Loader.dll` in Cargo's registry,
/// using the version pinned in Cargo.lock.
fn find_loader(arch: &str) -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR")?);
    let lock = std::fs::read_to_string(manifest_dir.parent()?.parent()?.join("Cargo.lock")).ok()?;
    let version = locked_version(&lock, "webview2-com-sys")?;
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(|h| Path::new(&h).join(".cargo")))?;
    let registries = std::fs::read_dir(cargo_home.join("registry").join("src")).ok()?;
    registries
        .flatten()
        .map(|r| r.path().join(format!("webview2-com-sys-{version}")).join(arch).join("WebView2Loader.dll"))
        .find(|p| p.exists())
}

fn locked_version(lock: &str, package: &str) -> Option<String> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == format!("name = \"{package}\"") {
            let version = lines.next()?.trim().strip_prefix("version = \"")?.strip_suffix('"')?;
            return Some(version.to_owned());
        }
    }
    None
}
