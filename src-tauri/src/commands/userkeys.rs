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

/// 重命名密钥：私钥/公钥文件一并改名，返回刷新后的密钥信息
#[tauri::command]
pub fn user_key_rename(
    public_path: String,
    private_path: Option<String>,
    new_name: String,
) -> Result<UserKeyInfo, String> {
    let name = validate_key_name(&new_name)?;
    let dir = ssh_dir().ok_or("无法定位用户主目录")?;
    let old_pub = std::path::Path::new(&public_path);
    if !old_pub.is_file() {
        return Err("公钥文件不存在".into());
    }
    let new_priv = dir.join(&name);
    let new_pub = dir.join(format!("{name}.pub"));
    if new_priv.exists() || new_pub.exists() {
        return Err(format!("同名密钥已存在：{name}"));
    }

    std::fs::rename(old_pub, &new_pub).map_err(|e| format!("公钥改名失败: {e}"))?;
    if let Some(pp) = private_path.as_deref().filter(|p| !p.trim().is_empty()) {
        let old_priv = std::path::Path::new(pp);
        if old_priv.is_file() {
            std::fs::rename(old_priv, &new_priv).map_err(|e| {
                let _ = std::fs::rename(&new_pub, old_pub); // 回滚公钥，避免半改名状态
                format!("私钥改名失败: {e}")
            })?;
        }
    }

    parse_public_key(&new_pub).ok_or_else(|| "重命名完成，但公钥解析失败".to_string())
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

    let name = validate_key_name(&req.name)?;
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
    write_private_key(&priv_path, &pem)?;
    let pub_line = key.public_key().to_openssh().map_err(|e| format!("公钥编码失败: {e}"))?;
    write_text(&pub_path, &pub_line)?;

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
    write_text(&path, &content)?;
    Ok(path.to_string_lossy().to_string())
}

/* ==================== 密钥导入 / 导出 ==================== */

/// 解码私钥文本；支持 OpenSSH（含加密）与 PEM（PKCS#1 / PKCS#8 / SEC1，仅明文）
fn load_private_text(text: &str, pass: Option<&str>) -> Result<PrivateKey, String> {
    // 1) OpenSSH 格式（加密外壳可无口令解析出元信息）
    if let Ok(key) = PrivateKey::from_openssh(text) {
        if !key.is_encrypted() {
            return Ok(key);
        }
        let p = pass.filter(|p| !p.is_empty()).ok_or("该私钥已加密，请输入其口令")?;
        return key.decrypt(p).map_err(|_| "口令错误，无法解密私钥".to_string());
    }

    // 2) PEM 家族
    let t = text.trim();
    if !t.starts_with("-----BEGIN") {
        return Err("无法识别的私钥格式（支持 OpenSSH / PEM PKCS#1 / PKCS#8 / SEC1 格式私钥）".into());
    }
    if t.contains("ENCRYPTED") || t.contains("Proc-Type: 4,ENCRYPTED") {
        return Err("暂不支持加密的 PEM 私钥，请先用 ssh-keygen 转为 OpenSSH 格式后导入".into());
    }
    if let Ok(k) = pkcs1_rsa(t) {
        return Ok(k);
    }
    if let Ok(k) = pkcs8_any(t) {
        return Ok(k);
    }
    if let Ok(k) = sec1_ec(t) {
        return Ok(k);
    }
    Err("无法识别的私钥格式（支持 OpenSSH / PEM PKCS#1 / PKCS#8 / SEC1 格式私钥）".into())
}

/// PKCS#1：-----BEGIN RSA PRIVATE KEY-----
fn pkcs1_rsa(t: &str) -> Result<PrivateKey, String> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    if !t.contains("BEGIN RSA PRIVATE KEY") {
        return Err("not pkcs1".into());
    }
    let sk = rsa::RsaPrivateKey::from_pkcs1_pem(t).map_err(|e| format!("PKCS#1 RSA 解析失败: {e}"))?;
    let kp = RsaKeypair::try_from(sk).map_err(|e| format!("RSA 密钥编码失败: {e}"))?;
    PrivateKey::new(KeypairData::Rsa(kp), "").map_err(|e| format!("密钥封装失败: {e}"))
}

/// PKCS#8：-----BEGIN PRIVATE KEY-----（RSA / Ed25519 / EC 曲线自动识别）
fn pkcs8_any(t: &str) -> Result<PrivateKey, String> {
    use rsa::pkcs8::DecodePrivateKey;
    if !t.contains("BEGIN PRIVATE KEY") {
        return Err("not pkcs8".into());
    }
    if let Ok(sk) = rsa::RsaPrivateKey::from_pkcs8_pem(t) {
        let kp = RsaKeypair::try_from(sk).map_err(|e| format!("RSA 密钥编码失败: {e}"))?;
        return PrivateKey::new(KeypairData::Rsa(kp), "").map_err(|e| format!("密钥封装失败: {e}"));
    }
    if let Ok(signing) = ed25519_from_pkcs8(t) {
        return PrivateKey::new(KeypairData::Ed25519(Ed25519Keypair::from(signing)), "")
            .map_err(|e| format!("密钥封装失败: {e}"));
    }
    if let Ok(sk) = p256::SecretKey::from_pkcs8_pem(t) {
        return ec_to_private256(sk);
    }
    if let Ok(sk) = p384::SecretKey::from_pkcs8_pem(t) {
        return ec_to_private384(sk);
    }
    if let Ok(sk) = p521::SecretKey::from_pkcs8_pem(t) {
        return ec_to_private521(sk);
    }
    Err("PKCS#8 私钥解析失败（算法不支持）".into())
}

/// Ed25519 PKCS#8：PrivateKeyInfo → SigningKey
fn ed25519_from_pkcs8(t: &str) -> Result<ed25519_dalek::SigningKey, String> {
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    ed25519_dalek::SigningKey::from_pkcs8_pem(t).map_err(|e| format!("Ed25519 解析失败: {e}"))
}

fn ec_to_private256(sk: p256::SecretKey) -> Result<PrivateKey, String> {
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    let public = sk.public_key().to_encoded_point(false);
    PrivateKey::new(KeypairData::Ecdsa(EcdsaKeypair::NistP256 { public, private: sk.into() }), "")
        .map_err(|e| format!("密钥封装失败: {e}"))
}

fn ec_to_private384(sk: p384::SecretKey) -> Result<PrivateKey, String> {
    use p384::elliptic_curve::sec1::ToEncodedPoint;
    let public = sk.public_key().to_encoded_point(false);
    PrivateKey::new(KeypairData::Ecdsa(EcdsaKeypair::NistP384 { public, private: sk.into() }), "")
        .map_err(|e| format!("密钥封装失败: {e}"))
}

fn ec_to_private521(sk: p521::SecretKey) -> Result<PrivateKey, String> {
    use p521::elliptic_curve::sec1::ToEncodedPoint;
    let public = sk.public_key().to_encoded_point(false);
    PrivateKey::new(KeypairData::Ecdsa(EcdsaKeypair::NistP521 { public, private: sk.into() }), "")
        .map_err(|e| format!("密钥封装失败: {e}"))
}

/// SEC1：-----BEGIN EC PRIVATE KEY-----
fn sec1_ec(t: &str) -> Result<PrivateKey, String> {
    if !t.contains("BEGIN EC PRIVATE KEY") {
        return Err("not sec1".into());
    }
    if let Ok(sk) = p256::SecretKey::from_sec1_pem(t) {
        return ec_to_private256(sk);
    }
    if let Ok(sk) = p384::SecretKey::from_sec1_pem(t) {
        return ec_to_private384(sk);
    }
    if let Ok(sk) = p521::SecretKey::from_sec1_pem(t) {
        return ec_to_private521(sk);
    }
    Err("SEC1 EC 私钥解析失败（曲线不支持）".into())
}

/// 解码 OpenSSH 格式私钥文本；加密私钥需要口令
fn load_openssh_private(text: &str, pass: Option<&str>) -> Result<PrivateKey, String> {
    load_private_text(text, pass)
}

/// 从公钥推导（类型, 位数）
fn pub_key_meta(pk: &ssh_key::PublicKey) -> (String, u32) {
    use ssh_key::{Algorithm, EcdsaCurve};
    match pk.algorithm() {
        Algorithm::Rsa { .. } => {
            let bits = pk.key_data().rsa().map(|r| mpint_bits(r.n.as_bytes())).unwrap_or(0);
            ("ssh-rsa".to_string(), bits)
        }
        Algorithm::Ed25519 => ("ssh-ed25519".to_string(), 256),
        Algorithm::Ecdsa { curve } => {
            let (bits, ident) = match curve {
                EcdsaCurve::NistP256 => (256, "nistp256"),
                EcdsaCurve::NistP384 => (384, "nistp384"),
                EcdsaCurve::NistP521 => (521, "nistp521"),
            };
            (format!("ecdsa-sha2-{ident}"), bits)
        }
        Algorithm::Dsa => {
            let bits = pk.key_data().dsa().map(|d| mpint_bits(d.p.as_bytes())).unwrap_or(0);
            ("ssh-dss".to_string(), bits)
        }
        _ => ("unknown".to_string(), 0),
    }
}

/// 校验密钥文件名（字母数字._-）
fn validate_key_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("密钥名称不能为空".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err("密钥名称只能包含字母、数字、点、下划线和连字符".into());
    }
    Ok(name.to_string())
}

fn write_text(path: &std::path::Path, content: &str) -> Result<(), String> {
    let mut text = content.to_string();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(path, text).map_err(|e| format!("写入文件失败: {e}"))
}

/// 写私钥文件（unix 下收紧权限）
fn write_private_key(path: &std::path::Path, pem: &str) -> Result<(), String> {
    std::fs::write(path, pem.as_bytes()).map_err(|e| format!("写入私钥失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/* ---------- 导入 ---------- */

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInspect {
    pub encrypted: bool,
    pub key_type: Option<String>,
    pub bits: Option<u32>,
    pub comment: String,
}

/// 导入前探测：格式/是否加密/类型与位数
/// 注意：加密的 OpenSSH 私钥可以解析出外层公钥信息，探测不应要求口令，
/// 否则导入对话框无法弹出（口令在对话框里输入）。
#[tauri::command]
pub fn user_key_inspect(source_path: String) -> Result<KeyInspect, String> {
    let text = std::fs::read_to_string(&source_path).map_err(|e| format!("读取文件失败: {e}"))?;

    // OpenSSH：加密的也能从外层拿到公钥/注释等元信息
    if let Ok(key) = PrivateKey::from_openssh(&text) {
        let (key_type, bits) = pub_key_meta(key.public_key());
        return Ok(KeyInspect {
            encrypted: key.is_encrypted(),
            key_type: Some(key_type),
            bits: Some(bits),
            comment: key.comment().to_string(),
        });
    }

    // PEM 家族（仅明文；加密 PEM 直接给出明确提示）
    let t = text.trim();
    if t.starts_with("-----BEGIN") {
        if t.contains("ENCRYPTED") || t.contains("Proc-Type: 4,ENCRYPTED") {
            return Err("暂不支持加密的 PEM 私钥，请先用 ssh-keygen 转为 OpenSSH 格式后导入".into());
        }
        for k in [pkcs1_rsa(t), pkcs8_any(t), sec1_ec(t)] {
            if let Ok(key) = k {
                let (key_type, bits) = pub_key_meta(key.public_key());
                return Ok(KeyInspect {
                    encrypted: false,
                    key_type: Some(key_type),
                    bits: Some(bits),
                    comment: key.comment().to_string(),
                });
            }
        }
    }

    Err("无法识别的私钥格式（支持 OpenSSH / PEM PKCS#1 / PKCS#8 / SEC1 格式私钥）".into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyImportRequest {
    pub source_path: String,
    pub name: String,
    pub passphrase: String,
}

/// 导入私钥到 ~/.ssh：口令既用于解密源文件，也用于加密落盘副本
#[tauri::command]
pub async fn user_key_import(req: KeyImportRequest) -> Result<UserKeyInfo, String> {
    let text = std::fs::read_to_string(&req.source_path).map_err(|e| format!("读取文件失败: {e}"))?;
    let pass = Some(req.passphrase.as_str()).filter(|p| !p.is_empty());
    let mut key = load_openssh_private(&text, pass)?;
    let name = validate_key_name(&req.name)?;

    let dir = ssh_dir().ok_or("无法定位用户主目录")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 .ssh 目录失败: {e}"))?;
    let priv_path = dir.join(&name);
    let pub_path = dir.join(format!("{name}.pub"));
    if priv_path.exists() || pub_path.exists() {
        return Err(format!("同名密钥已存在：{name}"));
    }

    if let Some(p) = pass {
        key = key
            .encrypt(&mut OsRng, p.as_bytes())
            .map_err(|e| format!("私钥加密失败: {e}"))?;
    }
    let pem = key.to_openssh(LineEnding::LF).map_err(|e| format!("私钥编码失败: {e}"))?;
    write_private_key(&priv_path, &pem)?;
    let pub_line = key.public_key().to_openssh().map_err(|e| format!("公钥编码失败: {e}"))?;
    write_text(&pub_path, &pub_line)?;

    parse_public_key(&pub_path).ok_or_else(|| "导入完成，但公钥解析失败".to_string())
}

/* ---------- 导出 ---------- */

#[tauri::command]
pub fn user_key_public_content(public_path: String) -> Result<String, String> {
    let text = std::fs::read_to_string(&public_path).map_err(|e| format!("读取公钥失败: {e}"))?;
    Ok(text.trim_end().to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyExportPrivateRequest {
    pub private_path: String,
    pub current_passphrase: String,
    pub new_passphrase: String,
    /// openssh / pem / ppk
    pub format: String,
    pub dest_path: String,
}

/// 导出私钥到指定路径：校验当前口令 → 按目标格式编码 → 落盘
/// - openssh: OpenSSH New Format（可用新口令加密）
/// - pem:     PEM(OpenSSH2)，即 PKCS#8（不支持口令）
/// - ppk:     PuTTY PPK v3（不支持口令）
#[tauri::command]
pub fn user_key_export_private_to(req: KeyExportPrivateRequest) -> Result<(), String> {
    let text = std::fs::read_to_string(&req.private_path).map_err(|e| format!("读取私钥失败: {e}"))?;
    let cur = Some(req.current_passphrase.as_str()).filter(|p| !p.is_empty());
    let key = load_private_text(&text, cur)?;

    let content = match req.format.as_str() {
        "openssh" => {
            let mut key = key;
            let new_pass = Some(req.new_passphrase.as_str()).filter(|p| !p.is_empty());
            if let Some(np) = new_pass {
                key = key
                    .encrypt(&mut OsRng, np.as_bytes())
                    .map_err(|e| format!("私钥加密失败: {e}"))?;
            }
            key.to_openssh(LineEnding::LF).map_err(|e| format!("私钥编码失败: {e}"))?.to_string()
        }
        "pem" => {
            if !req.new_passphrase.is_empty() {
                return Err("PEM(OpenSSH2) 格式导出暂不支持设置口令，请留空".into());
            }
            to_pkcs8_pem(&key)?
        }
        "ppk" => {
            if !req.new_passphrase.is_empty() {
                return Err("PuTTY .ppk 格式导出暂不支持设置口令，请留空".into());
            }
            to_ppk_v3(&key)?
        }
        other => return Err(format!("不支持的导出格式: {other}")),
    };
    write_private_key(std::path::Path::new(&req.dest_path), &content)
}

/// ssh_key 私钥 → PKCS#8 PEM 文本
fn to_pkcs8_pem(key: &PrivateKey) -> Result<String, String> {
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    match key.key_data() {
        KeypairData::Rsa(rkp) => {
            let sk = rsa_from_kp(rkp)?;
            sk.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).map(|s| s.to_string()).map_err(|e| format!("PKCS#8 编码失败: {e}"))
        }
        KeypairData::Ed25519(ek) => {
            let sk = ed25519_dalek::SigningKey::try_from(ek).map_err(|e| format!("Ed25519 转换失败: {e}"))?;
            sk.to_pkcs8_pem(LineEnding::LF).map(|s| s.to_string()).map_err(|e| format!("PKCS#8 编码失败: {e}"))
        }
        KeypairData::Ecdsa(EcdsaKeypair::NistP256 { private, .. }) => {
            let sk = p256::SecretKey::from_slice(private.as_slice()).map_err(|e| format!("EC 转换失败: {e}"))?;
            sk.to_pkcs8_pem(LineEnding::LF).map(|s| s.to_string()).map_err(|e| format!("PKCS#8 编码失败: {e}"))
        }
        KeypairData::Ecdsa(EcdsaKeypair::NistP384 { private, .. }) => {
            let sk = p384::SecretKey::from_slice(private.as_slice()).map_err(|e| format!("EC 转换失败: {e}"))?;
            sk.to_pkcs8_pem(LineEnding::LF).map(|s| s.to_string()).map_err(|e| format!("PKCS#8 编码失败: {e}"))
        }
        KeypairData::Ecdsa(EcdsaKeypair::NistP521 { private, .. }) => {
            let sk = p521::SecretKey::from_slice(private.as_slice()).map_err(|e| format!("EC 转换失败: {e}"))?;
            sk.to_pkcs8_pem(LineEnding::LF).map(|s| s.to_string()).map_err(|e| format!("PKCS#8 编码失败: {e}"))
        }
        _ => Err("该密钥类型不支持导出为 PEM(OpenSSH2) 格式".into()),
    }
}

/// RsaKeypair → rsa::RsaPrivateKey
/// 注意：ssh-key 0.6.7 自带的 TryFrom 存在 bug（primes 误用两个 p），这里手动构建。
fn rsa_from_kp(rkp: &RsaKeypair) -> Result<rsa::RsaPrivateKey, String> {
    rsa::RsaPrivateKey::from_components(
        rsa::BigUint::from_bytes_be(rkp.public.n.as_bytes()),
        rsa::BigUint::from_bytes_be(rkp.public.e.as_bytes()),
        rsa::BigUint::from_bytes_be(rkp.private.d.as_bytes()),
        vec![
            rsa::BigUint::from_bytes_be(rkp.private.p.as_bytes()),
            rsa::BigUint::from_bytes_be(rkp.private.q.as_bytes()),
        ],
    )
    .map_err(|e| format!("RSA 转换失败: {e}"))
}

/* ---------- PuTTY PPK v3 导出 ---------- */

/// SSH wire 编码：u32 长度 + 数据
fn put_wire_str(v: &mut Vec<u8>, data: &[u8]) {
    v.extend((data.len() as u32).to_be_bytes());
    v.extend_from_slice(data);
}

/// SSH wire 编码：mpint（去前导零，正数高位为 1 时补 0x00）
fn put_wire_mpint(v: &mut Vec<u8>, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() && bytes[i] == 0 {
        i += 1;
    }
    let body = &bytes[i..];
    if body.is_empty() {
        v.extend(0u32.to_be_bytes());
        return;
    }
    let mut out = body.to_vec();
    if out[0] & 0x80 != 0 {
        out.insert(0, 0);
    }
    v.extend((out.len() as u32).to_be_bytes());
    v.extend_from_slice(&out);
}

/// ssh_key 私钥 → PuTTY PPK v3 文本（Encryption: none）
fn to_ppk_v3(key: &PrivateKey) -> Result<String, String> {
    use base64::Engine;
    use hmac::Mac;
    let algo = key.algorithm();

    // 公钥 blob：取 openssh 单行公钥的第二段 base64
    let pub_line = key.public_key().to_openssh().map_err(|e| format!("公钥编码失败: {e}"))?;
    let pub_b64 = pub_line.split_whitespace().nth(1).ok_or("公钥数据异常")?;
    let pub_blob = base64::engine::general_purpose::STANDARD.decode(pub_b64).map_err(|e| format!("公钥解码失败: {e}"))?;

    // 私钥 blob（按算法）
    let mut priv_blob = Vec::new();
    match key.key_data() {
        KeypairData::Rsa(rkp) => {
            put_wire_mpint(&mut priv_blob, rkp.private.d.as_bytes());
            put_wire_mpint(&mut priv_blob, rkp.private.p.as_bytes());
            put_wire_mpint(&mut priv_blob, rkp.private.q.as_bytes());
            put_wire_mpint(&mut priv_blob, rkp.private.iqmp.as_bytes());
        }
        KeypairData::Ed25519(ek) => {
            // string(seed 32 字节)
            let sk = ed25519_dalek::SigningKey::try_from(ek).map_err(|e| format!("Ed25519 转换失败: {e}"))?;
            put_wire_str(&mut priv_blob, sk.as_bytes());
        }
        KeypairData::Ecdsa(EcdsaKeypair::NistP256 { private, .. }) => {
            put_wire_mpint(&mut priv_blob, private.as_slice());
        }
        KeypairData::Ecdsa(EcdsaKeypair::NistP384 { private, .. }) => {
            put_wire_mpint(&mut priv_blob, private.as_slice());
        }
        KeypairData::Ecdsa(EcdsaKeypair::NistP521 { private, .. }) => {
            put_wire_mpint(&mut priv_blob, private.as_slice());
        }
        _ => return Err("该密钥类型不支持导出为 PuTTY .ppk 格式".into()),
    }

    // MAC：HMAC-SHA256(空 key, u32l(algo)+u32l(enc)+u32l(comment)+u32l(pub)+u32l(priv))
    let mut mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice(&[])
        .map_err(|e| format!("MAC 初始化失败: {e}"))?;
    let algo_name = match algo {
        ssh_key::Algorithm::Rsa { .. } => "ssh-rsa",
        ssh_key::Algorithm::Ed25519 => "ssh-ed25519",
        ssh_key::Algorithm::Ecdsa { curve } => match curve {
            ssh_key::EcdsaCurve::NistP256 => "ecdsa-sha2-nistp256",
            ssh_key::EcdsaCurve::NistP384 => "ecdsa-sha2-nistp384",
            ssh_key::EcdsaCurve::NistP521 => "ecdsa-sha2-nistp521",
        },
        _ => "ssh-unknown",
    };
    mac.update(&(algo_name.len() as u32).to_be_bytes());
    mac.update(algo_name.as_bytes());
    mac.update(&("none".len() as u32).to_be_bytes());
    mac.update(b"none");
    mac.update(&(key.comment().len() as u32).to_be_bytes());
    mac.update(key.comment().as_bytes());
    mac.update(&(pub_blob.len() as u32).to_be_bytes());
    mac.update(&pub_blob);
    mac.update(&(priv_blob.len() as u32).to_be_bytes());
    mac.update(&priv_blob);
    let mac_hex = hex_lower(mac.finalize().into_bytes().as_slice());

    Ok(format!(
        "PuTTY-User-Key-File-3: {algo_name}\nEncryption: none\nComment: {}\nPublic-Lines: {}\n{}\nPrivate-Lines: {}\n{}\nPrivate-MAC: {mac_hex}\n",
        key.comment(),
        b64_lines(&pub_blob).len(),
        b64_lines(&pub_blob).join("\n"),
        b64_lines(&priv_blob).len(),
        b64_lines(&priv_blob).join("\n"),
    ))
}

fn b64_lines(data: &[u8]) -> Vec<String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    b64.as_bytes().chunks(64).map(|c| String::from_utf8_lossy(c).to_string()).collect()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 把前端给定的文本写到选定路径（公钥导出用）
#[tauri::command]
pub fn user_key_write_text(path: String, content: String) -> Result<(), String> {
    write_text(std::path::Path::new(&path), &content)
}
