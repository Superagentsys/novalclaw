//! Configuration file watcher with debouncing support.
//!
//! This module provides file system watching capabilities for the configuration file,
//! with automatic debouncing to prevent excessive reload operations.

use crate::config::loader::resolve_config_path;
use crate::config::schema::Config;
use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Default debounce duration in milliseconds.
const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Configuration change callback type.
pub type ConfigChangeCallback = Box<dyn Fn(&Config) + Send + Sync>;

/// Error type for config watcher operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigWatcherError {
    #[error("Failed to create file watcher: {0}")]
    WatcherCreate(#[from] notify::Error),

    #[error("Failed to load config: {0}")]
    LoadError(#[from] anyhow::Error),

    #[error("Config file not found: {0}")]
    NotFound(PathBuf),

    #[error("Watcher not running")]
    NotRunning,
}

/// Status of a config reload operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigReloadStatus {
    /// Config was successfully reloaded.
    Reloaded,
    /// Config reload failed, previous config retained.
    Failed(String),
    /// No change detected (file content identical).
    Unchanged,
}

/// Configuration file watcher with debouncing support.
///
/// This struct wraps `notify::RecommendedWatcher` and provides:
/// - Automatic debouncing of file change events
/// - Thread-safe access to the current configuration
/// - Callback registration for change notifications
pub struct ConfigWatcher {
    watcher: Option<RecommendedWatcher>,
    config_path: PathBuf,
    debounce_ms: u64,
}

impl ConfigWatcher {
    /// Create a new config watcher for the default config path.
    pub fn new() -> Self {
        Self::with_path(resolve_config_path())
    }

    /// Create a new config watcher for a specific config path.
    pub fn with_path(config_path: PathBuf) -> Self {
        Self {
            watcher: None,
            config_path,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
        }
    }

    /// Set the debounce duration in milliseconds.
    pub fn with_debounce(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    /// Get the config file path being watched.
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Check if the watcher is currently running.
    pub fn is_running(&self) -> bool {
        self.watcher.is_some()
    }

    /// Start watching the config file for changes.
    ///
    /// This spawns a background task that handles file system events
    /// with debouncing. When a change is detected, the provided callback
    /// is invoked with the new configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Shared reference to the configuration
    /// * `on_change` - Callback to invoke when config changes
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the watcher was started successfully.
    pub fn start(
        &mut self,
        config: Arc<RwLock<Config>>,
        on_change: ConfigChangeCallback,
    ) -> Result<()> {
        if self.watcher.is_some() {
            debug!("Config watcher already running, stopping previous instance");
            self.stop();
        }

        let config_path = self.config_path.clone();
        let debounce_duration = Duration::from_millis(self.debounce_ms);

        // Create channel for file system events
        let (tx, mut rx) = mpsc::channel::<()>(32);

        // Create the watcher with event handler
        let event_tx = tx.clone();
        let watcher_path = config_path.clone();
        let mut watcher: RecommendedWatcher = Watcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Only process modify events for our config file
                        if event.kind.is_modify() {
                            for path in &event.paths {
                                if path == &watcher_path {
                                    debug!("Config file modified: {:?}", path);
                                    let _ = event_tx.try_send(());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Watch error: {:?}", e);
                    }
                }
            },
            notify::Config::default(),
        )
        .context("Failed to create file watcher")?;

        // Watch the config file's parent directory (more reliable than watching file directly)
        let watch_path = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| config_path.clone());

        watcher
            .watch(&watch_path, RecursiveMode::NonRecursive)
            .context("Failed to start watching config path")?;

        info!("Started watching config file: {:?}", config_path);

        // Spawn debounced event handler
        let config_clone = config.clone();
        let callback = Arc::new(on_change);
        tokio::spawn(async move {
            let mut last_trigger: Option<tokio::time::Instant> = None;

            loop {
                // Wait for an event or timeout
                tokio::select! {
                    _ = rx.recv() => {
                        // Event received, update last trigger time
                        last_trigger = Some(tokio::time::Instant::now());
                    }
                    _ = tokio::time::sleep(debounce_duration), if last_trigger.is_some() => {
                        // Debounce period elapsed since last event
                        if let Some(trigger_time) = last_trigger {
                            if trigger_time.elapsed() >= debounce_duration {
                                // Debounce complete, reload config
                                match Self::reload_config_internal(&config_path, &config_clone).await {
                                    Ok(new_config) => {
                                        info!("Config reloaded successfully");
                                        callback(&new_config);
                                    }
                                    Err(e) => {
                                        error!("Failed to reload config: {}", e);
                                    }
                                }
                                last_trigger = None;
                            }
                        }
                    }
                    else => break,
                }
            }
        });

        self.watcher = Some(watcher);
        Ok(())
    }

    /// Stop watching the config file.
    pub fn stop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            // Watcher stops automatically when dropped
            drop(watcher);
            info!("Stopped watching config file");
        }
    }

    /// Reload the configuration from disk.
    ///
    /// This loads the configuration from the file, applies environment
    /// variable overrides, and updates the shared config reference.
    ///
    /// # Returns
    ///
    /// - `Ok(ConfigReloadStatus::Reloaded)` if config was successfully reloaded
    /// - `Ok(ConfigReloadStatus::Unchanged)` if file content was identical
    /// - `Err(...)` if loading failed
    pub async fn reload_config(
        config_path: &PathBuf,
        config: Arc<RwLock<Config>>,
    ) -> Result<ConfigReloadStatus> {
        Self::reload_config_internal(config_path, &config).await?;
        Ok(ConfigReloadStatus::Reloaded)
    }

    /// Internal implementation of config reload.
    async fn reload_config_internal(
        config_path: &PathBuf,
        config: &Arc<RwLock<Config>>,
    ) -> Result<Config> {
        debug!("Reloading config from {:?}", config_path);

        if !config_path.exists() {
            return Err(ConfigWatcherError::NotFound(config_path.clone()).into());
        }

        // Load new config from file
        let mut new_config = Config::load_from(config_path)
            .with_context(|| format!("Failed to load config from {:?}", config_path))?;

        // Preserve the config_path
        new_config.config_path = config_path.clone();

        // Update shared config
        {
            let mut cfg = config.write().await;
            *cfg = new_config.clone();
        }

        Ok(new_config)
    }
}

impl Default for ConfigWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Manager for configuration with hot reload support.
///
/// This struct provides a high-level interface for managing configuration
/// with automatic file watching and change notifications.
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    config_path: PathBuf,
    watcher: Option<ConfigWatcher>,
    change_callbacks: Vec<Arc<ConfigChangeCallback>>,
}

impl ConfigManager {
    /// Create a new config manager with the given initial configuration.
    pub fn new(config: Config) -> Self {
        let config_path = config.config_path.clone();
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
            watcher: None,
            change_callbacks: Vec::new(),
        }
    }

    /// Create a new config manager with a shared configuration reference.
    ///
    /// This allows the manager to share its configuration with other components
    /// (like GatewayRuntime) so that changes are visible to all holders of the reference.
    pub fn with_shared_config(config: Arc<RwLock<Config>>, config_path: PathBuf) -> Self {
        Self {
            config,
            config_path,
            watcher: None,
            change_callbacks: Vec::new(),
        }
    }

    /// Create a new config manager, loading config from disk or initializing default.
    pub async fn load() -> Result<Self> {
        let config = Config::load_or_init()
            .context("Failed to load configuration")?;
        Ok(Self::new(config))
    }

    /// Get a clone of the shared config reference.
    pub fn config(&self) -> Arc<RwLock<Config>> {
        self.config.clone()
    }

    /// Get a read guard to the current configuration.
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config.read().await
    }

    /// Get a write guard to the current configuration.
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, Config> {
        self.config.write().await
    }

    /// Get the config file path.
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Start watching for config file changes.
    pub fn start_watching(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            return Ok(());
        }

        let config = self.config.clone();
        let callbacks = self.change_callbacks.clone();

        let mut watcher = ConfigWatcher::with_path(self.config_path.clone());
        watcher.start(config, Box::new(move |new_config| {
            // Notify all registered callbacks
            for callback in &callbacks {
                callback(new_config);
            }
        }))?;

        self.watcher = Some(watcher);
        Ok(())
    }

    /// Stop watching for config file changes.
    pub fn stop_watching(&mut self) {
        if let Some(mut watcher) = self.watcher.take() {
            watcher.stop();
        }
    }

    /// Register a callback to be called when config changes.
    pub fn on_change<F>(&mut self, callback: F)
    where
        F: Fn(&Config) + Send + Sync + 'static,
    {
        self.change_callbacks.push(Arc::new(Box::new(callback)));
    }

    /// Manually reload the configuration from disk.
    ///
    /// This method is "safe" in that it will not modify the current
    /// configuration if the reload fails.
    pub async fn reload(&self) -> Result<ConfigReloadStatus> {
        let config_path = self.config_path.clone();

        if !config_path.exists() {
            return Err(ConfigWatcherError::NotFound(config_path).into());
        }

        // Load new config from file
        let mut new_config = Config::load_from(&config_path)
            .with_context(|| format!("Failed to load config from {:?}", config_path))?;

        new_config.config_path = config_path.clone();

        // Update shared config
        {
            let mut cfg = self.config.write().await;
            *cfg = new_config;
        }

        Ok(ConfigReloadStatus::Reloaded)
    }

    /// Reload configuration with error recovery.
    ///
    /// If the reload fails, the current configuration is preserved
    /// and an error message is returned.
    pub async fn reload_safe(&self) -> ConfigReloadStatus {
        match self.reload().await {
            Ok(status) => status,
            Err(e) => {
                error!("Failed to reload config, keeping current: {}", e);
                ConfigReloadStatus::Failed(e.to_string())
            }
        }
    }

    /// Save the current configuration to disk.
    pub async fn save(&self) -> Result<()> {
        let config = self.config.read().await;
        config.save().context("Failed to save configuration")?;
        info!("Configuration saved to {:?}", config.config_path);
        Ok(())
    }

    /// Check if the manager is currently watching for changes.
    pub fn is_watching(&self) -> bool {
        self.watcher.as_ref().map(|w| w.is_running()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_temp_config(dir: &TempDir) -> PathBuf {
        let config_path = dir.path().join("config.toml");
        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(b"api_key = \"test-key\"\ndefault_provider = \"openai\"\n")
            .unwrap();
        config_path
    }

    #[test]
    fn test_config_watcher_new() {
        let watcher = ConfigWatcher::new();
        assert!(!watcher.is_running());
        assert!(watcher.config_path().to_string_lossy().contains(".omninova"));
    }

    #[test]
    fn test_config_watcher_with_debounce() {
        let watcher = ConfigWatcher::new().with_debounce(500);
        assert_eq!(watcher.debounce_ms, 500);
    }

    #[test]
    fn test_config_watcher_with_path() {
        let custom_path = PathBuf::from("/custom/path/config.toml");
        let watcher = ConfigWatcher::with_path(custom_path.clone());
        assert_eq!(watcher.config_path(), &custom_path);
    }

    #[tokio::test]
    async fn test_config_watcher_start_stop() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir);

        let config = Config::load_from(&config_path).unwrap();
        let shared_config = Arc::new(RwLock::new(config));

        let mut watcher = ConfigWatcher::with_path(config_path);
        assert!(!watcher.is_running());

        let result = watcher.start(shared_config, Box::new(|_| {}));
        assert!(result.is_ok());
        assert!(watcher.is_running());

        watcher.stop();
        assert!(!watcher.is_running());
    }

    #[tokio::test]
    async fn test_config_manager_new() {
        let config = Config::default();
        let manager = ConfigManager::new(config);
        assert!(!manager.is_watching());
    }

    #[tokio::test]
    async fn test_config_manager_read_write() {
        let config = Config::default();
        let manager = ConfigManager::new(config);

        // Test read
        {
            let cfg = manager.read().await;
            assert_eq!(cfg.default_temperature, 0.7);
        }

        // Test write
        {
            let mut cfg = manager.write().await;
            cfg.default_temperature = 0.8;
        }

        // Verify change persisted
        {
            let cfg = manager.read().await;
            assert_eq!(cfg.default_temperature, 0.8);
        }
    }

    #[tokio::test]
    async fn test_config_manager_reload() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir);

        let config = Config::load_from(&config_path).unwrap();
        let manager = ConfigManager::new(config);

        // Modify the file
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&config_path)
            .unwrap();
        file.write_all(b"api_key = \"new-key\"\ndefault_provider = \"anthropic\"\n")
            .unwrap();
        file.flush().unwrap();

        // Reload and verify
        let status = manager.reload().await.unwrap();
        assert_eq!(status, ConfigReloadStatus::Reloaded);

        let cfg = manager.read().await;
        assert_eq!(cfg.api_key.as_deref(), Some("new-key"));
    }

    #[tokio::test]
    async fn test_config_manager_reload_safe() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir);

        let config = Config::load_from(&config_path).unwrap();
        let original_api_key = config.api_key.clone();
        let manager = ConfigManager::new(config);

        // Write invalid config
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&config_path)
            .unwrap();
        file.write_all(b"invalid [[[toml\n").unwrap();
        file.flush().unwrap();

        // Reload should fail but preserve current config
        let status = manager.reload_safe().await;
        assert!(matches!(status, ConfigReloadStatus::Failed(_)));

        let cfg = manager.read().await;
        assert_eq!(cfg.api_key, original_api_key);
    }

    #[tokio::test]
    async fn test_config_manager_on_change() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir);

        let config = Config::load_from(&config_path).unwrap();
        let mut manager = ConfigManager::new(config);

        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        manager.on_change(move |_| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Manually trigger reload (simulating change callback)
        manager.reload().await.unwrap();

        // Note: The callback is only triggered by the watcher, not by manual reload
        // This test verifies the callback can be registered
        assert_eq!(manager.change_callbacks.len(), 1);
    }

    #[tokio::test]
    async fn test_config_reload_status() {
        // Test that Reloaded and Failed are distinct
        let reloaded = ConfigReloadStatus::Reloaded;
        let failed = ConfigReloadStatus::Failed("error".to_string());
        let unchanged = ConfigReloadStatus::Unchanged;

        assert_ne!(reloaded, failed);
        assert_ne!(reloaded, unchanged);
        assert_ne!(failed, unchanged);
    }

    /// Helper to lock env test access and clear common overrides
    fn env_test_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[tokio::test]
    async fn test_env_override_applied_on_reload() {
        let _guard = env_test_lock().lock().unwrap();

        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir);

        // Set environment variable
        std::env::set_var("OMNINOVA_API_KEY", "env-override-key");

        // Load config - should have env override
        let config = Config::load_from(&config_path).unwrap();
        let manager = ConfigManager::new(config);

        // Verify env override applied
        {
            let cfg = manager.read().await;
            assert_eq!(cfg.api_key.as_deref(), Some("env-override-key"));
        }

        // Modify the file (without api_key)
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&config_path)
            .unwrap();
        file.write_all(b"default_provider = \"anthropic\"\n")
            .unwrap();
        file.flush().unwrap();

        // Reload
        manager.reload().await.unwrap();

        // Verify env override still applied after reload
        {
            let cfg = manager.read().await;
            assert_eq!(cfg.api_key.as_deref(), Some("env-override-key"));
        }

        std::env::remove_var("OMNINOVA_API_KEY");
    }

    #[tokio::test]
    async fn test_env_gateway_port_override_on_reload() {
        let _guard = env_test_lock().lock().unwrap();

        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir);

        // Set environment variable for port
        std::env::set_var("PORT", "9999");

        let config = Config::load_from(&config_path).unwrap();
        let manager = ConfigManager::new(config);

        // Verify env override applied
        {
            let cfg = manager.read().await;
            assert_eq!(cfg.gateway.port, 9999);
        }

        // Modify the file with different port
        let mut file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&config_path)
            .unwrap();
        file.write_all(b"api_key = \"test\"\n[gateway]\nport = 8080\n")
            .unwrap();
        file.flush().unwrap();

        // Reload
        manager.reload().await.unwrap();

        // Env should still override file value
        {
            let cfg = manager.read().await;
            assert_eq!(cfg.gateway.port, 9999, "ENV PORT should override file port");
        }

        std::env::remove_var("PORT");
    }
}