use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Top-level Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub workspace_dir: PathBuf,
    #[serde(skip)]
    pub config_path: PathBuf,

    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    #[serde(default = "default_temperature")]
    pub default_temperature: f64,
    pub provider_api: Option<ProviderApiMode>,

    #[serde(default)]
    pub model_providers: HashMap<String, ModelProviderConfig>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub model_routes: Vec<ModelRouteConfig>,
    #[serde(default)]
    pub embedding_routes: Vec<EmbeddingRouteConfig>,

    #[serde(default)]
    pub provider: ProviderBehaviorConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub autonomy: AutonomyConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub tunnel: TunnelConfig,

    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub http_request: HttpRequestConfig,
    #[serde(default)]
    pub web_fetch: WebFetchConfig,
    #[serde(default)]
    pub web_search: WebSearchConfig,
    #[serde(default)]
    pub composio: ComposioConfig,

    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub query_classification: QueryClassificationConfig,
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub cron: CronConfig,
    #[serde(default)]
    pub goal_loop: GoalLoopConfig,
    #[serde(default)]
    pub channels_config: ChannelsConfig,
    #[serde(default)]
    pub reliability: ReliabilityConfig,
    #[serde(default)]
    pub research: ResearchPhaseConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub cost: CostConfig,
    #[serde(default)]
    pub multimodal: MultimodalConfig,
    #[serde(default)]
    pub transcription: TranscriptionConfig,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,

    #[serde(default)]
    pub coordination: CoordinationConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default)]
    pub hardware: HardwareConfig,
    #[serde(default)]
    pub peripherals: PeripheralsConfig,

    #[serde(default)]
    pub agents: HashMap<String, DelegateAgentConfig>,
    #[serde(default)]
    pub agents_ipc: AgentsIpcConfig,

    pub model_support_vision: Option<bool>,

    #[serde(default)]
    pub robot: Option<RobotConfig>,
}

fn default_temperature() -> f64 {
    0.7
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs_home();
        let omninova_dir = home.join(".omninova");
        Self {
            workspace_dir: omninova_dir.join("workspace"),
            config_path: omninova_dir.join("config.toml"),
            api_key: None,
            api_url: None,
            default_provider: Some("openrouter".into()),
            default_model: Some("anthropic/claude-sonnet-4-20250514".into()),
            default_temperature: 0.7,
            provider_api: None,
            model_providers: HashMap::new(),
            providers: Vec::new(),
            model_routes: Vec::new(),
            embedding_routes: Vec::new(),
            provider: ProviderBehaviorConfig::default(),
            agent: AgentConfig::default(),
            autonomy: AutonomyConfig::default(),
            security: SecurityConfig::default(),
            runtime: RuntimeConfig::default(),
            memory: MemoryConfig::default(),
            storage: StorageConfig::default(),
            observability: ObservabilityConfig::default(),
            gateway: GatewayConfig::default(),
            proxy: ProxyConfig::default(),
            tunnel: TunnelConfig::default(),
            browser: BrowserConfig::default(),
            http_request: HttpRequestConfig::default(),
            web_fetch: WebFetchConfig::default(),
            web_search: WebSearchConfig::default(),
            composio: ComposioConfig::default(),
            skills: SkillsConfig::default(),
            query_classification: QueryClassificationConfig::default(),
            heartbeat: HeartbeatConfig::default(),
            cron: CronConfig::default(),
            goal_loop: GoalLoopConfig::default(),
            channels_config: ChannelsConfig::default(),
            reliability: ReliabilityConfig::default(),
            research: ResearchPhaseConfig::default(),
            scheduler: SchedulerConfig::default(),
            cost: CostConfig::default(),
            multimodal: MultimodalConfig::default(),
            transcription: TranscriptionConfig::default(),
            identity: IdentityConfig::default(),
            secrets: SecretsConfig::default(),
            coordination: CoordinationConfig::default(),
            hooks: HooksConfig::default(),
            hardware: HardwareConfig::default(),
            peripherals: PeripheralsConfig::default(),
            agents: HashMap::new(),
            agents_ipc: AgentsIpcConfig::default(),
            model_support_vision: None,
            robot: None,
        }
    }
}

fn dirs_home() -> PathBuf {
    home::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// ---------------------------------------------------------------------------
// Provider API Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApiMode {
    Chat,
    Completion,
    Responses,
}

// ---------------------------------------------------------------------------
// Model Provider (named profile)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelProviderConfig {
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// ProviderConfig (legacy list-style)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Provider Behavior
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBehaviorConfig {
    #[serde(default = "default_reasoning_level")]
    pub reasoning_level: String,
}

fn default_reasoning_level() -> String {
    "medium".into()
}

impl Default for ProviderBehaviorConfig {
    fn default() -> Self {
        Self {
            reasoning_level: default_reasoning_level(),
        }
    }
}

// ---------------------------------------------------------------------------
// Model Routing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRouteConfig {
    pub hint: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingRouteConfig {
    pub hint: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    #[serde(default = "default_true")]
    pub compact_context: bool,
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,
    #[serde(default = "default_max_history_messages")]
    pub max_history_messages: usize,
    #[serde(default)]
    pub parallel_tools: bool,
    pub tool_dispatcher: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_max_tool_iterations() -> usize {
    20
}
fn default_max_history_messages() -> usize {
    50
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "omninova".into(),
            description: None,
            system_prompt: None,
            compact_context: true,
            max_tool_iterations: 20,
            max_history_messages: 50,
            parallel_tools: false,
            tool_dispatcher: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Delegate Agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DelegateAgentConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub max_depth: Option<u32>,
    #[serde(default)]
    pub agentic: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub max_iterations: Option<usize>,
}

// ---------------------------------------------------------------------------
// Autonomy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyConfig {
    #[serde(default = "default_autonomy_level")]
    pub level: String,
    #[serde(default = "default_true")]
    pub workspace_only: bool,
    #[serde(default = "default_allowed_commands")]
    pub allowed_commands: Vec<String>,
    #[serde(default = "default_forbidden_paths")]
    pub forbidden_paths: Vec<String>,
    #[serde(default = "default_max_actions_per_hour")]
    pub max_actions_per_hour: u32,
    #[serde(default = "default_max_cost_per_day_cents")]
    pub max_cost_per_day_cents: u32,
    #[serde(default = "default_true")]
    pub require_approval_for_medium_risk: bool,
    #[serde(default = "default_true")]
    pub block_high_risk_commands: bool,
    #[serde(default = "default_auto_approve")]
    pub auto_approve: Vec<String>,
    #[serde(default)]
    pub non_cli_excluded_tools: Vec<String>,
}

fn default_autonomy_level() -> String {
    "supervised".into()
}
fn default_allowed_commands() -> Vec<String> {
    ["git", "npm", "cargo", "ls", "cat", "grep", "find", "echo", "pwd", "wc", "head", "tail", "date"]
        .iter().map(|s| s.to_string()).collect()
}
fn default_forbidden_paths() -> Vec<String> {
    ["/etc", "/root", "/home", "/usr", "/bin", "/sbin", "/lib", "/opt",
     "/boot", "/dev", "/proc", "/sys", "/var", "/tmp",
     "~/.ssh", "~/.gnupg", "~/.aws", "~/.config"]
        .iter().map(|s| s.to_string()).collect()
}
fn default_max_actions_per_hour() -> u32 {
    20
}
fn default_max_cost_per_day_cents() -> u32 {
    500
}
fn default_auto_approve() -> Vec<String> {
    vec!["file_read".into(), "memory_recall".into()]
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            level: default_autonomy_level(),
            workspace_only: true,
            allowed_commands: default_allowed_commands(),
            forbidden_paths: default_forbidden_paths(),
            max_actions_per_hour: default_max_actions_per_hour(),
            max_cost_per_day_cents: default_max_cost_per_day_cents(),
            require_approval_for_medium_risk: true,
            block_high_risk_commands: true,
            auto_approve: default_auto_approve(),
            non_cli_excluded_tools: vec![
                "shell".into(), "file_write".into(), "file_edit".into(),
                "git_operations".into(), "browser".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub otp: OtpConfig,
    #[serde(default)]
    pub estop: EstopConfig,
    #[serde(default)]
    pub syscall_anomaly: SyscallAnomalyConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub audit: AuditConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OtpConfig {
    #[serde(default)]
    pub enabled: bool,
    pub method: Option<String>,
    #[serde(default)]
    pub gated_actions: Vec<String>,
    #[serde(default)]
    pub gated_domains: Vec<String>,
    #[serde(default)]
    pub gated_domain_categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstopConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub state_file: Option<String>,
    #[serde(default)]
    pub require_otp_to_resume: bool,
}

impl Default for EstopConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            state_file: None,
            require_otp_to_resume: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyscallAnomalyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub strict_mode: bool,
    #[serde(default)]
    pub alert_on_unknown_syscall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditConfig {
    #[serde(default)]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_runtime_kind")]
    pub kind: String,
    #[serde(default)]
    pub reasoning_enabled: bool,
    pub reasoning_level: Option<String>,
    #[serde(default)]
    pub wasm: WasmRuntimeConfig,
}

fn default_runtime_kind() -> String {
    "native".into()
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            kind: default_runtime_kind(),
            reasoning_enabled: false,
            reasoning_level: None,
            wasm: WasmRuntimeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmRuntimeConfig {
    pub tools_dir: Option<String>,
    #[serde(default = "default_fuel_limit")]
    pub fuel_limit: u64,
    #[serde(default = "default_wasm_memory_mb")]
    pub memory_limit_mb: u32,
    #[serde(default = "default_max_module_size_mb")]
    pub max_module_size_mb: u32,
    #[serde(default)]
    pub allow_workspace_read: bool,
    #[serde(default)]
    pub allow_workspace_write: bool,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default)]
    pub security: WasmSecurityConfig,
}

fn default_fuel_limit() -> u64 {
    2_000_000
}
fn default_wasm_memory_mb() -> u32 {
    128
}
fn default_max_module_size_mb() -> u32 {
    64
}

impl Default for WasmRuntimeConfig {
    fn default() -> Self {
        Self {
            tools_dir: None,
            fuel_limit: default_fuel_limit(),
            memory_limit_mb: default_wasm_memory_mb(),
            max_module_size_mb: default_max_module_size_mb(),
            allow_workspace_read: false,
            allow_workspace_write: false,
            allowed_hosts: Vec::new(),
            security: WasmSecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmSecurityConfig {
    #[serde(default = "default_true")]
    pub require_workspace_relative_tools_dir: bool,
    #[serde(default = "default_true")]
    pub reject_symlink_modules: bool,
    #[serde(default = "default_true")]
    pub strict_host_validation: bool,
    #[serde(default = "default_capability_escalation_mode")]
    pub capability_escalation_mode: String,
    #[serde(default = "default_module_hash_policy")]
    pub module_hash_policy: String,
    #[serde(default)]
    pub module_sha256: HashMap<String, String>,
}

fn default_capability_escalation_mode() -> String {
    "clamp".into()
}
fn default_module_hash_policy() -> String {
    "warn".into()
}

impl Default for WasmSecurityConfig {
    fn default() -> Self {
        Self {
            require_workspace_relative_tools_dir: true,
            reject_symlink_modules: true,
            strict_host_validation: true,
            capability_escalation_mode: default_capability_escalation_mode(),
            module_hash_policy: default_module_hash_policy(),
            module_sha256: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_memory_backend")]
    pub backend: String,
    pub db_path: Option<String>,
    pub qdrant_url: Option<String>,
    pub qdrant_collection: Option<String>,
    pub qdrant_api_key: Option<String>,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

fn default_memory_backend() -> String {
    "sqlite".into()
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: default_memory_backend(),
            db_path: None,
            qdrant_url: None,
            qdrant_collection: None,
            qdrant_api_key: None,
            embedding: EmbeddingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {
    #[serde(default)]
    pub provider: StorageProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageProviderConfig {
    #[serde(default)]
    pub config: StorageProviderInner,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageProviderInner {
    pub provider: Option<String>,
    pub db_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Observability
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservabilityConfig {
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub prometheus_enabled: bool,
    #[serde(default)]
    pub prometheus_port: Option<u16>,
    #[serde(default)]
    pub tracing_enabled: bool,
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_gateway_host")]
    pub host: String,
    #[serde(default = "default_gateway_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub require_pairing: bool,
    #[serde(default)]
    pub allow_public_bind: bool,
    #[serde(default = "default_gateway_session_ttl_secs")]
    pub session_ttl_secs: u64,
    #[serde(default = "default_gateway_max_sessions")]
    pub max_sessions: usize,
    #[serde(default = "default_gateway_webhook_require_nonce")]
    pub webhook_require_nonce: bool,
    #[serde(default = "default_gateway_webhook_max_skew_secs")]
    pub webhook_max_skew_secs: u64,
    #[serde(default = "default_gateway_webhook_nonce_ttl_secs")]
    pub webhook_nonce_ttl_secs: u64,
    #[serde(default = "default_gateway_webhook_signature_algorithms")]
    pub webhook_signature_algorithms: Vec<String>,
    #[serde(default = "default_gateway_webhook_signature_priority")]
    pub webhook_signature_priority: Vec<String>,
    #[serde(default)]
    pub webhook_signature_strict_priority: bool,
    #[serde(default)]
    pub webhook_signing_include_timestamp: bool,
    #[serde(default)]
    pub webhook_signing_require_timestamp: bool,
}

fn default_gateway_host() -> String {
    "127.0.0.1".into()
}
fn default_gateway_port() -> u16 {
    42617
}
fn default_gateway_session_ttl_secs() -> u64 {
    24 * 60 * 60
}
fn default_gateway_max_sessions() -> usize {
    500
}
fn default_gateway_webhook_require_nonce() -> bool {
    false
}
fn default_gateway_webhook_max_skew_secs() -> u64 {
    300
}
fn default_gateway_webhook_nonce_ttl_secs() -> u64 {
    600
}
fn default_gateway_webhook_signature_algorithms() -> Vec<String> {
    vec!["sha256".to_string(), "v1".to_string(), "v0".to_string(), "raw".to_string()]
}
fn default_gateway_webhook_signature_priority() -> Vec<String> {
    vec!["v1".to_string(), "sha256".to_string(), "v0".to_string(), "raw".to_string()]
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_gateway_host(),
            port: default_gateway_port(),
            require_pairing: true,
            allow_public_bind: false,
            session_ttl_secs: default_gateway_session_ttl_secs(),
            max_sessions: default_gateway_max_sessions(),
            webhook_require_nonce: default_gateway_webhook_require_nonce(),
            webhook_max_skew_secs: default_gateway_webhook_max_skew_secs(),
            webhook_nonce_ttl_secs: default_gateway_webhook_nonce_ttl_secs(),
            webhook_signature_algorithms: default_gateway_webhook_signature_algorithms(),
            webhook_signature_priority: default_gateway_webhook_signature_priority(),
            webhook_signature_strict_priority: false,
            webhook_signing_include_timestamp: false,
            webhook_signing_require_timestamp: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Proxy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    #[serde(default)]
    pub enabled: bool,
    pub scope: Option<String>,
    #[serde(default)]
    pub services: Vec<String>,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub all_proxy: Option<String>,
    pub no_proxy: Option<String>,
}

// ---------------------------------------------------------------------------
// Tunnel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunnelConfig {
    #[serde(default)]
    pub enabled: bool,
    pub provider: Option<String>,
    pub auth_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Browser Tool
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_browser_backend")]
    pub backend: String,
    #[serde(default)]
    pub native_headless: bool,
}

fn default_browser_backend() -> String {
    "playwright".into()
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_domains: Vec::new(),
            backend: default_browser_backend(),
            native_headless: false,
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP Request Tool
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_max_response_size")]
    pub max_response_size: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_max_response_size() -> usize {
    1_048_576
}
fn default_timeout_secs() -> u64 {
    30
}

impl Default for HttpRequestConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_domains: Vec::new(),
            max_response_size: default_max_response_size(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Web Fetch Tool
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_max_response_size")]
    pub max_response_size: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_domains: Vec::new(),
            max_response_size: default_max_response_size(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Web Search Tool
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSearchConfig {
    #[serde(default)]
    pub enabled: bool,
    pub provider: Option<String>,
    pub brave_api_key: Option<String>,
    pub max_results: Option<u32>,
    pub timeout_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// Composio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComposioConfig {
    #[serde(default)]
    pub enabled: bool,
    pub api_key: Option<String>,
    pub entity_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    #[serde(default)]
    pub open_skills_enabled: bool,
    pub open_skills_dir: Option<String>,
    pub prompt_injection_mode: Option<String>,
}

// ---------------------------------------------------------------------------
// Query Classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryClassificationConfig {
    #[serde(default)]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeartbeatConfig {
    #[serde(default)]
    pub enabled: bool,
    pub interval_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// Cron
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CronConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub jobs: Vec<CronJobConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CronJobConfig {
    pub name: Option<String>,
    pub schedule: Option<String>,
    pub action: Option<String>,
}

// ---------------------------------------------------------------------------
// Goal Loop
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalLoopConfig {
    #[serde(default)]
    pub enabled: bool,
    pub interval_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: Option<ChannelEntry>,
    #[serde(default)]
    pub discord: Option<ChannelEntry>,
    #[serde(default)]
    pub slack: Option<ChannelEntry>,
    #[serde(default)]
    pub whatsapp: Option<ChannelEntry>,
    #[serde(default)]
    pub matrix: Option<ChannelEntry>,
    #[serde(default)]
    pub lark: Option<ChannelEntry>,
    #[serde(default)]
    pub feishu: Option<ChannelEntry>,
    #[serde(default)]
    pub dingtalk: Option<ChannelEntry>,
    #[serde(default)]
    pub email: Option<ChannelEntry>,
    #[serde(default)]
    pub webhook: Option<ChannelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelEntry {
    #[serde(default)]
    pub enabled: bool,
    pub token: Option<String>,
    pub token_env: Option<String>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Reliability
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
    #[serde(default)]
    pub circuit_breaker_enabled: bool,
    pub circuit_breaker_threshold: Option<u32>,
}

fn default_max_retries() -> u32 {
    3
}
fn default_retry_backoff_ms() -> u64 {
    1000
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_backoff_ms: default_retry_backoff_ms(),
            circuit_breaker_enabled: false,
            circuit_breaker_threshold: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Research Phase
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResearchPhaseConfig {
    #[serde(default)]
    pub enabled: bool,
    pub max_depth: Option<u32>,
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerConfig {
    #[serde(default)]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Cost
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostConfig {
    #[serde(default)]
    pub tracking_enabled: bool,
    pub max_daily_cents: Option<u32>,
    pub alert_threshold_cents: Option<u32>,
}

// ---------------------------------------------------------------------------
// Multimodal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MultimodalConfig {
    #[serde(default)]
    pub vision_enabled: bool,
    #[serde(default)]
    pub audio_enabled: bool,
}

// ---------------------------------------------------------------------------
// Transcription
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranscriptionConfig {
    #[serde(default)]
    pub enabled: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    #[serde(default = "default_identity_name")]
    pub name: String,
    pub bio: Option<String>,
}

fn default_identity_name() -> String {
    "OmniNova".into()
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            name: default_identity_name(),
            bio: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecretsConfig {
    pub store_path: Option<String>,
    #[serde(default)]
    pub encrypt_at_rest: bool,
}

// ---------------------------------------------------------------------------
// Coordination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoordinationConfig {
    #[serde(default)]
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub on_start: Vec<String>,
    #[serde(default)]
    pub on_message: Vec<String>,
    #[serde(default)]
    pub on_tool_call: Vec<String>,
    #[serde(default)]
    pub on_error: Vec<String>,
}

// ---------------------------------------------------------------------------
// Hardware / Peripherals / Robot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwareConfig {
    pub platform: Option<String>,
    #[serde(default)]
    pub gpio_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeripheralsConfig {
    #[serde(default)]
    pub stm32: Option<Stm32Config>,
    #[serde(default)]
    pub rpi_gpio: Option<RpiGpioConfig>,
    #[serde(default)]
    pub arduino: Option<ArduinoConfig>,
    #[serde(default)]
    pub esp32: Option<Esp32Config>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stm32Config {
    pub serial_port: Option<String>,
    pub baud_rate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RpiGpioConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArduinoConfig {
    pub serial_port: Option<String>,
    pub baud_rate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Esp32Config {
    pub serial_port: Option<String>,
    pub baud_rate: Option<u32>,
}

// Robot
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RobotConfig {
    #[serde(default)]
    pub drive: DriveConfig,
    #[serde(default)]
    pub camera: CameraConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub sensors: SensorsConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveConfig {
    #[serde(default = "default_drive_backend")]
    pub backend: String,
    pub ros2_topic: Option<String>,
    pub serial_port: Option<String>,
    #[serde(default = "default_max_speed")]
    pub max_speed: f64,
    #[serde(default = "default_max_rotation")]
    pub max_rotation: f64,
}

fn default_drive_backend() -> String {
    "mock".into()
}
fn default_max_speed() -> f64 {
    0.5
}
fn default_max_rotation() -> f64 {
    1.0
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            backend: default_drive_backend(),
            ros2_topic: None,
            serial_port: None,
            max_speed: default_max_speed(),
            max_rotation: default_max_rotation(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraConfig {
    #[serde(default = "default_camera_device")]
    pub device: String,
    #[serde(default = "default_camera_width")]
    pub width: u32,
    #[serde(default = "default_camera_height")]
    pub height: u32,
    #[serde(default = "default_vision_model")]
    pub vision_model: String,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
}

fn default_camera_device() -> String {
    "/dev/video0".into()
}
fn default_camera_width() -> u32 {
    640
}
fn default_camera_height() -> u32 {
    480
}
fn default_vision_model() -> String {
    "moondream".into()
}
fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            device: default_camera_device(),
            width: default_camera_width(),
            height: default_camera_height(),
            vision_model: default_vision_model(),
            ollama_url: default_ollama_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_mic_device")]
    pub mic_device: String,
    #[serde(default = "default_speaker_device")]
    pub speaker_device: String,
    #[serde(default = "default_whisper_model")]
    pub whisper_model: String,
    pub whisper_path: Option<String>,
    pub piper_path: Option<String>,
    pub piper_voice: Option<String>,
}

fn default_mic_device() -> String {
    "default".into()
}
fn default_speaker_device() -> String {
    "default".into()
}
fn default_whisper_model() -> String {
    "base".into()
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            mic_device: default_mic_device(),
            speaker_device: default_speaker_device(),
            whisper_model: default_whisper_model(),
            whisper_path: None,
            piper_path: None,
            piper_voice: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorsConfig {
    pub lidar_port: Option<String>,
    #[serde(default = "default_lidar_type")]
    pub lidar_type: String,
    #[serde(default)]
    pub motion_pins: Vec<u8>,
    pub ultrasonic_pins: Option<(u8, u8)>,
}

fn default_lidar_type() -> String {
    "mock".into()
}

impl Default for SensorsConfig {
    fn default() -> Self {
        Self {
            lidar_port: None,
            lidar_type: default_lidar_type(),
            motion_pins: Vec::new(),
            ultrasonic_pins: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default = "default_min_obstacle_distance")]
    pub min_obstacle_distance: f64,
    #[serde(default = "default_slow_zone_multiplier")]
    pub slow_zone_multiplier: f64,
    #[serde(default = "default_approach_speed_limit")]
    pub approach_speed_limit: f64,
    pub estop_pin: Option<u8>,
    #[serde(default)]
    pub bump_sensor_pins: Vec<u8>,
}

fn default_min_obstacle_distance() -> f64 {
    0.3
}
fn default_slow_zone_multiplier() -> f64 {
    3.0
}
fn default_approach_speed_limit() -> f64 {
    0.3
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            min_obstacle_distance: default_min_obstacle_distance(),
            slow_zone_multiplier: default_slow_zone_multiplier(),
            approach_speed_limit: default_approach_speed_limit(),
            estop_pin: None,
            bump_sensor_pins: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Agents IPC
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsIpcConfig {
    #[serde(default)]
    pub enabled: bool,
    pub transport: Option<String>,
}
