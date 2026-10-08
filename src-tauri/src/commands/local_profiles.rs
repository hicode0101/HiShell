//! 本地终端会话配置：会话名 + 默认工作路径。
//! 整表存配置目录 local_terminals.json（原子写 + 备份，复用 paths 助手），
//! 增删改与排序由前端整表提交保存。

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LocalTermSession {
    pub id: String,
    pub name: String,
    /// 默认工作路径；空串表示用主目录
    #[serde(default)]
    pub path: String,
}

fn store_path() -> std::path::PathBuf {
    paths::config_dir().join("local_terminals.json")
}

/// 读取全部本地终端会话（文件不存在视为空表）
#[tauri::command]
pub fn local_session_list() -> Result<Vec<LocalTermSession>, String> {
    match paths::read_json::<Vec<LocalTermSession>>(&store_path()) {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

/// 整表保存（前端完成增删改/排序后调用）
#[tauri::command]
pub fn local_session_save(items: Vec<LocalTermSession>) -> Result<(), String> {
    paths::write_json(&store_path(), &items).map_err(|e| e.to_string())
}
