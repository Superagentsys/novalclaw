fn main() {
    tauri_build::build();

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.join("../../..");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let bin_name = if cfg!(target_os = "windows") {
        "omninova.exe"
    } else {
        "omninova"
    };
    let src = workspace_root
        .join("target")
        .join(&profile)
        .join(bin_name);
    let dst_dir = manifest_dir.join("resources/cli");
    let dst = dst_dir.join(bin_name);
    if src.exists() {
        let _ = std::fs::create_dir_all(&dst_dir);
        match std::fs::copy(&src, &dst) {
            Ok(_) => println!(
                "cargo:warning=Bundled omninova CLI: {} -> {}",
                src.display(),
                dst.display()
            ),
            Err(e) => println!("cargo:warning=Failed to copy omninova CLI: {e}"),
        }
    } else {
        println!(
            "cargo:warning=omninova CLI not found at {} — run: cargo build -p omninova-core --bin omninova",
            src.display()
        );
    }
}
