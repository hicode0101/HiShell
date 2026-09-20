use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH 错误: {0}")]
    Ssh(#[from] russh::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("凭据存储错误: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("代理错误: {0}")]
    Proxy(String),

    #[error("操作超时")]
    Timeout,

    #[error("用户取消")]
    Cancelled,

    #[error("会话不存在: {0}")]
    NoSuchSession(u32),

    #[error("任务不存在: {0}")]
    NoSuchTransfer(u64),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        AppError::Msg(s.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

