//! 用户密钥管理：扫描 ~/.ssh 下的公钥（.pub），解析类型/长度/备注/指纹。
//! 密钥生成向导：start 生成并暂存 → finish 落盘（可选口令加密）→ cancel 丢弃。

use std::collections::{HashMap, HashSet};

use base64::Engine;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use ssh_key::private::{EcdsaKeypair, Ed25519Keypair, KeypairData, RsaKeypair};
use ssh_key::{LineEnding, PrivateKey};
use tauri::State;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserKeyInfo {
    /// 文件名去 .pub（如 id_ed25519）
    pub name: String,
    /// ssh-rsa / ssh-ed25519 / ecdsa-sha2-nistp256 …
    pub key_type: String,
    /// 密钥长度（RSA 为模数位数，其余为曲线位数）
    pub bits: u32,
    /// 公钥行尾备注
    pub comment: String,
    /// SHA256:… 指纹（OpenSSH 格式）
    pub fingerprint: String,
    /// 公钥完整路径
    pub public_path: String,
    /// 同名私钥路径（存在才返回）
    pub private_path: Option<String>,
}

fn ssh_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh"))
}

/// 解析 OpenSSH one-line 公钥：type base64blob [comment…]
fn parse_public_key(path: &std::path::Path) -> Option<UserKeyInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().find(|l| !l.trim().is_empty())?.trim().to_string();
    // SSH2 PEM 公钥（BEGIN SSH2 PUBLIC KEY）暂不解析主体，仅记录
    let mut parts = line.split_whitespace();
    let key_type = parts.next()?.to_string();
    let blob_b64 = parts.next()?;
    let comment = parts.collect::<Vec<_>>().join(" ");

    let blob = base64::engine::general_purpose::STANDARD.decode(blob_b64).ok()?;
    let mut cur = &blob[..];
    let name = read_string(&mut cur)?;
    if name != key_type {
        return None;
    }
    let bits = match key_type.as_str() {
        "ssh-rsa" => {
            let _e = read_mpint(&mut cur)?;
            let n = read_mpint(&mut cur)?;
            mpint_bits(&n)
        }
        "ssh-dss" => {
            let p = read_mpint(&mut cur)?;
            mpint_bits(&p)
        }
        "ssh-ed25519" => 256,
        t if t.starts_with("ecdsa-sha2-nistp") => {
            t.rsplit("nistp").next().and_then(|s| s.parse().ok()).unwrap_or(256)
        }
        _ => 0, // 未知类型长度留 0
    };

    let fingerprint = format!("SHA256:{}", base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&blob)).trim_end_matches('='));

    let name = path.file_stem()?.to_string_lossy().to_string();
    let private = path.with_file_name(&name);
    Some(UserKeyInfo {
        name,
        key_type,
        bits,
        comment,
        fingerprint,
        public_path: path.to_string_lossy().to_string(),
        private_path: if private.is_file() { Some(private.to_string_lossy().to_string()) } else { None },
    })
}

fn read_string(cur: &mut &[u8]) -> Option<String> {
    if cur.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([cur[0], cur[1], cur[2], cur[3]]) as usize;
    if cur.len() < 4 + len {
        return None;
    }
    let s = String::from_utf8_lossy(&cur[4..4 + len]).into_owned();
    *cur = &cur[4 + len..];
    Some(s)
}

fn read_mpint(cur: &mut &[u8]) -> Option<Vec<u8>> {
    if cur.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([cur[0], cur[1], cur[2], cur[3]]) as usize;
    if cur.len() < 4 + len {
        return None;
    }
    let v = cur[4..4 + len].to_vec();
    *cur = &cur[4 + len..];
    Some(v)
}

/// 大端 mpint 的位长度
fn mpint_bits(v: &[u8]) -> u32 {
    let mut i = 0;
    while i < v.len() && v[i] == 0 {
        i += 1;
    }
    if i >= v.len() {
        return 0;
    }
    let leading = v[i].leading_zeros();
    ((v.len() - i) as u32) * 8 - leading
}

/// 列出 ~/.ssh 下的公钥
#[tauri::command]
pub fn user_key_list() -> Result<Vec<UserKeyInfo>, String> {
    let Some(dir) = ssh_dir() else { return Ok(vec![]) };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(vec![]) };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let is_pub = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pub"))
            .unwrap_or(false);
        if !is_pub || !path.is_file() {
            continue;
        }
        if let Some(info) = parse_public_key(&path) {
            out.push(info);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// 删除密钥：公钥必删，私钥（同名无扩展名）如存在一并删除
#[tauri::command]
pub fn user_key_delete(public_path: String, private_path: Option<String>) -> Result<(), String> {
    std::fs::remove_file(&public_path).map_err(|e| format!("删除公钥失败: {e}"))?;
    if let Some(p) = private_path {
        let path = std::path::Path::new(&p);
        if path.is_file() {
            std::fs::remove_file(path).map_err(|e| format!("删除私钥失败: {e}"))?;
        }
    }
    Ok(())
}

/* ==================== 密钥生成向导 ==================== */

/// 生成中的密钥对暂存（向导第 2 步生成，第 4 步完成后落盘）
#[derive(Default)]
pub struct KeyGenSessions {
    pending: std::sync::Mutex<HashMap<String, PrivateKey>>,
    cancelled: std::sync::Mutex<HashSet<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedKeyInfo {
    pub gen_id: String,
    pub key_type: String,
    pub bits: u32,
    /// OpenSSH 单行公钥（type blob，备注稍后在 finish 时写入）
    pub public_blob: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedKeyPaths {
    pub name: String,
    pub private_path: String,
    pub public_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyGenFinishRequest {
    pub gen_id: String,
    pub name: String,
    pub comment: String,
    pub passphrase: String,
}

/// 生成密钥对（CPU 密集，跑在阻塞线程池）
fn build_keypair(key_type: &str, bits: u32) -> Result<PrivateKey, String> {
    let data = match key_type {
        "RSA" => {
            if ![2048, 3072, 4096, 8192].contains(&bits) {
                return Err("RSA 密钥长度仅支持 2048 / 3072 / 4096 / 8192 位".into());
            }
            let sk = rsa::RsaPrivateKey::new(&mut OsRng, bits as usize)
                .map_err(|e| format!("RSA 密钥生成失败: {e}"))?;
            let kp = RsaKeypair::try_from(sk).map_err(|e| format!("RSA 密钥编码失败: {e}"))?;
            KeypairData::Rsa(kp)
        }
        "ECDSA" => {
            use ssh_key::EcdsaCurve;
            let curve = match bits {
                256 => EcdsaCurve::NistP256,
                384 => EcdsaCurve::NistP384,
                521 => EcdsaCurve::NistP521,
                _ => return Err("ECDSA 密钥长度仅支持 256 / 384 / 521 位".into()),
            };
            let kp = EcdsaKeypair::random(&mut OsRng, curve)
                .map_err(|e| format!("ECDSA 密钥生成失败: {e}"))?;
            KeypairData::Ecdsa(kp)
        }
        "ED25519" => {
            if bits != 256 {
                return Err("ED25519 固定为 256 位".into());
            }
            let signing = ed25519_dalek::SigningKey::generate(&mut OsRng);
            KeypairData::Ed25519(Ed25519Keypair::from(signing))
        }
        other => return Err(format!("不支持的密钥类型: {other}")),
    };
    PrivateKey::new(data, "").map_err(|e| format!("密钥封装失败: {e}"))
}

/// 向导第 2 步：生成密钥对并暂存（前端先带一个自生成 genId，便于任意时刻取消）
#[tauri::command]
pub async fn user_key_generate_start(
    state: State<'_, KeyGenSessions>,
    gen_id: String,
    key_type: String,
    bits: u32,
) -> Result<GeneratedKeyInfo, String> {
    let kt = key_type.clone();
    let mut key = tokio::task::spawn_blocking(move || build_keypair(&kt, bits))
        .await
        .map_err(|e| format!("生成任务失败: {e}"))??;

    let public_blob = key
        .public_key()
        .to_openssh()
        .map_err(|e| format!("公钥编码失败: {e}"))?
        .trim_end()
        .to_string();

    {
        let mut pending = state.pending.lock().unwrap();
        // 取消发生在生成完成之前：直接丢弃，不入库
        if state.cancelled.lock().unwrap().remove(&gen_id) {
            return Ok(GeneratedKeyInfo { gen_id, key_type, bits, public_blob });
        }
        pending.insert(gen_id.clone(), key);
    }
    Ok(GeneratedKeyInfo { gen_id, key_type, bits, public_blob })
}

/// 向导第 4 步「完成」：按名称/注释/口令把暂存密钥写入 ~/.ssh
#[tauri::command]
pub async fn user_key_generate_finish(
    state: State<'_, KeyGenSessions>,
    req: KeyGenFinishRequest,
) -> Result<GeneratedKeyPaths, String> {
    let mut key = {
        let mut pending = state.pending.lock().unwrap();
        state.cancelled.lock().unwrap().remove(&req.gen_id);
        pending.remove(&req.gen_id).ok_or("生成会话不存在或已取消")?
    };

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err("密钥名称不能为空".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err("密钥名称只能包含字母、数字、点、下划线和连字符".into());
    }
    let dir = ssh_dir().ok_or("无法定位用户主目录")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 .ssh 目录失败: {e}"))?;
    let priv_path = dir.join(&name);
    let pub_path = dir.join(format!("{name}.pub"));
    if priv_path.exists() || pub_path.exists() {
        return Err(format!("同名密钥已存在：{name}"));
    }

    if !req.passphrase.is_empty() {
        key = key
            .encrypt(&mut OsRng, req.passphrase.as_bytes())
            .map_err(|e| format!("私钥加密失败: {e}"))?;
    }
    // encrypt() 会用 key_data 重建 public_key 并丢掉注释，因此注释必须在加密后再设置
    key.set_comment(req.comment.trim());

    let pem = key.to_openssh(LineEnding::LF).map_err(|e| format!("私钥编码失败: {e}"))?;
    std::fs::write(&priv_path, pem.as_bytes()).map_err(|e| format!("写入私钥失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&priv_path, std::fs::Permissions::from_mode(0o600));
    }
    let pub_line = key.public_key().to_openssh().map_err(|e| format!("公钥编码失败: {e}"))?;
    std::fs::write(&pub_path, format!("{pub_line}\n")).map_err(|e| format!("写入公钥失败: {e}"))?;

    Ok(GeneratedKeyPaths {
        name,
        private_path: priv_path.to_string_lossy().to_string(),
        public_path: pub_path.to_string_lossy().to_string(),
    })
}

/// 取消生成：密钥已暂存则直接丢弃；生成中则记下标记，待生成完成后丢弃
#[tauri::command]
pub fn user_key_generate_cancel(state: State<'_, KeyGenSessions>, gen_id: String) -> Result<(), String> {
    let mut pending = state.pending.lock().unwrap();
    if pending.remove(&gen_id).is_none() {
        state.cancelled.lock().unwrap().insert(gen_id);
    }
    Ok(())
}

/// 「保存为文件」：把前端展示中的公钥文本写到下载目录
#[tauri::command]
pub fn user_key_save_public(name: String, content: String) -> Result<String, String> {
    let safe = name.trim();
    if safe.is_empty() {
        return Err("密钥名称不能为空".into());
    }
    let dir = dirs::download_dir().or_else(dirs::home_dir).ok_or("无法定位下载目录")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建下载目录失败: {e}"))?;
    let mut path = dir.join(format!("{safe}.pub"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{safe} ({n}).pub"));
        n += 1;
    }
    let mut text = content;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(&path, text).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}
