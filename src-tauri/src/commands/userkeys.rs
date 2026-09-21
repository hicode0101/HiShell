//! 用户密钥管理：扫描 ~/.ssh 下的公钥（.pub），解析类型/长度/备注/指纹。
//! 生成 / 导入 / 导出功能按需求后续接入。

use base64::Engine;
use serde::Serialize;
use sha2::Digest;

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
