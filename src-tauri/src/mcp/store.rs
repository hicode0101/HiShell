//! MCP Server 配置持久化（mcp.json）：
//! 仅存开关 / 监听地址 / 端口；访问令牌是秘密，走 secrets（OS 钥匙串）。

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct McpConfig {
    /// 是否随应用自动启动 MCP Server
    pub enabled: bool,
    /// 监听 IP（默认仅回环地址；改为 0.0.0.0 等意味着接受局域网访问，需用户自担风险）
    pub listen_host: String,
    /// 监听端口
    pub port: u16,
}

impl Default for McpConfig {
    fn default() -> Self {
        McpConfig { enabled: false, listen_host: "127.0.0.1".into(), port: 9500 }
    }
}

fn file_path() -> std::path::PathBuf {
    paths::config_dir().join("mcp.json")
}

pub fn load() -> McpConfig {
    match paths::read_json::<McpConfig>(&file_path()) {
        Ok(Some(cfg)) => cfg,
        _ => McpConfig::default(),
    }
}

pub fn save(cfg: &McpConfig) -> AppResult<()> {
    paths::write_json(&file_path(), cfg)
}

/// 生成新的访问令牌（hs_ + 32 字节随机数的 base64url，无填充）
pub fn generate_token() -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("hs_{b64}")
}

/// 确保令牌存在（首次开启时生成），返回当前令牌
pub fn ensure_token() -> AppResult<String> {
    if let Some(t) = crate::secrets::mcp_token() {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let t = generate_token();
    crate::secrets::set_mcp_token(&t)?;
    Ok(t)
}
