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
    pub auth: String, // password | priFileKey | keyboardInteractive | agent | auto
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
    /// 用户密钥管理中的密钥名（auth = userKey 时连接时解析为 ~/.ssh/<name>）
    #[serde(default, rename = "userKeyName")]
    pub user_key_name: String,
    /// 快速连接临时配置：连接可用，但 profile_list 对其隐藏
    #[serde(default)]
    pub temporary: bool,
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
    /// 仅清除已存密码（保留口令等其他凭据）
    #[serde(default)]
    pub clear_password: bool,
    /// 仅清除已存私钥口令
    #[serde(default, rename = "clearPassphrase")]
    pub clear_passphrase: bool,
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
        user_key_name: String::new(),
        temporary: false,
        proxy: ProxyConfig::None,
        keepalive_interval: 5000,
        keepalive_count_max: 10,
        connect_timeout: 10000,
        term: "xterm-256color".into(),
        forwarded_ports: vec![],
        behavior_on_session_end: "keep".into(),
    }
}

/// 首次启动写入的示例配置：全部为无害占位地址（不含任何真实服务器信息），
/// 仅在 profiles.json 缺失/损坏时生成一次，之后完全由用户数据接管
fn seed() -> Vec<Profile> {
    let mut p1 = base_profile();
    p1.id = "profile-example-1".into();
    p1.name = "示例：云服务器".into();
    p1.host = "example.com".into();
    p1.username = "root".into();

    let mut p2 = base_profile();
    p2.id = "profile-example-2".into();
    p2.name = "示例：内网主机".into();
    p2.host = "192.168.1.100".into();
    p2.username = "root".into();

    let mut p3 = base_profile();
    p3.id = "profile-example-3".into();
    p3.name = "示例：经 SOCKS5 代理访问内网".into();
    p3.host = "10.0.0.100".into();
    p3.username = "root".into();
    p3.auth = "priFileKey".into();
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
    profile.password.clear();
    profile.private_key_passphrase.clear();

    let mut list = load();
    // 认证方式是否发生切换（同一 id 保存前后的 auth 对比）
    let auth_changed = list
        .iter()
        .find(|p| p.id == profile.id)
        .map(|p| p.auth.as_str())
        != Some(profile.auth.as_str());
    let key_based = matches!(profile.auth.as_str(), "priFileKey" | "userKey");

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
    // 按字段清除（优先级高于同请求中的赋值）
    if secrets_in.clear_password {
        bundle.password = None;
    }
    if secrets_in.clear_passphrase {
        bundle.passphrase = None;
    }
    // 切换认证方式后，新方式用不到的已存凭据一律清除：
    // 密钥类（priFileKey/userKey）只用口令，密码类（password/keyboardInteractive/auto）只用密码，agent 两者皆不用
    if auth_changed {
        if key_based || profile.auth == "agent" {
            bundle.password = None;
        }
        if !key_based {
            bundle.passphrase = None;
        }
    }
    if bundle.password.is_some() || bundle.passphrase.is_some() || bundle.proxy_password.is_some() {
        secrets::set(&profile.id, &bundle)?;
    } else {
        secrets::delete(&profile.id);
    }
    profile.has_stored_password = bundle.password.is_some();
    profile.has_stored_passphrase = bundle.passphrase.is_some();

    // 配置规范：只保留当前认证方式所需的配置项，其它认证方式的字段一律重置
    if profile.auth != "priFileKey" {
        profile.private_key_path.clear();
    }
    if profile.auth != "userKey" {
        profile.user_key_name.clear();
    }

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

/// 清理全部历史临时配置（含其在 OS 凭据管理器中的凭据），返回清理条数。
/// 快速连接每次连接都会落盘一份 temporary 配置且对列表隐藏，反复使用会无限累积。
/// 调用时机：应用启动完成后（后台任务）——此刻不存在任何会话标签，
/// 删除全部 temporary 必然安全；本次运行中新产生的临时配置不受影响，留待下次启动清理。
pub fn purge_temporary() -> usize {
    let list = load();
    let removed: Vec<String> = list
        .iter()
        .filter(|p| p.temporary)
        .map(|p| p.id.clone())
        .collect();
    if removed.is_empty() {
        return 0;
    }
    let keep: Vec<Profile> = list.into_iter().filter(|p| !p.temporary).collect();
    for id in &removed {
        secrets::delete(id);
    }
    if save(&keep).is_err() {
        return 0;
    }
    removed.len()
}

/// 按给定 id 顺序重排配置（未列出的配置按原顺序追加在后），顺序持久化
pub fn reorder(ids: &[String]) -> AppResult<()> {
    let mut list = load();
    let mut ordered: Vec<Profile> = Vec::with_capacity(list.len());
    for id in ids {
        if let Some(pos) = list.iter().position(|p| &p.id == id) {
            ordered.push(list.remove(pos));
        }
    }
    ordered.extend(list);
    save(&ordered)
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
