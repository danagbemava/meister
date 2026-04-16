use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=scripts/fetch-base-model.sh");
    println!("cargo:rerun-if-changed=build.rs");

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
