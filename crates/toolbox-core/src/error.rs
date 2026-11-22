use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolboxError {
    #[error("配置读写失败: {0}")]
    Config(String),

    #[error("注册表/系统设置操作失败: {0}")]
    System(String),

    #[error("工具执行失败: {0}")]
    Tool(String),

    #[error("参数无效: {0}")]
    InvalidArgument(String),

    #[error("不支持的平台: {0}")]
    UnsupportedPlatform(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ToolboxError>;
