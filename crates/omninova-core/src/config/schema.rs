use serde::{Deserialize, Serialize};
use serde::Deserializer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Top-level Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// User-selected workspace root. Persisted to config.toml so that the
    /// file/shell/git tools always use the same directory after a restart.
    /// We still need to keep `config_path` skipped because it is a runtime
    /// path computed from the actual on-disk location of the config file.
    #[serde(default)]
    pub workspace_dir: PathBuf,
    #[serde(skip)]
    pub config_path: PathBuf,

    #[serde(alias = "apiKey")]
    pub api_key: Option<String>,
    #[serde(alias = "apiUrl")]
    pub api_url: Option<String>,
    #[serde(alias = "defaultProvider")]
    pub default_provider: Option<String>,
    #[serde(alias = "defaultModel")]
    pub default_model: Option<String>,
    #[serde(default = "default_temperature", alias = "defaultTemperature")]
    pub default_temperature: f64,
    #[serde(alias = "providerApi")]
    pub provider_api: Option<ProviderApiMode>,

    #[serde(default, alias = "modelProviders")]
    pub model_providers: HashMap<String, ModelProviderConfig>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default, alias = "modelRoutes")]
    pub model_routes: Vec<ModelRouteConfig>,
    #[serde(default, alias = "embeddingRoutes")]
    pub embedding_routes: Vec<EmbeddingRouteConfig>,

    #[serde(default)]
    pub provider: ProviderBehaviorConfig,
    #[serde(default)]
    pub provider_runtime: ProviderRuntimeConfig,
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
    pub gateway_public: GatewayPublicConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub tunnel: TunnelConfig,

    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default, alias = "computerUse")]
    pub computer_use: ComputerUseConfig,
    #[serde(default, alias = "httpRequest")]
    pub http_request: HttpRequestConfig,
    #[serde(default, alias = "webFetch")]
    pub web_fetch: WebFetchConfig,
    #[serde(default, alias = "webSearch")]
    pub web_search: WebSearchConfig,
    #[serde(default)]
    pub composio: ComposioConfig,

    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default, alias = "queryClassification")]
    pub query_classification: QueryClassificationConfig,
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub cron: CronConfig,
    #[serde(default, alias = "goalLoop")]
    pub goal_loop: GoalLoopConfig,
    #[serde(default, alias = "channels")]
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

    #[serde(default, deserialize_with = "deserialize_agents_compat")]
    pub agents: HashMap<String, DelegateAgentConfig>,
    #[serde(default, alias = "agentsIpc")]
    pub agents_ipc: AgentsIpcConfig,

    #[serde(default)]
    pub meta: MetaConfig,
    #[serde(default)]
    pub env: EnvConfig,
    #[serde(default)]
    pub wizard: WizardConfig,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub cli: CliConfig,
    #[serde(default)]
    pub update: UpdateConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub acp: AcpConfig,
    #[serde(default)]
    pub media: MediaConfig,
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    #[serde(default, alias = "canvasHost")]
    pub canvas_host: CanvasHostConfig,
    #[serde(default)]
    pub talk: TalkConfig,
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub approvals: ApprovalsConfig,
    #[serde(default)]
    pub messages: MessagesConfig,
    #[serde(default)]
    pub commands: CommandsConfig,
    #[serde(default)]
    pub bindings: Vec<BindingEntry>,
    #[serde(default)]
    pub broadcast: BroadcastConfig,
    #[serde(default)]
    pub agent_defaults_extended: AgentDefaultsExtendedConfig,

    #[serde(alias = "modelSupportVision")]
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
            workspace_dir: PathBuf::new(),
            config_path: omninova_dir.join("config.toml"),
            api_key: None,
            api_url: None,
            default_provider: Some("doubao".into()),
            default_model: Some("doubao-seed-2-0-pro-260215".into()),
            default_temperature: 0.7,
            provider_api: None,
            model_providers: HashMap::new(),
            providers: Vec::new(),
            model_routes: Vec::new(),
            embedding_routes: Vec::new(),
            provider: ProviderBehaviorConfig::default(),
            provider_runtime: ProviderRuntimeConfig::default(),
            agent: AgentConfig::default(),
            autonomy: AutonomyConfig::default(),
            security: SecurityConfig::default(),
            runtime: RuntimeConfig::default(),
            memory: MemoryConfig::default(),
            storage: StorageConfig::default(),
            observability: ObservabilityConfig::default(),
            gateway: GatewayConfig::default(),
            gateway_public: GatewayPublicConfig::default(),
            proxy: ProxyConfig::default(),
            tunnel: TunnelConfig::default(),
            browser: BrowserConfig::default(),
            computer_use: ComputerUseConfig::default(),
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
            meta: MetaConfig::default(),
            env: EnvConfig::default(),
            wizard: WizardConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
            logging: LoggingConfig::default(),
            cli: CliConfig::default(),
            update: UpdateConfig::default(),
            ui: UiConfig::default(),
            auth: AuthConfig::default(),
            acp: AcpConfig::default(),
            media: MediaConfig::default(),
            discovery: DiscoveryConfig::default(),
            canvas_host: CanvasHostConfig::default(),
            talk: TalkConfig::default(),
            web: WebConfig::default(),
            session: SessionConfig::default(),
            approvals: ApprovalsConfig::default(),
            messages: MessagesConfig::default(),
            commands: CommandsConfig::default(),
            bindings: Vec::new(),
            broadcast: BroadcastConfig::default(),
            agent_defaults_extended: AgentDefaultsExtendedConfig::default(),
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
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub transport: ProviderTransportConfig,
    /// Optional authoritative model context-window override.
    #[serde(default)]
    pub context_window_tokens: Option<u64>,
    /// Optional model max-output-token capability override.
    /// This is model metadata, not the per-request generation cap.
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// Optional per-request generation cap sent on outgoing Provider calls.
    /// Distinct from `max_output_tokens` (model capability). `None` or `0`
    /// means the OmniNova product default request policy applies, clamped to
    /// the model maximum when known.
    #[serde(default)]
    pub request_max_output_tokens: Option<u64>,
    /// Optional trusted display tokenizer implementation name. Unknown names
    /// are ignored; this never infers a tokenizer from a model alias.
    #[serde(default)]
    pub exact_tokenizer: Option<String>,
    /// Optional canonical model id used when this profile is an alias/proxy.
    /// This is the only supported way to map an alias to trusted capabilities.
    #[serde(default)]
    pub canonical_model: Option<String>,
    /// Optional explicit provider family, e.g. "anthropic" or "openai".
    /// This never overrides capability resolution by itself; it is metadata.
    #[serde(default)]
    pub provider_family: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider HTTP transport profile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    #[default]
    Auto,
    Http1,
    Http2,
}

impl TransportMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Http1 => "http1",
            Self::Http2 => "http2",
        }
    }
}

/// Per-provider HTTP transport settings. The default is `Auto`, so existing
/// config files without a `[model_providers.*.transport]` table keep the
/// normal reqwest negotiation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProviderTransportConfig {
    #[serde(default)]
    pub mode: TransportMode,
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
// Provider runtime timeouts
// ---------------------------------------------------------------------------

pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_STREAM_IDLE_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 900;

/// Bounded timeouts for provider network calls, configurable under
/// `[provider_runtime]` in config.toml. Missing fields deserialize to 0 and
/// fall back to the defaults; zero and out-of-range values are sanitized to
/// safe bounds at access time so old config files stay valid and a mis-typed
/// value can never produce an unbounded or instant timeout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRuntimeConfig {
    /// Maximum time allowed to establish the provider connection.
    /// Default 30s. Zero → default; clamped to 1..=120.
    #[serde(default)]
    pub connect_timeout_secs: u64,
    /// Maximum continuous period without valid stream activity. The timer is
    /// reset whenever stream data arrives. Default 300s. Zero → default;
    /// clamped to 10..=3600.
    #[serde(default)]
    pub stream_idle_timeout_secs: u64,
    /// Maximum wall-clock duration for one complete model request.
    /// Default 900s. Zero → default; clamped to 60..=86400.
    #[serde(default)]
    pub request_timeout_secs: u64,
}

impl Default for ProviderRuntimeConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            stream_idle_timeout_secs: DEFAULT_STREAM_IDLE_TIMEOUT_SECS,
            request_timeout_secs: DEFAULT_REQUEST_TIMEOUT_SECS,
        }
    }
}

impl ProviderRuntimeConfig {
    pub fn connect_timeout(&self) -> Duration {
        clamp_timeout_secs(
            self.connect_timeout_secs,
            DEFAULT_CONNECT_TIMEOUT_SECS,
            1,
            120,
        )
    }

    pub fn stream_idle_timeout(&self) -> Duration {
        clamp_timeout_secs(
            self.stream_idle_timeout_secs,
            DEFAULT_STREAM_IDLE_TIMEOUT_SECS,
            10,
            3600,
        )
    }

    pub fn request_timeout(&self) -> Duration {
        clamp_timeout_secs(
            self.request_timeout_secs,
            DEFAULT_REQUEST_TIMEOUT_SECS,
            60,
            86_400,
        )
    }
}

fn clamp_timeout_secs(value: u64, default: u64, min: u64, max: u64) -> Duration {
    let effective = if value == 0 { default } else { value.clamp(min, max) };
    Duration::from_secs(effective)
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
    #[serde(default)]
    pub planning: PlanningConfig,
    #[serde(default)]
    pub budget: BudgetConfig,
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
            planning: PlanningConfig::default(),
            budget: BudgetConfig::default(),
        }
    }
}

/// Plan-Execute-Reflect loop configuration (off by default; the agent then
/// runs the plain ReAct tool loop).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Maximum number of steps the planner may emit per plan.
    #[serde(default = "default_max_plan_steps")]
    pub max_plan_steps: usize,
    /// Maximum number of replans triggered by the reflector.
    #[serde(default = "default_max_replans")]
    pub max_replans: usize,
}

fn default_max_plan_steps() -> usize {
    8
}
fn default_max_replans() -> usize {
    2
}

impl Default for PlanningConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_plan_steps: default_max_plan_steps(),
            max_replans: default_max_replans(),
        }
    }
}

/// Hard run budgets. `None` disables a given cap; `max_tool_iterations`
/// remains the per-turn step cap.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BudgetConfig {
    /// Total tokens (input+output as reported by providers) per request.
    #[serde(default)]
    pub max_total_tokens: Option<u64>,
    /// Wall-clock seconds per request.
    #[serde(default)]
    pub max_wall_time_secs: Option<u64>,
    /// Total provider calls per request (spans plan/execute/reflect).
    #[serde(default)]
    pub max_provider_calls: Option<u32>,
}

// ---------------------------------------------------------------------------
// Delegate Agent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DelegateAgentConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    /// Tells the parent when to pick this agent. Shown in the `delegate` tool
    /// catalog. Built-in and markdown subagents always set this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub max_depth: Option<u32>,
    #[serde(default)]
    pub agentic: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub max_iterations: Option<usize>,
    /// Per-agent workspace root. Takes precedence over the global
    /// `Config::workspace_dir` when set. When empty the agent falls back
    /// to the global workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<PathBuf>,
}

/// Resolve the effective workspace directory for an agent using the priority:
///   1. `session_workspace_dir` (session-level override, e.g. from a one-off
///      "use this folder" action in the UI)
///   2. `agent_workspace_dir` (per-agent setting)
///   3. `global_workspace_dir` (Config::workspace_dir)
///   4. Return `None` when none of the above is set.
pub fn resolve_effective_workspace_dir(
    session_workspace_dir: Option<&Path>,
    agent_workspace_dir: Option<&Path>,
    global_workspace_dir: &Path,
) -> Option<PathBuf> {
    session_workspace_dir
        .map(PathBuf::from)
        .or_else(|| agent_workspace_dir.map(PathBuf::from))
        .or_else(|| {
            let g = global_workspace_dir;
            if g.as_os_str().is_empty() || g.components().next().is_none() {
                None
            } else {
                Some(g.to_path_buf())
            }
        })
}

/// Canonical installed-skill store used by install, discovery, status,
/// remove, rollback, and Agent injection.
///
/// `open_skills_dir` when set and non-empty; otherwise `<global workspace>/skills`.
/// Session and per-agent workspace overrides do not change this directory:
/// Setup/UI cannot observe those overrides, so using them would split
/// install writes from runtime reads.
pub fn resolve_configured_skills_dir(config: &Config) -> PathBuf {
    nonempty_open_skills_dir(config).unwrap_or_else(|| config.workspace_dir.join("skills"))
}

/// Skills directory implied by an effective (session/per-agent) workspace.
/// Runtime loading in S1 does **not** use this path unless it happens to
/// equal [`resolve_configured_skills_dir`]. Exposed so tests can detect a split.
pub fn resolve_effective_workspace_skills_dir(
    config: &Config,
    effective_workspace: &Path,
) -> PathBuf {
    nonempty_open_skills_dir(config).unwrap_or_else(|| effective_workspace.join("skills"))
}

fn nonempty_open_skills_dir(config: &Config) -> Option<PathBuf> {
    config
        .skills
        .open_skills_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OmninovalAgentsCompat {
    #[serde(default)]
    pub defaults: Option<AgentDefaultsExtendedConfig>,
    #[serde(default)]
    pub list: Vec<OmninovalAgentEntryCompat>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OmninovalAgentEntryCompat {
    pub id: String,
    #[serde(default)]
    pub model: Option<OmninovalModelRefCompat>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OmninovalModelRefCompat {
    Name(String),
    Detailed(AgentModelConfig),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum AgentsCompatInput {
    DelegateMap(HashMap<String, DelegateAgentConfig>),
    Omninoval(OmninovalAgentsCompat),
}

fn deserialize_agents_compat<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, DelegateAgentConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let input = Option::<AgentsCompatInput>::deserialize(deserializer)?;
    let Some(input) = input else {
        return Ok(HashMap::new());
    };
    let mapped = match input {
        AgentsCompatInput::DelegateMap(map) => map,
        AgentsCompatInput::Omninoval(omninoval) => {
            let mut map = HashMap::new();
            let fallback_provider = omninoval
                .defaults
                .as_ref()
                .and_then(|d| d.model.as_ref())
                .and_then(|m| m.provider.clone());
            let fallback_model = omninoval
                .defaults
                .as_ref()
                .and_then(|d| d.model.as_ref())
                .and_then(|m| m.model.clone());
            for entry in omninoval.list {
                let (provider, model) = match entry.model {
                    Some(OmninovalModelRefCompat::Name(model_name)) => {
                        (fallback_provider.clone(), Some(model_name))
                    }
                    Some(OmninovalModelRefCompat::Detailed(model_cfg)) => {
                        (model_cfg.provider, model_cfg.model)
                    }
                    None => (fallback_provider.clone(), fallback_model.clone()),
                };
                map.insert(
                    entry.id,
                    DelegateAgentConfig {
                        provider,
                        model,
                        system_prompt: None,
                        description: None,
                        max_depth: None,
                        agentic: false,
                        allowed_tools: Vec::new(),
                        max_iterations: None,
                        workspace_dir: None,
                    },
                );
            }
            map
        }
    };
    Ok(mapped)
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
/// `memory_store` is auto-approved alongside `memory_recall`: it only writes to
/// the local memory store, and prompting for approval on every remembered fact
/// makes long-term memory unusable under the default supervised autonomy.
fn default_auto_approve() -> Vec<String> {
    vec![
        "file_read".into(),
        "memory_recall".into(),
        "memory_store".into(),
        "knowledge_search".into(),
        "web_search".into(),
        "web_fetch".into(),
    ]
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
                "shell".into(), "file_write".into(), "office_create".into(), "file_edit".into(),
                "git_operations".into(), "browser".into(), "computer_use".into(),
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
    #[serde(default)]
    pub tool_policy: ToolPolicyConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub workspace_jail: bool,
    #[serde(default = "default_true")]
    pub strip_environment: bool,
    #[serde(default)]
    pub allowed_env_vars: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            workspace_jail: true,
            strip_environment: true,
            allowed_env_vars: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    #[serde(default)]
    pub enabled: bool,
    pub log_file: Option<String>,
    #[serde(default)]
    pub record_arguments: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            log_file: None,
            record_arguments: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
}

impl Default for ToolPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
        }
    }
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
            // Wasm-side reads/writes are still bounded by `workspace_jail`,
            // so defaulting to true lets in-workspace Wasm tools operate;
            // the per-tool resolver remains the definitive jail check.
            allow_workspace_read: true,
            allow_workspace_write: true,
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
    #[serde(default = "default_true")]
    pub search_expand_query: bool,
    #[serde(default = "default_memory_search_recency_weight")]
    pub search_recency_weight: f64,
    #[serde(default = "default_memory_search_half_life_days")]
    pub search_recency_half_life_days: f64,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

fn default_memory_backend() -> String {
    "sqlite".into()
}

fn default_memory_search_recency_weight() -> f64 {
    2.0
}

fn default_memory_search_half_life_days() -> f64 {
    7.0
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: default_memory_backend(),
            db_path: None,
            qdrant_url: None,
            qdrant_collection: None,
            qdrant_api_key: None,
            search_expand_query: default_true(),
            search_recency_weight: default_memory_search_recency_weight(),
            search_recency_half_life_days: default_memory_search_half_life_days(),
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
    /// How long conversation history is retained. `0` disables expiry, keeping
    /// history until the user deletes the session explicitly.
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
    /// DingTalk (企业内部应用机器人) gateway config. Defaults to disabled.
    /// When disabled, the `/api/v1/gateway/dingtalk/events` route is registered
    /// but rejects all requests; no outbound DingTalk calls are made.
    #[serde(default)]
    pub dingtalk: GatewayDingtalkConfig,
    /// WeCom (企业微信智能机器人) gateway config. Defaults to disabled.
    /// When disabled, no WebSocket connection is established.
    #[serde(default)]
    pub wecom: GatewayWecomConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayDingtalkConfig {
    /// Master switch. When `false` (the default), DingTalk routes are
    /// registered but every webhook is rejected and no outbound calls happen.
    #[serde(default)]
    pub enabled: bool,
    /// DingTalk AppKey. Prefer `app_key_env` (or `OMNINOVA_DINGTALK_APP_KEY`
    /// environment variable) for real secrets; the inline value should stay
    /// empty in committed config files.
    #[serde(default)]
    pub app_key: String,
    /// Optional environment variable name holding the AppKey. Resolved after
    /// the inline `app_key` field.
    #[serde(default)]
    pub app_key_env: Option<String>,
    /// DingTalk AppSecret. Strongly prefer the env var path; never commit
    /// a real secret here.
    #[serde(default)]
    pub app_secret: String,
    /// Optional environment variable name holding the AppSecret.
    #[serde(default)]
    pub app_secret_env: Option<String>,
    /// DingTalk robotCode (unique identifier for the enterprise app bot).
    #[serde(default)]
    pub robot_code: String,
    /// Optional environment variable name holding the robotCode.
    #[serde(default)]
    pub robot_code_env: Option<String>,
    /// DingTalk advanced-card template id used by createAndDeliver. An empty
    /// value disables only the interactive menu card, not the Gateway.
    #[serde(default)]
    pub card_template_id: String,
    /// Optional environment variable holding the advanced-card template id.
    #[serde(default)]
    pub card_template_id_env: Option<String>,
    /// DingTalk connection mode. HTTP keeps the stable webhook + text bot
    /// flow; Stream additionally enables OmniNova advanced interactive cards.
    #[serde(default)]
    pub transport_mode: DingtalkTransportMode,
    /// HTTP path that DingTalk servers POST events to. Exposed for visibility
    /// only; the actual route is registered as `/webhook/dingtalk` and the
    /// public callback URL is `/api/v1/gateway/dingtalk/events`.
    #[serde(default = "default_gateway_dingtalk_webhook_path")]
    pub webhook_path: String,
    /// Outbound delivery mode:
    /// - `disabled` (default): never send replies back to DingTalk.
    /// - `session_webhook`: reply via the sessionWebhook URL supplied in the
    ///   inbound payload (the standard app-bot flow).
    /// - `mock`: log only, no network call.
    #[serde(default = "default_gateway_dingtalk_outbound_mode")]
    pub outbound_mode: String,
    /// When `true` (the default), DingTalk log lines redact sensitive
    /// fields (app_secret, access_token, sessionWebhook, senderStaffId,
    /// conversationId, messageId).
    #[serde(default = "default_true")]
    pub redact_sensitive_logs: bool,
}

impl Default for GatewayDingtalkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_key: String::new(),
            app_key_env: None,
            app_secret: String::new(),
            app_secret_env: None,
            robot_code: String::new(),
            robot_code_env: None,
            card_template_id: String::new(),
            card_template_id_env: None,
            transport_mode: DingtalkTransportMode::default(),
            webhook_path: default_gateway_dingtalk_webhook_path(),
            outbound_mode: default_gateway_dingtalk_outbound_mode(),
            redact_sensitive_logs: true,
        }
    }
}

fn default_gateway_dingtalk_webhook_path() -> String {
    "/api/v1/gateway/dingtalk/events".into()
}

fn default_gateway_dingtalk_outbound_mode() -> String {
    "session_webhook".into()
}

/// WeCom (企业微信) Smart Bot config. Uses WebSocket long-connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WecomTransportMode {
    #[default]
    LongConnection,
    HttpCallback,
}

impl WecomTransportMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LongConnection => "long_connection",
            Self::HttpCallback => "http_callback",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayWecomConfig {
    /// Master switch. When `false` (the default), no WeCom WebSocket connection is established.
    #[serde(default)]
    pub enabled: bool,
    /// Explicit inbound transport. Legacy configs remain on long connection.
    #[serde(default)]
    pub transport_mode: WecomTransportMode,
    /// WeCom Bot ID (智能机器人 BotID).
    #[serde(default)]
    pub bot_id: String,
    /// Optional environment variable name holding the Bot ID.
    #[serde(default)]
    pub bot_id_env: Option<String>,
    /// WeCom Long-connection Secret (长连接专用密钥).
    #[serde(default)]
    pub secret: String,
    /// Optional environment variable name holding the Secret.
    #[serde(default)]
    pub secret_env: Option<String>,
    /// Token configured in the Smart Bot URL callback settings.
    #[serde(default)]
    pub callback_token: String,
    #[serde(default)]
    pub callback_token_env: Option<String>,
    /// 43-character EncodingAESKey for encrypted HTTP callbacks.
    #[serde(default)]
    pub encoding_aes_key: String,
    #[serde(default)]
    pub encoding_aes_key_env: Option<String>,
    /// ReceiveId for Bot Webhook callback verification.
    /// Per WeCom official docs: this field is OPTIONAL for Bot Webhook.
    /// If configured in WeCom后台, must match the value in the decrypted frame.
    #[serde(default)]
    pub receive_id: String,
    #[serde(default)]
    pub receive_id_env: Option<String>,
}

impl Default for GatewayWecomConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport_mode: WecomTransportMode::LongConnection,
            bot_id: String::new(),
            bot_id_env: None,
            secret: String::new(),
            secret_env: None,
            callback_token: String::new(),
            callback_token_env: None,
            encoding_aes_key: String::new(),
            encoding_aes_key_env: None,
            receive_id: String::new(),
            receive_id_env: None,
        }
    }
}

fn default_gateway_host() -> String {
    "127.0.0.1".into()
}
fn default_gateway_port() -> u16 {
    10809
}
/// 30 days. The previous 24-hour default silently emptied history overnight,
/// which users experienced as memory loss rather than as expiry.
fn default_gateway_session_ttl_secs() -> u64 {
    30 * 24 * 60 * 60
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
            dingtalk: GatewayDingtalkConfig::default(),
            wecom: GatewayWecomConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayPublicMode {
    QuickTunnel,
    NamedCloudflareTunnel,
    ExternalPublicUrl,
}

impl Default for GatewayPublicMode {
    fn default() -> Self {
        Self::ExternalPublicUrl
    }
}

impl GatewayPublicMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QuickTunnel => "quick_tunnel",
            Self::NamedCloudflareTunnel => "named_cloudflare_tunnel",
            Self::ExternalPublicUrl => "external_public_url",
        }
    }
}

/// Stable public ingress configuration. This section contains locations and
/// hostnames only; tunnel credentials and webhook secrets never belong here.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayPublicConfig {
    #[serde(default)]
    pub mode: GatewayPublicMode,
    #[serde(default)]
    pub public_webhook_base_url: Option<String>,
    #[serde(default)]
    pub cloudflared_path: Option<PathBuf>,
    #[serde(default)]
    pub named_tunnel_name: Option<String>,
    #[serde(default)]
    pub named_tunnel_hostname: Option<String>,
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
    #[serde(default)]
    pub attach_only: bool,
    pub cdp_url: Option<String>,
    /// Trusted local override for the Chrome-compatible executable used by
    /// Managed Browser. This is runtime configuration, never a tool argument.
    #[serde(default)]
    pub executable_path: Option<PathBuf>,
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
            attach_only: false,
            cdp_url: None,
            executable_path: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Computer Use (OS desktop control)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerUseConfig {
    /// Off by default. Enabling still requires approval for click/type/press.
    #[serde(default)]
    pub enabled: bool,
    /// Default `*` = operate any foreground app.
    /// Empty = observe-only. Otherwise a name whitelist.
    #[serde(default = "default_computer_use_allowed_apps")]
    pub allowed_apps: Vec<String>,
    #[serde(default = "default_true")]
    pub require_screenshot_before_click: bool,
    /// Mouse/keyboard actions one agent run may perform. `0` = unlimited, which
    /// is what a long unattended desktop task needs.
    #[serde(default = "default_computer_use_max_actions_per_turn")]
    pub max_actions_per_turn: u32,
    /// Process-wide hourly rate limit on mouse/keyboard actions. `0` = unlimited.
    #[serde(default = "default_computer_use_max_actions_per_hour")]
    pub max_actions_per_hour: u32,
    /// Raise the ReAct iteration cap when this tool is in the session.
    #[serde(default = "default_computer_use_max_tool_iterations")]
    pub max_tool_iterations: usize,
    #[serde(default = "default_desktop_vision_max_dimension_px")]
    pub max_dimension_px: u32,
    #[serde(default = "default_computer_use_thrash_soft")]
    pub thrash_soft_limit: u32,
    #[serde(default = "default_computer_use_thrash_hard")]
    pub thrash_hard_limit: u32,
    #[serde(default = "default_computer_use_max_snapshot_nodes")]
    pub max_snapshot_nodes: usize,
}

fn default_computer_use_allowed_apps() -> Vec<String> {
    vec!["*".into()]
}

/// Unlimited by default: a desktop task runs until it is done, and the ReAct
/// iteration cap plus E-Stop are what bound it. A small number here silently
/// froze long runs halfway through.
fn default_computer_use_max_actions_per_turn() -> u32 {
    0
}
fn default_computer_use_max_actions_per_hour() -> u32 {
    0
}
fn default_computer_use_max_tool_iterations() -> usize {
    200
}
fn default_computer_use_thrash_soft() -> u32 {
    3
}
fn default_computer_use_thrash_hard() -> u32 {
    5
}
fn default_computer_use_max_snapshot_nodes() -> usize {
    80
}

impl ComputerUseConfig {
    pub fn allowlist_allows_all(allowed_apps: &[String]) -> bool {
        allowed_apps.iter().any(|app| {
            let trimmed = app.trim();
            trimmed == "*" || trimmed.eq_ignore_ascii_case("all")
        })
    }

    pub fn allows_all_apps(&self) -> bool {
        Self::allowlist_allows_all(&self.allowed_apps)
    }
}

impl Default for ComputerUseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_apps: default_computer_use_allowed_apps(),
            require_screenshot_before_click: true,
            max_actions_per_turn: default_computer_use_max_actions_per_turn(),
            max_actions_per_hour: default_computer_use_max_actions_per_hour(),
            max_tool_iterations: default_computer_use_max_tool_iterations(),
            max_dimension_px: default_desktop_vision_max_dimension_px(),
            thrash_soft_limit: default_computer_use_thrash_soft(),
            thrash_hard_limit: default_computer_use_thrash_hard(),
            max_snapshot_nodes: default_computer_use_max_snapshot_nodes(),
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
    #[serde(default = "default_true")]
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
            enabled: true,
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

/// Source of truth for whether workspace skills are injected into the Agent.
/// UI and serde defaults must use this constant so an unsaved form cannot
/// disagree with Runtime.
pub const DEFAULT_OPEN_SKILLS_ENABLED: bool = true;

fn default_open_skills_enabled() -> bool {
    DEFAULT_OPEN_SKILLS_ENABLED
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    #[serde(default = "default_open_skills_enabled")]
    pub open_skills_enabled: bool,
    pub open_skills_dir: Option<String>,
    pub prompt_injection_mode: Option<String>,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            open_skills_enabled: DEFAULT_OPEN_SKILLS_ENABLED,
            open_skills_dir: None,
            prompt_injection_mode: None,
        }
    }
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
    pub google_chat: Option<ChannelEntry>,
    #[serde(default)]
    pub signal: Option<ChannelEntry>,
    #[serde(default)]
    pub bluebubbles: Option<ChannelEntry>,
    #[serde(default)]
    pub imessage: Option<ChannelEntry>,
    #[serde(default)]
    pub irc: Option<ChannelEntry>,
    #[serde(default)]
    pub msteams: Option<ChannelEntry>,
    #[serde(default)]
    pub matrix: Option<ChannelEntry>,
    #[serde(default)]
    pub feishu: Option<ChannelEntry>,
    #[serde(default)]
    pub line: Option<ChannelEntry>,
    #[serde(default)]
    pub mattermost: Option<ChannelEntry>,
    #[serde(default)]
    pub nextcloud_talk: Option<ChannelEntry>,
    #[serde(default)]
    pub nostr: Option<ChannelEntry>,
    #[serde(default)]
    pub synology_chat: Option<ChannelEntry>,
    #[serde(default)]
    pub tlon: Option<ChannelEntry>,
    #[serde(default)]
    pub twitch: Option<ChannelEntry>,
    #[serde(default)]
    pub wechat: Option<ChannelEntry>,
    #[serde(default)]
    pub wecom: Option<ChannelEntry>,
    #[serde(default)]
    pub zalo: Option<ChannelEntry>,
    #[serde(default)]
    pub zalo_personal: Option<ChannelEntry>,
    #[serde(default)]
    pub lark: Option<ChannelEntry>,
    #[serde(default)]
    pub dingtalk: Option<ChannelEntry>,
    #[serde(default)]
    pub email: Option<ChannelEntry>,
    #[serde(default)]
    pub webhook: Option<ChannelEntry>,
    #[serde(default)]
    pub webchat: Option<ChannelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelEntry {
    #[serde(default)]
    pub enabled: bool,
    pub token: Option<String>,
    pub token_env: Option<String>,
    /// Security mode: "dev" (no verification), "token" (verify verification_token), "encrypted" (decrypt payload)
    #[serde(default)]
    pub security_mode: Option<String>,
    /// Verification token for webhook security (used when security_mode=token or encrypted)
    pub verification_token: Option<String>,
    /// Env var name for verification_token (e.g., "FEISHU_VERIFICATION_TOKEN")
    pub verification_token_env: Option<String>,
    /// Encryption key for encrypted events (used when security_mode=encrypted)
    pub encrypt_key: Option<String>,
    /// Env var name for encrypt_key (e.g., "FEISHU_ENCRYPT_KEY")
    pub encrypt_key_env: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
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

fn default_desktop_vision_max_dimension_px() -> u32 {
    1280
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalConfig {
    #[serde(default)]
    pub vision_enabled: bool,
    #[serde(default)]
    pub audio_enabled: bool,
    /// 桌面视觉监控：发送消息时截取主屏幕并传给支持视觉的模型。
    #[serde(default)]
    pub desktop_vision_enabled: bool,
    #[serde(default = "default_desktop_vision_max_dimension_px")]
    pub desktop_vision_max_dimension_px: u32,
}

impl Default for MultimodalConfig {
    fn default() -> Self {
        Self {
            vision_enabled: false,
            audio_enabled: false,
            desktop_vision_enabled: false,
            desktop_vision_max_dimension_px: default_desktop_vision_max_dimension_px(),
        }
    }
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

// ---------------------------------------------------------------------------
// omninoval Compatibility Configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetaConfig {
    pub last_touched_version: Option<String>,
    pub last_touched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnvConfig {
    pub shell_env: Option<ShellEnvConfig>,
    #[serde(default)]
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShellEnvConfig {
    #[serde(default)]
    pub enabled: bool,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WizardConfig {
    pub last_run_at: Option<String>,
    pub last_run_version: Option<String>,
    pub last_run_commit: Option<String>,
    pub last_run_command: Option<String>,
    pub last_run_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub flags: Vec<String>,
    pub stuck_session_warn_ms: Option<u64>,
    pub otel: Option<OtelConfig>,
    pub cache_trace: Option<CacheTraceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OtelConfig {
    #[serde(default)]
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub protocol: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub service_name: Option<String>,
    #[serde(default)]
    pub traces: bool,
    #[serde(default)]
    pub metrics: bool,
    #[serde(default)]
    pub logs: bool,
    pub sample_rate: Option<f64>,
    pub flush_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CacheTraceConfig {
    #[serde(default)]
    pub enabled: bool,
    pub file_path: Option<String>,
    #[serde(default)]
    pub include_messages: bool,
    #[serde(default)]
    pub include_prompt: bool,
    #[serde(default)]
    pub include_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    pub level: Option<String>,
    pub file: Option<String>,
    pub max_file_bytes: Option<u64>,
    pub console_level: Option<String>,
    pub console_style: Option<String>,
    pub redact_sensitive: Option<String>,
    #[serde(default)]
    pub redact_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CliConfig {
    pub banner: Option<CliBannerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CliBannerConfig {
    pub tagline_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfig {
    pub channel: Option<String>,
    #[serde(default)]
    pub check_on_start: bool,
    pub auto: Option<UpdateAutoConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAutoConfig {
    #[serde(default)]
    pub enabled: bool,
    pub stable_delay_hours: Option<u32>,
    pub stable_jitter_hours: Option<u32>,
    pub beta_check_interval_hours: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub seam_color: Option<String>,
    pub assistant: Option<UiAssistantConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiAssistantConfig {
    pub name: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(default)]
    pub profiles: HashMap<String, AuthProfileConfig>,
    #[serde(default)]
    pub order: HashMap<String, Vec<String>>,
    pub cooldowns: Option<AuthCooldownsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthProfileConfig {
    pub provider: Option<String>,
    pub mode: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthCooldownsConfig {
    pub billing_backoff_hours: Option<u32>,
    #[serde(default)]
    pub billing_backoff_hours_by_provider: HashMap<String, u32>,
    pub billing_max_hours: Option<u32>,
    pub failure_window_hours: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AcpConfig {
    #[serde(default)]
    pub enabled: bool,
    pub dispatch: Option<AcpDispatchConfig>,
    pub backend: Option<String>,
    pub default_agent: Option<String>,
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    pub max_concurrent_sessions: Option<u32>,
    pub stream: Option<AcpStreamConfig>,
    pub runtime: Option<AcpRuntimeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AcpDispatchConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AcpStreamConfig {
    pub coalesce_idle_ms: Option<u64>,
    pub max_chunk_chars: Option<u32>,
    #[serde(default)]
    pub repeat_suppression: bool,
    pub delivery_mode: Option<String>,
    pub hidden_boundary_separator: Option<String>,
    pub max_output_chars: Option<u32>,
    pub max_session_update_chars: Option<u32>,
    #[serde(default)]
    pub tag_visibility: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AcpRuntimeConfig {
    pub ttl_minutes: Option<u32>,
    pub install_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaConfig {
    #[serde(default)]
    pub preserve_filenames: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryConfig {
    pub wide_area: Option<DiscoveryWideAreaConfig>,
    pub mdns: Option<DiscoveryMdnsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryWideAreaConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryMdnsConfig {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CanvasHostConfig {
    #[serde(default)]
    pub enabled: bool,
    pub root: Option<String>,
    pub port: Option<u16>,
    #[serde(default)]
    pub live_reload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TalkConfig {
    pub provider: Option<String>,
    #[serde(default)]
    pub providers: HashMap<String, TalkProviderConfig>,
    pub voice_id: Option<String>,
    #[serde(default)]
    pub voice_aliases: HashMap<String, String>,
    pub model_id: Option<String>,
    pub output_format: Option<String>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub interrupt_on_speech: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TalkProviderConfig {
    pub voice_id: Option<String>,
    #[serde(default)]
    pub voice_aliases: HashMap<String, String>,
    pub model_id: Option<String>,
    pub output_format: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebConfig {
    #[serde(default)]
    pub enabled: bool,
    pub heartbeat_seconds: Option<u32>,
    pub reconnect: Option<WebReconnectConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebReconnectConfig {
    pub initial_ms: Option<u64>,
    pub max_ms: Option<u64>,
    pub factor: Option<f64>,
    pub jitter: Option<f64>,
    pub max_attempts: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub retention: Option<String>,
    pub max_concurrent: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub mode: Option<String>,
    #[serde(default = "default_auto_approve")]
    pub auto_approve: Vec<String>,
    #[serde(default = "default_require_approval_tools")]
    pub require_approval: Vec<String>,
    /// `blocking` waits in-process (desktop). `deferred` ends the turn and resumes after approve.
    /// When omitted, IM channels default to deferred and others to blocking.
    pub wait_mode: Option<String>,
}

fn default_require_approval_tools() -> Vec<String> {
    vec![
        "shell".into(),
        "file_write".into(),
        "file_edit".into(),
        "git_operations".into(),
        "browser".into(),
        "computer_use".into(),
        "http_request".into(),
    ]
}

impl Default for ApprovalsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: Some("supervised".into()),
            auto_approve: default_auto_approve(),
            require_approval: default_require_approval_tools(),
            wait_mode: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MessagesConfig {
    pub max_history: Option<u32>,
    pub retention: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommandsConfig {
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub forbidden: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BindingEntry {
    pub agent_id: Option<String>,
    pub comment: Option<String>,
    #[serde(rename = "match")]
    pub match_rule: Option<BindingMatchConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BindingMatchConfig {
    pub channel: Option<String>,
    pub account_id: Option<String>,
    pub peer: Option<BindingPeerConfig>,
    pub guild_id: Option<String>,
    pub team_id: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BindingPeerConfig {
    pub kind: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastConfig {
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefaultsExtendedConfig {
    pub model: Option<AgentModelConfig>,
    pub image_model: Option<AgentModelConfig>,
    pub pdf_model: Option<AgentModelConfig>,
    pub pdf_max_bytes_mb: Option<u32>,
    pub pdf_max_pages: Option<u32>,
    #[serde(default)]
    pub models: HashMap<String, ModelAliasConfig>,
    pub workspace: Option<String>,
    pub repo_root: Option<String>,
    #[serde(default)]
    pub skip_bootstrap: bool,
    pub bootstrap_max_chars: Option<u32>,
    pub bootstrap_total_max_chars: Option<u32>,
    pub bootstrap_prompt_truncation_warning: Option<String>,
    pub user_timezone: Option<String>,
    pub time_format: Option<String>,
    pub envelope_timezone: Option<String>,
    pub envelope_timestamp: Option<String>,
    pub envelope_elapsed: Option<String>,
    pub context_tokens: Option<u32>,
    pub context_pruning: Option<ContextPruningConfig>,
    pub compaction: Option<CompactionConfig>,
    pub thinking_default: Option<String>,
    pub verbose_default: Option<String>,
    pub elevated_default: Option<String>,
    pub block_streaming_default: Option<String>,
    pub block_streaming_break: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub media_max_mb: Option<f64>,
    pub image_max_dimension_px: Option<u32>,
    pub typing_interval_seconds: Option<u32>,
    pub typing_mode: Option<String>,
    pub max_concurrent: Option<u32>,
    pub subagents: Option<SubagentsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelAliasConfig {
    pub alias: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
    pub streaming: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextPruningConfig {
    pub mode: Option<String>,
    pub ttl: Option<String>,
    pub keep_last_assistants: Option<u32>,
    pub soft_trim_ratio: Option<f64>,
    pub hard_clear_ratio: Option<f64>,
    pub min_prunable_tool_chars: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfig {
    pub mode: Option<String>,
    pub reserve_tokens: Option<u32>,
    pub keep_recent_tokens: Option<u32>,
    pub reserve_tokens_floor: Option<u32>,
    pub max_history_share: Option<f64>,
    pub identifier_policy: Option<String>,
    pub identifier_instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubagentsConfig {
    pub max_concurrent: Option<u32>,
    pub max_spawn_depth: Option<u32>,
    pub max_children_per_agent: Option<u32>,
    pub archive_after_minutes: Option<u32>,
    pub model: Option<AgentModelConfig>,
    pub thinking: Option<String>,
    pub run_timeout_seconds: Option<u32>,
    pub announce_timeout_ms: Option<u64>,
    /// Directory of `*.md` subagent definitions. Defaults to
    /// `{workspace}/.omninova/agents`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definitions_dir: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_runtime_defaults_apply_when_fields_absent() {
        // Old config files have no [provider_runtime] table: serde defaults
        // fill 0s and the accessors fall back to the safe defaults.
        let cfg = ProviderRuntimeConfig::default();
        assert_eq!(cfg.connect_timeout(), Duration::from_secs(30));
        assert_eq!(cfg.stream_idle_timeout(), Duration::from_secs(300));
        assert_eq!(cfg.request_timeout(), Duration::from_secs(900));

        let zeroed = ProviderRuntimeConfig {
            connect_timeout_secs: 0,
            stream_idle_timeout_secs: 0,
            request_timeout_secs: 0,
        };
        assert_eq!(zeroed.connect_timeout(), Duration::from_secs(30));
        assert_eq!(zeroed.stream_idle_timeout(), Duration::from_secs(300));
        assert_eq!(zeroed.request_timeout(), Duration::from_secs(900));
    }

    #[test]
    fn provider_runtime_loads_custom_values_from_partial_toml() {
        let loaded: Config = toml::from_str(
            r#"
[provider_runtime]
connect_timeout_secs = 12
stream_idle_timeout_secs = 240
request_timeout_secs = 1800
"#,
        )
        .expect("partial config with provider_runtime must load");
        assert_eq!(loaded.provider_runtime.connect_timeout(), Duration::from_secs(12));
        assert_eq!(
            loaded.provider_runtime.stream_idle_timeout(),
            Duration::from_secs(240)
        );
        assert_eq!(loaded.provider_runtime.request_timeout(), Duration::from_secs(1800));
    }

    #[test]
    fn provider_runtime_clamps_out_of_range_values() {
        let extreme = ProviderRuntimeConfig {
            connect_timeout_secs: 9_999_999,
            stream_idle_timeout_secs: 9_999_999,
            request_timeout_secs: 1,
        };
        assert_eq!(extreme.connect_timeout(), Duration::from_secs(120));
        assert_eq!(extreme.stream_idle_timeout(), Duration::from_secs(3600));
        // request min bound is 60s.
        assert_eq!(extreme.request_timeout(), Duration::from_secs(60));
    }

    #[test]
    fn resolve_effective_workspace_dir_session_wins() {
        let global = PathBuf::from("C:\\global");
        let agent = PathBuf::from("C:\\agent");
        let session = PathBuf::from("C:\\session");

        let result = resolve_effective_workspace_dir(
            Some(&session),
            Some(&agent),
            &global,
        );
        assert_eq!(result, Some(PathBuf::from("C:\\session")));
    }

    #[test]
    fn resolve_effective_workspace_dir_agent_wins_over_global() {
        let global = PathBuf::from("C:\\global");
        let agent = PathBuf::from("C:\\agent");

        let result = resolve_effective_workspace_dir(None, Some(&agent), &global);
        assert_eq!(result, Some(PathBuf::from("C:\\agent")));
    }

    #[test]
    fn resolve_effective_workspace_dir_falls_back_to_global() {
        let global = PathBuf::from("C:\\global");

        let result = resolve_effective_workspace_dir(None, None, &global);
        assert_eq!(result, Some(PathBuf::from("C:\\global")));
    }

    #[test]
    fn resolve_effective_workspace_dir_none_returns_none() {
        let result = resolve_effective_workspace_dir(None, None, &PathBuf::new());
        assert_eq!(result, None);
    }

    #[test]
    fn delegate_agent_config_serde_with_workspace_dir() {
        let toml = r#"
provider = "openai"
workspace_dir = "C:\\agent-workspace"
"#;
        let cfg: DelegateAgentConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.workspace_dir, Some(PathBuf::from("C:\\agent-workspace")));
        assert_eq!(cfg.provider.as_deref(), Some("openai"));

        let serialized = toml::to_string_pretty(&cfg).unwrap();
        assert!(serialized.contains("workspace_dir"));
    }

    #[test]
    fn delegate_agent_config_workspace_dir_optional() {
        let toml = "provider = \"openai\"";
        let cfg: DelegateAgentConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.workspace_dir, None);
    }

    #[test]
    fn gateway_public_defaults_keep_legacy_config_compatible() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(
            cfg.gateway_public.mode,
            GatewayPublicMode::ExternalPublicUrl
        );
        assert!(cfg.gateway_public.public_webhook_base_url.is_none());
    }

    #[test]
    fn gateway_public_round_trips_without_secrets() {
        let cfg = GatewayPublicConfig {
            mode: GatewayPublicMode::NamedCloudflareTunnel,
            public_webhook_base_url: Some("https://gateway.example.com".to_string()),
            cloudflared_path: Some(PathBuf::from("C:\\tools\\cloudflared.exe")),
            named_tunnel_name: Some("omninova".to_string()),
            named_tunnel_hostname: Some("gateway.example.com".to_string()),
        };
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let restored: GatewayPublicConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(restored.mode, GatewayPublicMode::NamedCloudflareTunnel);
        assert_eq!(
            restored.public_webhook_base_url.as_deref(),
            Some("https://gateway.example.com")
        );
        assert!(!serialized.to_ascii_lowercase().contains("secret"));
        assert!(!serialized.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn legacy_dingtalk_config_defaults_card_template_to_disabled() {
        let cfg: GatewayDingtalkConfig = toml::from_str(
            r#"
enabled = true
outbound_mode = "session_webhook"
"#,
        )
        .unwrap();
        assert!(cfg.card_template_id.is_empty());
        assert!(cfg.card_template_id_env.is_none());
        assert_eq!(cfg.transport_mode, DingtalkTransportMode::Http);
    }

    #[test]
    fn dingtalk_card_template_round_trips() {
        let mut cfg = GatewayDingtalkConfig::default();
        cfg.card_template_id = "template-id".to_string();
        cfg.card_template_id_env = Some("OMNINOVA_DINGTALK_CARD_TEMPLATE_ID".to_string());
        cfg.transport_mode = DingtalkTransportMode::Stream;
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let restored: GatewayDingtalkConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(restored.card_template_id, "template-id");
        assert_eq!(
            restored.card_template_id_env.as_deref(),
            Some("OMNINOVA_DINGTALK_CARD_TEMPLATE_ID")
        );
        assert_eq!(restored.transport_mode, DingtalkTransportMode::Stream);
    }

    #[test]
    fn dingtalk_transport_mode_preserves_template_when_switching_to_http() {
        let mut cfg = GatewayDingtalkConfig::default();
        cfg.card_template_id = "template-id".to_string();
        cfg.transport_mode = DingtalkTransportMode::Stream;
        cfg.transport_mode = DingtalkTransportMode::Http;

        let restored: GatewayDingtalkConfig =
            toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(restored.transport_mode, DingtalkTransportMode::Http);
        assert_eq!(restored.card_template_id, "template-id");
    }

    #[test]
    fn provider_transport_defaults_to_auto_when_absent() {
        let cfg: Config = toml::from_str(
            r#"
[model_providers.custom_compatible]
base_url = "https://api.example.com/v1"
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.model_providers["custom_compatible"].transport.mode,
            TransportMode::Auto
        );
    }

    #[test]
    fn provider_transport_parses_explicit_modes() {
        for (mode, expected) in [
            ("auto", TransportMode::Auto),
            ("http1", TransportMode::Http1),
            ("http2", TransportMode::Http2),
        ] {
            let cfg: Config = toml::from_str(&format!(
                r#"
[model_providers.custom_compatible]
base_url = "https://api.example.com/v1"

[model_providers.custom_compatible.transport]
mode = "{mode}"
"#
            ))
            .unwrap();
            assert_eq!(
                cfg.model_providers["custom_compatible"].transport.mode,
                expected
            );
        }
    }

    #[test]
    fn provider_transport_invalid_mode_is_a_config_error() {
        let result = toml::from_str::<Config>(
            r#"
[model_providers.custom_compatible]
base_url = "https://api.example.com/v1"

[model_providers.custom_compatible.transport]
mode = "http10"
"#,
        );
        assert!(result.is_err(), "invalid transport mode must fail config loading");
    }

    #[test]
    fn r21_h_request_max_output_tokens_parses_and_roundtrips() {
        let cfg: Config = toml::from_str(
            r#"
[model_providers.deepseek]
request_max_output_tokens = 32000
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.model_providers["deepseek"].request_max_output_tokens,
            Some(32_000)
        );
        assert_eq!(cfg.model_providers["deepseek"].max_output_tokens, None);
        let restored: Config = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(
            restored.model_providers["deepseek"].request_max_output_tokens,
            Some(32_000)
        );
    }

    #[test]
    fn r21_request_max_output_tokens_absent_stays_none() {
        let cfg: Config = toml::from_str(
            r#"
[model_providers.deepseek]
base_url = "https://api.deepseek.com"
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.model_providers["deepseek"].request_max_output_tokens,
            None
        );
    }

    #[test]
    fn computer_use_and_scheduler_phase2_defaults() {
        let cfg = Config::default();
        assert!(!cfg.computer_use.enabled);
        assert!(cfg.computer_use.allows_all_apps());
        // 0 = unlimited, so a desktop task is not cut off mid-way.
        assert_eq!(cfg.computer_use.max_actions_per_turn, 0);
        assert_eq!(cfg.computer_use.max_actions_per_hour, 0);
        assert!(cfg.computer_use.max_tool_iterations >= 200);
        assert_eq!(cfg.computer_use.thrash_soft_limit, 3);
        assert_eq!(cfg.computer_use.thrash_hard_limit, 5);
        assert_eq!(cfg.computer_use.max_snapshot_nodes, 80);
        assert!(cfg.scheduler.enabled);

        let loaded: Config = toml::from_str("[computer_use]\nenabled = false\n").unwrap();
        assert!(loaded.scheduler.enabled);
        assert!(loaded.computer_use.allows_all_apps());
        assert_eq!(loaded.computer_use.thrash_hard_limit, 5);

        let observe_only: Config =
            toml::from_str("[computer_use]\nenabled = true\nallowed_apps = []\n").unwrap();
        assert!(!observe_only.computer_use.allows_all_apps());
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DingtalkTransportMode {
    #[default]
    Http,
    Stream,
}

impl DingtalkTransportMode {
    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "http" => Some(Self::Http),
            "stream" => Some(Self::Stream),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Stream => "stream",
        }
    }
}
