fn main() {
    tauri_build::build();

    // Windows: raise the executable's default thread stack reserve to 8 MiB.
    //
    // The `Config` struct is large and deeply nested, so serde (de)serialization
    // to/from JSON/TOML is stack-hungry. Tauri deserializes command arguments and
    // serializes results on its IPC/main threads — created with the OS default
    // stack size (PE `SizeOfStackReserve`, ~1 MiB by default on MSVC). That caused
    // a `0xC00000FD` (STATUS_STACK_OVERFLOW) crash when saving the model config.
    // Threads created with stack size 0 inherit this reserve, so this covers the
    // main thread, Tauri/wry internal threads and the IPC response path.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }

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
