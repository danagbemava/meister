use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=scripts/fetch-base-model.sh");
    println!("cargo:rerun-if-changed=build.rs");

    if cfg!(target_os = "macos") {
        if let Some(dir) = find_swift_runtime_dir() {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
        } else {
            println!("cargo:warning=could not resolve Swift runtime dir; tests linking Swift (screencapturekit) may fail to load libswift_Concurrency.dylib at runtime");
        }
    }

    let script = std::path::Path::new("scripts/fetch-base-model.sh");
    if script.exists() {
        let status = Command::new("bash")
            .arg(script)
            .status()
            .expect("failed to invoke fetch-base-model.sh");
        if !status.success() {
            panic!("fetch-base-model.sh failed (exit {:?}). Cannot build a meister bundle without the bundled whisper model.", status.code());
        }
    }

    // whisper.cpp's Metal backend looks for ggml-metal.metal at runtime
    // (via NSBundle's resource path or GGML_METAL_PATH_RESOURCES). Copy
    // the shader from whisper-rs-sys's source into our resources/ dir so
    // it can be bundled with the .app and resolved in dev mode too.
    let metal_target = PathBuf::from("resources/ggml-metal.metal");
    if !metal_target.is_file() {
        if let Some(src) = find_metal_shader() {
            std::fs::copy(&src, &metal_target)
                .unwrap_or_else(|e| panic!("copy ggml-metal.metal: {e}"));
            println!("cargo:warning=copied ggml-metal.metal from {}", src.display());
        } else {
            println!("cargo:warning=could not find ggml-metal.metal in cargo registry; Metal backend will fall back to CoreML+CPU at runtime");
        }
    }

    tauri_build::build();
}

/// Resolve the directory containing `libswift_Concurrency.dylib` (and friends)
/// so tests and the unbundled binary can load Swift runtime at dev-time.
/// The install name on screencapturekit's Swift output is @rpath-based, so we
/// add Xcode's Swift 5.5 back-deployment libs (which always ship these dylibs)
/// as an rpath. Portable across machines: resolves via `xcode-select -p`.
fn find_swift_runtime_dir() -> Option<PathBuf> {
    let dev = Command::new("xcode-select").arg("-p").output().ok()?;
    if !dev.status.success() { return None; }
    let dev = String::from_utf8(dev.stdout).ok()?;
    let dev = dev.trim();
    let candidate = PathBuf::from(dev)
        .join("Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx");
    if candidate.join("libswift_Concurrency.dylib").is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn find_metal_shader() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let registry = PathBuf::from(home).join(".cargo/registry/src");
    let output = Command::new("find")
        .arg(&registry)
        .args(["-name", "ggml-metal.metal", "-path", "*whisper-rs-sys*"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim().to_string();
    if line.is_empty() { None } else { Some(PathBuf::from(line)) }
}
