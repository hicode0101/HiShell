//! 连接配置存储：profiles.json（含默认种子），凭据字段剥离至 secrets 模块（OS 钥匙串）

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::paths;
use crate::secrets;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRule {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // local | remote | dynamic
    #[serde(rename = "bindHost")]
    pub bind_host: String,
    #[serde(rename = "bindPort")]
    pub bind_port: u16,
    #[serde(rename = "targetHost")]
    pub target_host: Option<String>,
    #[serde(rename = "targetPort")]
    pub target_port: Option<u16>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProxyConfig {
    None,
    Socks5 {
        host: String,
        port: u16,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
    Http {
        host: String,
        port: u16,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
    Jump {
        #[serde(rename = "profileId")]
        profile_id: String,
    },
    #[serde(rename = "proxyCommand", alias = "proxyCommand")]
    ProxyCommand { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: String, // password | publicKey | keyboardInteractive | agent | auto
    #[serde(default)]
    pub password: String,
    #[serde(default, rename = "hasStoredPassword")]
    pub has_stored_password: bool,
    #[serde(default, rename = "privateKeyPath")]
    pub private_key_path: String,
    #[serde(default)]
    pub private_key_passphrase: String,
    #[serde(default, rename = "hasStoredPassphrase")]
    pub has_stored_passphrase: bool,
    pub proxy: ProxyConfig,
    #[serde(rename = "keepaliveInterval")]
    pub keepalive_interval: u64,
    #[serde(rename = "keepaliveCountMax")]
    pub keepalive_count_max: u32,
    #[serde(rename = "connectTimeout")]
    pub connect_timeout: u64,
    pub term: String,
    #[serde(rename = "forwardedPorts", default)]
    pub forwarded_ports: Vec<ForwardRule>,
    #[serde(rename = "behaviorOnSessionEnd")]
    pub behavior_on_session_end: String,
}

/// 保存时随请求提交的明文凭据（None = 保持已存值不变）
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SecretsIn {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default, rename = "proxyPassword")]
    pub proxy_password: Option<String>,
    /// 显式要求清除已存凭据
    #[serde(default)]
    pub clear: bool,
}

fn file_path() -> std::path::PathBuf {
    paths::config_dir().join("profiles.json")
}

fn base_profile() -> Profile {
    Profile {
        id: String::new(),
        name: String::new(),
        host: String::new(),
        port: 22,
        username: "root".into(),
        auth: "password".into(),
        password: String::new(),
        has_stored_password: false,
        private_key_path: String::new(),
        private_key_passphrase: String::new(),
        has_stored_passphrase: false,
        proxy: ProxyConfig::None,
        keepalive_interval: 5000,
        keepalive_count_max: 10,
        connect_timeout: 10000,
        term: "xterm-256color".into(),
        forwarded_ports: vec![],
        behavior_on_session_end: "keep".into(),
    }
}

fn seed() -> Vec<Profile> {
    let mut p1 = base_profile();
    p1.id = "profile-yt-10808".into();
    p1.name = "yt - 10808".into();
    p1.host = "149.104.22.80".into();

    let mut p2 = base_profile();
    p2.id = "profile-hicode".into();
    p2.name = "top.hicode.top".into();
    p2.host = "top.hicode.top".into();

    let mut p3 = base_profile();
    p3.id = "profile-jump-demo".into();
    p3.name = "内网机器（经 SOCKS5 代理）".into();
    p3.host = "10.0.0.128".into();
    p3.username = "admin".into();
    p3.auth = "publicKey".into();
    p3.proxy = ProxyConfig::Socks5 {
        host: "127.0.0.1".into(),
        port: 1080,
        username: None,
        password: None,
    };

    vec![p1, p2, p3]
}

pub fn load() -> Vec<Profile> {
    match paths::read_json::<Vec<Profile>>(&file_path()) {
        Ok(Some(list)) => list,
        _ => {
            let list = seed();
            let _ = paths::write_json(&file_path(), &list);
            list
        }
    }
}

pub fn save(list: &[Profile]) -> AppResult<()> {
    paths::write_json(&file_path(), &list)
}

pub fn by_id(id: &str) -> Option<Profile> {
    load().into_iter().find(|p| p.id == id)
}

pub fn upsert_with_secrets(mut profile: Profile, secrets_in: &SecretsIn) -> AppResult<Profile> {
    // 既有 profile（用于保持已有凭据标记）
    let existing = by_id(&profile.id);
    profile.password.clear();
    profile.private_key_passphrase.clear();

    let mut bundle = secrets::get(&profile.id).unwrap_or_default();
    if secrets_in.clear {
        bundle = Default::default();
        secrets::delete(&profile.id);
        profile.has_stored_password = false;
        profile.has_stored_passphrase = false;
    }
    if let Some(p) = &secrets_in.password {
        bundle.password = Some(p.clone());
    }
    if let Some(p) = &secrets_in.passphrase {
        bundle.passphrase = Some(p.clone());
    }
    if let Some(p) = &secrets_in.proxy_password {
        bundle.proxy_password = Some(p.clone());
    }
    if bundle.password.is_some() || bundle.passphrase.is_some() || bundle.proxy_password.is_some() {
        secrets::set(&profile.id, &bundle)?;
        profile.has_stored_password = bundle.password.is_some();
        profile.has_stored_passphrase = bundle.passphrase.is_some();
    } else if existing.is_none() {
        profile.has_stored_password = false;
        profile.has_stored_passphrase = false;
    }

    let mut list = load();
    if let Some(slot) = list.iter_mut().find(|p| p.id == profile.id) {
        *slot = profile.clone();
    } else {
        list.push(profile.clone());
    }
    save(&list)?;
    Ok(profile)
}

pub fn delete(id: &str) -> AppResult<()> {
    secrets::delete(id);
    let mut list = load();
    list.retain(|p| p.id != id);
    save(&list)
}

/// 认证流程"记住密码"写入凭据管理器后，同步 hasStoredPassword 标记
pub fn mark_stored_password(id: &str) -> AppResult<()> {
    let mut list = load();
    if let Some(p) = list.iter_mut().find(|p| p.id == id) {
        if !p.has_stored_password {
            p.has_stored_password = true;
            save(&list)?;
        }
    }
    Ok(())
}

/// 将连接目标解析为 [跳板..., 最终目标] 的 profile 链（跳板机可递归、防环）
pub fn resolve_chain(profile: &Profile, depth: usize) -> AppResult<Vec<Profile>> {
    if depth > 4 {
        return Err(crate::error::AppError::msg("跳板机层级过深（>4），请检查是否存在循环引用"));
    }
    match &profile.proxy {
        ProxyConfig::Jump { profile_id } => {
            let jump = by_id(profile_id)
                .ok_or_else(|| crate::error::AppError::msg(format!("跳板机配置不存在: {profile_id}")))?;
            let mut chain = resolve_chain(&jump, depth + 1)?;
            chain.push(profile.clone());
            Ok(chain)
        }
        _ => Ok(vec![profile.clone()]),
    }
}
