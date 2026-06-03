use std::path::PathBuf;

fn main() {
    tauri_build::build();

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.join("../../..");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    // `target` 平台三元组（交叉构建时由 cargo 通过 --target 注入）。
    let target_triple = std::env::var("TARGET").ok();
    let bin_name = if cfg!(target_os = "windows") {
        "omninova.exe"
    } else {
        "omninova"
    };

    // 候选产物目录，按优先级排列：
    // 1) 由 OUT_DIR 反推的「profile 目录」(同时适配 target/<triple>/<profile>)；
    // 2) 显式 target/<triple>/<profile>；
    // 3) 顶层 target/<profile>（host 构建）。
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        // OUT_DIR = .../target[/<triple>]/<profile>/build/<crate>-<hash>/out
        // 向上第 3 层即 profile 目录。
        if let Some(profile_dir) = PathBuf::from(&out_dir).ancestors().nth(3) {
            candidates.push(profile_dir.join(bin_name));
        }
    }
    if let Some(triple) = &target_triple {
        candidates.push(
            workspace_root
                .join("target")
                .join(triple)
                .join(&profile)
                .join(bin_name),
        );
    }
    candidates.push(workspace_root.join("target").join(&profile).join(bin_name));

    let dst_dir = manifest_dir.join("resources/cli");
    let dst = dst_dir.join(bin_name);

    let found = candidates.iter().find(|p| p.exists());
    match found {
        Some(src) => {
            let _ = std::fs::create_dir_all(&dst_dir);
            match std::fs::copy(src, &dst) {
                Ok(_) => println!(
                    "cargo:warning=Bundled omninova CLI: {} -> {}",
                    src.display(),
                    dst.display()
                ),
                Err(e) => println!("cargo:warning=Failed to copy omninova CLI: {e}"),
            }
        }
        None => {
            println!(
                "cargo:warning=omninova CLI not found in any of {:?} — release builds via `npm run build:<platform>` build it automatically; for manual builds run: cargo build --release -p omninova-core --bin omninova{}",
                candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                target_triple.as_ref().map(|t| format!(" --target {t}")).unwrap_or_default()
            );
        }
    }
}
