//! OmniNova Personal Chrome transport substrate.
//!
//! This crate is a thin pipe: Chrome Native Messaging ↔ authenticated local IPC.
//! It must not depend on omninova-core or construct an Agent, browser runtime, or backend.

pub mod constants;
pub mod desktop;
pub mod error;
pub mod framing;
pub mod health;
pub mod host_loop;
pub mod install;
pub mod ipc;
pub mod origin;
pub mod protocol;
pub mod secret;

pub use constants::{
    allowed_origin, application_max_message_bytes, dev_extension_id, native_host_name,
    protocol_version,
};
pub use desktop::PersonalChromeBridge;
pub use error::BridgeError;
pub use health::{TransportHealth, TransportStatus};
pub use host_loop::run_native_host;
pub use install::{
    install_host, install_product_host, remove_host, remove_product_host, resolve_host_executable,
    verify_host, verify_product_host,
};
pub use origin::verify_connecting_origin;

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn crate_is_not_an_agent_or_backend() {
        let cargo = include_str!("../Cargo.toml");
        assert!(!cargo.contains("omninova-core"));
        assert!(!cargo.contains("omninova-tauri"));
    }

    #[test]
    fn host_sources_do_not_write_stdout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for name in ["main.rs", "host_loop.rs"] {
            let src = std::fs::read_to_string(root.join(name)).unwrap();
            for line in src.lines() {
                let trimmed = line.trim();
                if trimmed.contains("println!") && !trimmed.contains("eprintln!") {
                    panic!("{name} must not use println! because stdout is Native Messaging: {trimmed}");
                }
            }
        }
        let host = std::fs::read_to_string(root.join("host_loop.rs")).unwrap();
        assert!(host.contains("write_stdout_frame"));
        assert!(host.contains("encode_raw_json"));
    }
}
