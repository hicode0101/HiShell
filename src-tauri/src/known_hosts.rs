//! known_hosts 管理：存储于应用配置目录 known_hosts.json（对齐 Tabby 的自有存储策略）

use std::path::PathBuf;

use russh::keys::key::PublicKey;
use russh::keys::PublicKeyBase64;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHostEntry {
    pub host: String,
    pub port: u16,
    #[serde(rename = "keyType")]
    pub key_type: String,
    /// 算法名 + base64 blob，用于比较密钥是否一致
    #[serde(rename = "keyData")]
    pub key_data: String,
    /// SHA256 指纹（"SHA256:xxxx"）
    pub fingerprint: String,
    #[serde(rename = "addedAt")]
    pub added_at: u64,
}

#[derive(Default)]
pub struct KnownHosts;

fn file_path() -> PathBuf {
    paths::config_dir().join("known_hosts.json")
}

fn load() -> Vec<KnownHostEntry> {
    paths::read_json::<Vec<KnownHostEntry>>(&file_path()).ok().flatten().unwrap_or_default()
}

fn save(entries: &[KnownHostEntry]) -> AppResult<()> {
    paths::write_json(&file_path(), &entries)
}

fn key_openssh(key: &PublicKey) -> String {
    format!("{} {}", key.name(), key.public_key_base64())
}

fn key_fingerprint(key: &PublicKey) -> String {
    format!("SHA256:{}", key.fingerprint())
}

fn find<'a>(entries: &'a [KnownHostEntry], host: &str, port: u16) -> Option<&'a KnownHostEntry> {
    entries.iter().find(|e| e.host == host && e.port == port)
}

pub enum VerifyResult {
    /// 已知且匹配，直接通过
    Known,
    /// 新主机，fingerprint 为将展示的指纹
    New { fingerprint: String, key_type: String },
    /// 主机密钥与记录不符（可能 MITM）
    Mismatch { old: String, new: String, key_type: String },
}

impl KnownHosts {
    /// 校验服务器密钥；不改存储，仅给出判定
    pub fn verify(&self, host: &str, port: u16, key: &PublicKey) -> VerifyResult {
        let entries = load();
        let data = key_openssh(key);
        let fp = key_fingerprint(key);
        match find(&entries, host, port) {
            None => VerifyResult::New { fingerprint: fp, key_type: key.name().to_string() },
            Some(entry) => {
                if entry.key_data == data {
                    VerifyResult::Known
                } else {
                    VerifyResult::Mismatch {
                        old: entry.fingerprint.clone(),
                        new: fp,
                        key_type: key.name().to_string(),
                    }
                }
            }
        }
    }

    /// 记住主机密钥（覆盖同 host:port 旧记录）
    pub fn remember(&self, host: &str, port: u16, key: &PublicKey) -> AppResult<()> {
        let mut entries = load();
        let data = key_openssh(key);
        entries.retain(|e| !(e.host == host && e.port == port));
        entries.push(KnownHostEntry {
            host: host.to_string(),
            port,
            key_type: key.name().to_string(),
            key_data: data,
            fingerprint: key_fingerprint(key),
            added_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        });
        save(&entries)
    }
}
