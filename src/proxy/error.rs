//! Proxy error types

use crate::error::AppError;
use std::fmt;

#[derive(Debug)]
pub enum ProxyError {
    AuthError(String),
    ConfigError(String),
    ForwardError(String),
    InvalidRequest(String),
    UpstreamError { status: u16, body: String },
    Io(std::io::Error),
}

impl fmt::Display for ProxyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyError::AuthError(msg) => write!(f, "认证错误: {msg}"),
            ProxyError::ConfigError(msg) => write!(f, "配置错误: {msg}"),
            ProxyError::ForwardError(msg) => write!(f, "转发错误: {msg}"),
            ProxyError::InvalidRequest(msg) => write!(f, "无效请求: {msg}"),
            ProxyError::UpstreamError { status, body } => {
                write!(f, "上游 HTTP {status}: {body}")
            }
            ProxyError::Io(e) => write!(f, "IO 错误: {e}"),
        }
    }
}

impl std::error::Error for ProxyError {}

impl From<AppError> for ProxyError {
    fn from(err: AppError) -> Self {
        ProxyError::ForwardError(err.to_string())
    }
}

impl From<std::io::Error> for ProxyError {
    fn from(err: std::io::Error) -> Self {
        ProxyError::Io(err)
    }
}

impl From<serde_json::Error> for ProxyError {
    fn from(err: serde_json::Error) -> Self {
        ProxyError::InvalidRequest(err.to_string())
    }
}
