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
    Ok(profile_store::load())
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
