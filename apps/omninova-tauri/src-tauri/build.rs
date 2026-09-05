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
    let bin_name = if target_os == "windows" {
        "omninova.exe"
    } else {
        "omninova"
    };
    let default_target_dir = workspace_root.join("target");
    let cargo_target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| default_target_dir.clone());
    let target_triple = std::env::var("TARGET").unwrap_or_default();

    let mut candidates = Vec::new();
    if !target_triple.is_empty() {
        candidates.push(
            cargo_target_dir
                .join(&target_triple)
                .join(&profile)
                .join(bin_name),
        );
    }
    candidates.push(cargo_target_dir.join(&profile).join(bin_name));
    // `target/release` on a bind-mounted macOS workspace is a host binary.
    // Only use it when Cargo is actually writing into that directory.
    if cargo_target_dir == default_target_dir {
        if !target_triple.is_empty() {
            candidates.push(
                default_target_dir
                    .join(&target_triple)
                    .join(&profile)
                    .join(bin_name),
            );
        }
        candidates.push(default_target_dir.join(&profile).join(bin_name));
    }

    let dst_dir = manifest_dir.join("resources/cli");
    let dst = dst_dir.join(bin_name);
    if let Some(src) = candidates.into_iter().find(|path| path.exists()) {
        let src_bytes = std::fs::read(&src);
        let dest_same = std::fs::read(&dst)
            .ok()
            .zip(src_bytes.as_ref().ok())
            .is_some_and(|(dest, src)| dest == *src);
        if dest_same {
            println!(
                "cargo:warning=Bundled omninova CLI unchanged: {}",
                dst.display()
            );
        } else {
            let _ = std::fs::create_dir_all(&dst_dir);
            match src_bytes.and_then(|bytes| {
                std::fs::write(&dst, bytes)?;
                Ok(())
            }) {
                Ok(_) => println!(
                    "cargo:warning=Bundled omninova CLI: {} -> {}",
                    src.display(),
                    dst.display()
                ),
                Err(e) => println!("cargo:warning=Failed to copy omninova CLI: {e}"),
            }
        }
    } else {
        println!(
            "cargo:warning=omninova CLI not found — run: cargo build -p omninova-core --bin omninova --release"
        );
    }
}
