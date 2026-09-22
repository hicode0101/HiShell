//! 会话导入：扫描 Xshell（.xsh）与 Tabby（config.yaml）会话文件并转为 HiShell 连接配置。
//! 密码类凭据在两种来源中均为加密存储（不可逆），导入后首次连接时按密码认证提示输入。

use serde::{Deserialize, Serialize};

use crate::profile_store::{self, Profile, ProxyConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    /// xshell | tabby
    pub source: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// password | publicKey | keyboardInteractive
    pub auth: String,
    #[serde(default)]
    pub private_key_path: String,
    /// 来源文件完整路径（.xsh 文件或 config.yaml）
    pub origin_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportApplyResult {
    pub imported: usize,
    pub skipped: usize,
}

fn sanitize_id_part(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn candidate_id(c: &ImportCandidate) -> String {
    format!(
        "import-{}-{}-{}-{}",
        c.source,
        sanitize_id_part(&c.host),
        c.port,
        sanitize_id_part(&c.username)
    )
}

fn to_profile(c: &ImportCandidate) -> Profile {
    Profile {
        id: candidate_id(c),
        name: if c.name.trim().is_empty() { c.host.clone() } else { c.name.clone() },
        host: c.host.clone(),
        port: c.port,
        username: if c.username.trim().is_empty() { "root".into() } else { c.username.clone() },
        auth: c.auth.clone(),
        password: String::new(),
        has_stored_password: false,
        private_key_path: c.private_key_path.clone(),
        private_key_passphrase: String::new(),
        has_stored_passphrase: false,
        proxy: ProxyConfig::None,
        keepalive_interval: 5000,
        keepalive_count_max: 10,
        connect_timeout: 10000,
        term: "xterm-256color".into(),
        forwarded_ports: vec![],
        user_key_name: String::new(),
        temporary: false,
        behavior_on_session_end: "keep".into(),
    }
}

/* ---------------- Xshell (.xsh) ---------------- */

/// .xsh 常为 UTF-16LE（带 BOM）；兼容 UTF-8 BOM 与无 BOM 文本
fn decode_text(raw: &[u8]) -> String {
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        return String::from_utf8_lossy(&raw[3..]).into_owned();
    }
    String::from_utf8_lossy(raw).into_owned()
}

/// 解析 INI 为 (section -> key -> value)
fn parse_ini(text: &str) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    let mut out: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            out.entry(section.clone()).or_default();
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            let value = line[eq + 1..].trim().to_string();
            out.entry(section.clone()).or_default().insert(key, value);
        }
    }
    out
}

/// Xshell 认证方式 Method 值：1=密码 2/3=公钥 4=键盘交互
fn map_xshell_auth(method: Option<&String>) -> &'static str {
    match method.and_then(|m| m.trim().parse::<u32>().ok()) {
        Some(2) | Some(3) => "publicKey",
        Some(4) => "keyboardInteractive",
        _ => "password",
    }
}

/// 递归收集目录下所有 .xsh 文件（Xshell 会话目录内含子分组目录）
fn collect_xsh_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_xsh_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("xsh")).unwrap_or(false) {
            out.push(path);
        }
    }
}

/// 常见 Xshell 会话目录：NetSarang Computer/{5,6,7}/Xshell/Sessions 及旧版 NetSarang/Xshell/Sessions
fn xshell_session_dirs() -> Vec<std::path::PathBuf> {
    let Some(doc) = dirs::document_dir() else { return vec![] };
    let mut dirs = Vec::new();
    for root in ["NetSarang Computer", "NetSarang"] {
        for ver in ["8", "7", "6", "5", ""] {
            let p = if ver.is_empty() {
                doc.join(root).join("Xshell").join("Sessions")
            } else {
                doc.join(root).join(ver).join("Xshell").join("Sessions")
            };
            if p.is_dir() && !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }
    dirs
}

fn scan_xshell(out: &mut Vec<ImportCandidate>) {
    for dir in xshell_session_dirs() {
        let mut files = Vec::new();
        collect_xsh_files(&dir, &mut files);
        for file in files {
            let Ok(raw) = std::fs::read(&file) else { continue };
            let ini = parse_ini(&decode_text(&raw));
            let conn = ini.get("CONNECTION");
            let auth = ini.get("CONNECTION:AUTHENTICATION");
            let host = conn.and_then(|s| s.get("Host")).cloned().unwrap_or_default();
            if host.trim().is_empty() {
                continue;
            }
            let port = conn
                .and_then(|s| s.get("Port"))
                .and_then(|p| p.trim().parse::<u16>().ok())
                .unwrap_or(22);
            let username = auth.and_then(|s| s.get("UserName")).cloned().unwrap_or_default();
            let method = auth.and_then(|s| s.get("Method"));
            let auth_kind = map_xshell_auth(method);
            // UserKey 为 Sessions/Keys 下的密钥名，拼接为完整路径（存在才导入）
            let mut private_key_path = String::new();
            if auth_kind == "publicKey" {
                if let Some(key) = auth.and_then(|s| s.get("UserKey")) {
                    let key_path = dir.join("Keys").join(key.trim());
                    if key_path.is_file() {
                        private_key_path = key_path.to_string_lossy().to_string();
                    }
                }
            }
            let name = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&host)
                .to_string();
            out.push(ImportCandidate {
                source: "xshell".into(),
                name,
                host,
                port,
                username,
                auth: auth_kind.into(),
                private_key_path,
                origin_path: file.to_string_lossy().to_string(),
            });
        }
    }
}

/* ---------------- Tabby (config.yaml) ---------------- */

fn yaml_str(v: &serde_yaml::Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

fn scan_tabby(out: &mut Vec<ImportCandidate>) {
    let Some(cfg) = dirs::config_dir().map(|d| d.join("tabby").join("config.yaml")) else { return };
    let Ok(text) = std::fs::read_to_string(&cfg) else { return };
    let Ok(root) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { return };
    let Some(profiles) = root.get("profiles").and_then(|p| p.as_sequence()) else { return };
    for item in profiles {
        if yaml_str(item, "type") != "ssh" {
            continue;
        }
        let opts = item.get("options");
        let host = opts.map(|o| yaml_str(o, "host")).unwrap_or_default();
        if host.trim().is_empty() {
            continue;
        }
        let port = opts
            .and_then(|o| o.get("port"))
            .and_then(|p| p.as_u64())
            .map(|p| p as u16)
            .unwrap_or(22);
        let username = opts.map(|o| yaml_str(o, "user")).unwrap_or_default();
        let name = opts.map(|o| yaml_str(o, "name")).unwrap_or_default();
        out.push(ImportCandidate {
            source: "tabby".into(),
            name: if name.trim().is_empty() { host.clone() } else { name },
            host,
            port,
            username,
            auth: "password".into(),
            private_key_path: opts
                .map(|o| yaml_str(o, "privateKeyPath"))
                .unwrap_or_default(),
            origin_path: cfg.to_string_lossy().to_string(),
        });
    }
}

/// 扫描本机 Xshell / Tabby 会话，返回可导入列表（不做去重，去重在应用时进行）
#[tauri::command]
pub fn import_scan() -> Result<Vec<ImportCandidate>, String> {
    let mut out = Vec::new();
    scan_xshell(&mut out);
    scan_tabby(&mut out);
    Ok(out)
}

/// 应用导入：按 host+port+username 去重后写入 profiles.json
#[tauri::command]
pub fn import_apply(candidates: Vec<ImportCandidate>) -> Result<ImportApplyResult, String> {
    let mut imported = 0;
    let mut skipped = 0;
    let mut list = profile_store::load();
    for c in candidates {
        if c.host.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let dup = list.iter().any(|p| p.host == c.host && p.port == c.port && p.username == c.username);
        if dup {
            skipped += 1;
            continue;
        }
        list.push(to_profile(&c));
        imported += 1;
    }
    if imported > 0 {
        profile_store::save(&list).map_err(|e| e.to_string())?;
    }
    Ok(ImportApplyResult { imported, skipped })
}
