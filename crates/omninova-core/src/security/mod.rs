pub mod crypto;
pub mod dangerous_tools;
pub mod estop;
pub mod keyring;
pub mod password;
pub mod tool_policy;

pub use crypto::{
    AesGcmEncryption, EncryptionError, EncryptionKeyManager, EncryptionService,
    EncryptionSettings, encrypt_string, decrypt_string,
};
pub use estop::{EstopController, EstopState};
pub use keyring::{
    KeyringError, KeyringService, KeyReference, SecretStore,
    HybridSecretStore, OsKeyring, FallbackStorage,
};
pub use password::{hash_password, verify_password, validate_password_strength, PasswordError};
pub use tool_policy::resolve_shell_allowlist;
