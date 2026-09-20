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
