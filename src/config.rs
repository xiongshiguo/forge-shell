use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 熔炉配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// AI 后端配置
    #[serde(default)]
    pub ai: AiConfig,
    /// 界面配置
    #[serde(default)]
    pub ui: UiConfig,
    /// 引擎配置
    #[serde(default)]
    pub engine: EngineConfig,
    /// 社区配置
    #[serde(default)]
    pub community: CommunityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ai: AiConfig::default(),
            ui: UiConfig::default(),
            engine: EngineConfig::default(),
            community: CommunityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// 默认模型
    #[serde(default = "default_model")]
    pub default_model: String,
    /// 简单任务模型（快速、便宜）
    #[serde(default = "default_flash_model")]
    pub flash_model: String,
    /// 用户模型偏好: auto(智能) / pro / flash / local
    #[serde(default = "default_model_pref")]
    pub model_preference: String,
    /// Ollama 本地模型端点
    #[serde(default = "default_ollama_base")]
    pub ollama_base: String,
    /// API 地址
    #[serde(default = "default_api_base")]
    pub api_base: String,
    /// API Key（从环境变量 DEEPSEEK_API_KEY 读取，优先于此处）
    #[serde(default)]
    pub api_key: String,
    /// 最大 Token 数
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// 温度
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// 请求超时秒数
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_model_pref() -> String { "auto".into() }
fn default_ollama_base() -> String { String::new() }

fn default_model() -> String {
    "deepseek-v4-pro".into()
}
fn default_flash_model() -> String {
    "deepseek-v4-flash".into()
}
fn default_api_base() -> String {
    "https://api.deepseek.com".into()
}
fn default_max_tokens() -> u32 {
    128000
}
fn default_temperature() -> f32 {
    0.0
}
fn default_timeout() -> u64 {
    180
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            default_model: default_model(),
            flash_model: default_flash_model(),
            model_preference: default_model_pref(),
            ollama_base: default_ollama_base(),
            api_base: default_api_base(),
            api_key: String::new(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            timeout_secs: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// 默认模式 (interactive/quick)
    #[serde(default = "default_mode")]
    pub default_mode: String,
    /// 是否启用动画
    #[serde(default = "default_true")]
    pub animation: bool,
    /// 滚动缓冲区行数
    #[serde(default = "default_scrollback")]
    pub scrollback: usize,
}

fn default_mode() -> String {
    "interactive".into()
}
fn default_true() -> bool {
    true
}
fn default_scrollback() -> usize {
    10000
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            default_mode: default_mode(),
            animation: default_true(),
            scrollback: default_scrollback(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// 并行子 Agent 最大数量
    #[serde(default = "default_parallel")]
    pub max_parallel_agents: usize,
    /// 缓存目标命中率
    #[serde(default = "default_cache_target")]
    pub cache_target_rate: f64,
    /// 每轮缓存保留数
    #[serde(default = "default_session_rounds")]
    pub session_cache_rounds: usize,
}

fn default_parallel() -> usize {
    8
}
fn default_cache_target() -> f64 {
    0.97
}
fn default_session_rounds() -> usize {
    5
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: default_parallel(),
            cache_target_rate: default_cache_target(),
            session_cache_rounds: default_session_rounds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityConfig {
    /// Gitee OAuth 客户端 ID
    #[serde(default)]
    pub gitee_client_id: String,
    /// GitHub OAuth 客户端 ID
    #[serde(default)]
    pub github_client_id: String,
}

impl Default for CommunityConfig {
    fn default() -> Self {
        Self {
            gitee_client_id: String::new(),
            github_client_id: String::new(),
        }
    }
}

impl Config {
    /// 加载配置：先读用户目录下的 config.toml，不存在则使用默认值
    pub fn load() -> anyhow::Result<Self> {
        let config_path = forge_config_path();
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let mut cfg: Config = toml::from_str(&content)?;
            // 迁移旧模型名 (deepseek-chat → deepseek-v4-flash 等)
            let mut changed = false;
            if cfg.ai.default_model == "deepseek-chat" || cfg.ai.default_model == "deepseek-reasoner" {
                cfg.ai.default_model = default_model();
                changed = true;
            }
            if cfg.ai.flash_model == "deepseek-chat" {
                cfg.ai.flash_model = default_flash_model();
                changed = true;
            }
            if changed {
                std::fs::write(&config_path, toml::to_string_pretty(&cfg)?)?;
            }
            Ok(cfg)
        } else {
            let cfg = Config::default();
            // 自动写入默认配置
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, toml::to_string_pretty(&cfg)?)?;
            Ok(cfg)
        }
    }

    /// 获取有效的 API Key
    pub fn effective_api_key(&self) -> String {
        std::env::var("DEEPSEEK_API_KEY")
            .or_else(|_| std::env::var("FORGE_API_KEY"))
            .unwrap_or_else(|_| self.ai.api_key.clone())
    }
}

/// 用户配置目录
pub fn forge_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge-shell")
}

/// 匿名设备指纹：用于社区经验去重、成功率追踪，不包含可识别个人信息
/// 生成自 hostname 的 hash，持久化到 data_dir，同设备重启不变
pub fn device_fingerprint() -> String {
    let fp_path = forge_data_dir().join("device_id");
    // 如果已持久化，直接读取
    if let Ok(existing) = std::fs::read_to_string(&fp_path) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    // 生成新指纹：hostname hash + pid（防碰撞）
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    host.hash(&mut h);
    std::process::id().hash(&mut h);
    let fp = format!("fp_{:016x}", h.finish());
    // 持久化
    std::fs::create_dir_all(forge_data_dir()).ok();
    let _ = std::fs::write(&fp_path, &fp);
    fp
}

/// 配置文件路径
pub fn forge_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("forge-shell")
        .join("config.toml")
}
