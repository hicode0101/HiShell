use serde::Serialize;

use crate::error::AppResult;
use crate::profile_store::{self, Profile, SecretsIn};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOut {
    #[serde(flatten)]
    pub profile: Profile,
}

#[tauri::command]
pub fn profile_list() -> Result<Vec<Profile>, String> {
    // 快速连接的临时配置不进入任何列表（连接期间凭据仍可按 id 读取）
    Ok(profile_store::load().into_iter().filter(|p| !p.temporary).collect())
}

#[tauri::command]
pub fn profile_get(id: String) -> Result<Option<Profile>, String> {
    Ok(profile_store::by_id(&id))
}

/// 保存连接（明文凭据经此剥离进 OS 钥匙串；None = 保持已存值）
#[tauri::command]
pub fn profile_save(profile: Profile, secrets: Option<SecretsIn>) -> Result<Profile, String> {
    profile_save_impl(profile, secrets).map_err(|e| e.to_string())
}

fn profile_save_impl(profile: Profile, secrets: Option<SecretsIn>) -> AppResult<Profile> {
    profile_store::upsert_with_secrets(profile, &secrets.unwrap_or_default())
}

#[tauri::command]
pub fn profile_delete(id: String) -> Result<(), String> {
    profile_store::delete(&id).map_err(|e| e.to_string())
}

/// 按给定 id 顺序重排连接配置（会话管理器拖拽排序）
#[tauri::command]
pub fn profile_reorder(ids: Vec<String>) -> Result<(), String> {
    profile_store::reorder(&ids).map_err(|e| e.to_string())
}

/// 复制连接配置：新 id + 指定名称（重名自动递增），并同步复制系统凭据（密码/口令/代理密码）
#[tauri::command]
pub fn profile_copy(source_id: String, new_name: String) -> Result<Profile, String> {
    use rand::Rng;

    let src = profile_store::by_id(&source_id).ok_or("源配置不存在")?;
    let list = profile_store::load();

    let mut name = new_name.trim().to_string();
    if name.is_empty() {
        name = format!("{}-copy", src.name);
    }
    let mut n = 2;
    while list.iter().any(|p| p.name == name) {
        name = format!("{} ({})", new_name.trim(), n);
        n += 1;
    }

    let mut copy = src.clone();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let rnd: u32 = rand::thread_rng().gen_range(0x1000..0xffff);
    copy.id = format!("profile-{ts}-{rnd:x}");
    copy.name = name;

    // 同步复制凭据到新 id，凭据标记随 bundle 状态刷新
    if let Some(bundle) = crate::secrets::get(&source_id) {
        let _ = crate::secrets::set(&copy.id, &bundle);
    }

    profile_store::upsert_with_secrets(copy, &SecretsIn::default()).map_err(|e| e.to_string())
}

/* ---------------- 会话配置导出 / 导入（HiShell 自有格式） ---------------- */

/// 导出文件信封：自带格式标识与版本，便于将来格式演进时兼容识别
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportFile {
    pub app: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: u64,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileExportResult {
    pub exported: usize,
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileImportResult {
    pub imported: usize,
    pub skipped: usize,
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 导出全部连接配置到 JSON 文件（快速连接的临时配置除外）。
/// 密码等凭据保存在 OS 钥匙串、本就不落盘，导出前再清一遍敏感字段作双保险。
#[tauri::command]
pub fn profile_export(path: String) -> Result<ProfileExportResult, String> {
    let profiles: Vec<Profile> = profile_store::load()
        .into_iter()
        .filter(|p| !p.temporary)
        .map(|mut p| {
            p.password.clear();
            p.private_key_passphrase.clear();
            p
        })
        .collect();
    let exported = profiles.len();
    let file = ProfileExportFile {
        app: "HiShell".into(),
        kind: "hishell-profiles".into(),
        version: 1,
        exported_at: now_millis() as u64,
        profiles,
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(ProfileExportResult { exported, path })
}

/// 从 JSON 文件导入连接配置：按 id 与 host+port+username 去重，重名自动递增。
/// 凭据不随文件迁移（"已存凭据"标记一律清除），导入后连接时按需重新输入。
#[tauri::command]
pub fn profile_import(path: String) -> Result<ProfileImportResult, String> {
    use rand::Rng;

    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // 兼容带信封的导出文件与裸 Profile 数组两种形态
    let incoming: Vec<Profile> = match serde_json::from_str::<ProfileExportFile>(&raw) {
        Ok(file) => file.profiles,
        Err(_) => serde_json::from_str::<Vec<Profile>>(&raw).map_err(|e| e.to_string())?,
    };

    let mut list = profile_store::load();
    let mut imported = 0;
    let mut skipped = 0;
    for mut p in incoming {
        p.password.clear();
        p.private_key_passphrase.clear();
        p.has_stored_password = false;
        p.has_stored_passphrase = false;
        p.temporary = false;
        if p.host.trim().is_empty() || p.name.trim().is_empty() {
            skipped += 1;
            continue;
        }
        // 与现有配置按 id 或 host+port+username 重复的跳过（文件内重复项同样命中）
        if list.iter().any(|q| q.id == p.id)
            || list
                .iter()
                .any(|q| q.host == p.host && q.port == p.port && q.username == p.username)
        {
            skipped += 1;
            continue;
        }
        if p.id.is_empty() {
            let rnd: u32 = rand::thread_rng().gen_range(0x1000..0xffff);
            p.id = format!("profile-{}-{rnd:x}", now_millis());
        }
        let base = p.name.clone();
        let mut n = 2;
        while list.iter().any(|q| q.name == p.name) {
            p.name = format!("{base} ({n})");
            n += 1;
        }
        list.push(p);
        imported += 1;
    }
    if imported > 0 {
        profile_store::save(&list).map_err(|e| e.to_string())?;
    }
    Ok(ProfileImportResult { imported, skipped })
}
