use std::process::Command;

fn main() {
    // Re-run if the script itself changes
    println!("cargo:rerun-if-changed=scripts/fetch-base-model.sh");

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

    tauri_build::build();
}
