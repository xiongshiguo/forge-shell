use thiserror::Error;

/// 熔炉错误类型
#[derive(Error, Debug)]
pub enum ForgeError {
    #[error("配置错误: {0}")]
    Config(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("网络请求失败: {0}")]
    Network(#[from] reqwest::Error),

    #[error("API 调用失败: {0}")]
    Api(String),

    #[error("序列化错误: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("JSON 序列化失败: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("TOML 反序列化失败: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TUI 错误: {0}")]
    Tui(String),

    #[error("沙箱错误: {0}")]
    Sandbox(String),

    #[error("工具执行失败: {0}")]
    Tool(String),

    #[error("缓存错误: {0}")]
    Cache(String),

    #[error("认证失败: {0}")]
    Auth(String),

    #[error("{0}")]
    General(String),
}

impl From<&str> for ForgeError {
    fn from(s: &str) -> Self {
        ForgeError::General(s.to_string())
    }
}

impl From<String> for ForgeError {
    fn from(s: String) -> Self {
        ForgeError::General(s)
    }
}
