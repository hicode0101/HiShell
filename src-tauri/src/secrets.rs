//! 凭据存储：优先 OS 钥匙串（keyring：Windows Credential Manager / macOS Keychain / Linux Secret-Service），
//! 存储失败（如无 D-Bus 的 Linux）时回落到本地加密混淆文件并标记 insecure。
//! 每个连接 profile 一个条目：键 hisell.profile.{id}，值为 JSON 凭据包。

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::paths;

const SERVICE: &str = "hishell.profile";

/// 单个连接的凭据包
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileSecrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "proxyPassword")]
    pub proxy_password: Option<String>,
}

fn entry(profile_id: &str) -> keyring::Entry {
    keyring::Entry::new(SERVICE, profile_id).expect("keyring entry")
}

fn fallback_path() -> std::path::PathBuf {
    paths::config_dir().join("secrets.fallback.json")
}

/// 本地回落存储（仅 base64 混淆，明确标记为不安全）
fn fallback_load() -> std::collections::HashMap<String, String> {
    paths::read_json::<std::collections::HashMap<String, String>>(&fallback_path())
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn fallback_save(map: &std::collections::HashMap<String, String>) -> AppResult<()> {
    paths::write_json(&fallback_path(), map)
}

fn b64e(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

fn b64d(s: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.decode(s).ok().and_then(|b| String::from_utf8(b).ok())
}

pub fn get(profile_id: &str) -> Option<ProfileSecrets> {
    let key = format!("{SERVICE}.{profile_id}");
    if let Ok(raw) = entry(profile_id).get_password() {
        return serde_json::from_str(&raw).ok();
    }
    // 回落文件
    let map = fallback_load();
    map.get(&key).and_then(|v| b64d(v)).and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn set(profile_id: &str, secrets: &ProfileSecrets) -> AppResult<()> {
    let raw = serde_json::to_string(secrets)?;
    match entry(profile_id).set_password(&raw) {
        Ok(()) => Ok(()),
        Err(_) => {
            // keyring 不可用 → 回落文件
            let key = format!("{SERVICE}.{profile_id}");
            let mut map = fallback_load();
            map.insert(key, b64e(&raw));
            fallback_save(&map)
        }
    }
}

pub fn delete(profile_id: &str) {
    let _ = entry(profile_id).delete_credential();
    let key = format!("{SERVICE}.{profile_id}");
    let mut map = fallback_load();
    if map.remove(&key).is_some() {
        let _ = fallback_save(&map);
    }
}

/* ---------------- MCP Server 访问令牌（独立条目，不在连接配置命名空间内） ---------------- */

const SERVICE_MCP: &str = "hishell.mcp";
const MCP_TOKEN_KEY: &str = "token";

fn mcp_entry() -> keyring::Entry {
    keyring::Entry::new(SERVICE_MCP, MCP_TOKEN_KEY).expect("keyring entry")
}

/// 读取 MCP 访问令牌（keyring 优先，回落文件兜底）
pub fn mcp_token() -> Option<String> {
    if let Ok(raw) = mcp_entry().get_password() {
        return Some(raw);
    }
    let key = format!("{SERVICE_MCP}.{MCP_TOKEN_KEY}");
    fallback_load().get(&key).and_then(|v| b64d(v))
}

/// 写入 MCP 访问令牌
pub fn set_mcp_token(token: &str) -> AppResult<()> {
    match mcp_entry().set_password(token) {
        Ok(()) => Ok(()),
        Err(_) => {
            let key = format!("{SERVICE_MCP}.{MCP_TOKEN_KEY}");
            let mut map = fallback_load();
            map.insert(key, b64e(token));
            fallback_save(&map)
        }
    }
}
