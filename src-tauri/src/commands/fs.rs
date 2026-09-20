use serde::Serialize;
use std::fs;
use std::time::UNIX_EPOCH;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryDto {
    pub name: String,
    /// "dir" | "file" | "symlink"
    pub kind: String,
    pub size: u64,
    /// Unix 毫秒时间戳
    pub mtime: u64,
    /// 远程权限串（如 drwxr-xr-x），本地为空
    pub attrs: String,
}

fn entry_dto(name: String, meta: &fs::Metadata) -> DirEntryDto {
    let kind = if meta.is_dir() {
        "dir"
    } else if meta.file_type().is_symlink() {
        "symlink"
    } else {
        "file"
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    DirEntryDto { name, kind: kind.to_string(), size: meta.len(), mtime, attrs: String::new() }
}

/// 列出本地目录；path 为空时返回盘符列表（Windows）
#[tauri::command]
pub fn fs_list_dir(path: String) -> Result<Vec<DirEntryDto>, String> {
    fs_list_dir_impl(path).map_err(|e| e.to_string())
}

fn fs_list_dir_impl(path: String) -> AppResult<Vec<DirEntryDto>> {
    if path.trim().is_empty() {
        return Ok(list_drives());
    }
    let entries = fs::read_dir(&path).map_err(|e| AppError::msg(format!("无法读取目录 {path}: {e}")))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        out.push(entry_dto(name, &meta));
    }
    Ok(out)
}

#[cfg(windows)]
fn list_drives() -> Vec<DirEntryDto> {
    let mut drives = Vec::new();
    for letter in b'A'..=b'Z' {
        let path = format!("{}:\\", letter as char);
        if std::path::Path::new(&path).exists() {
            drives.push(DirEntryDto {
                name: format!("本地磁盘 ({}:)", letter as char),
                kind: "dir".into(),
                size: 0,
                mtime: 0,
                attrs: String::new(),
            });
        }
    }
    drives
}

#[cfg(not(windows))]
fn list_drives() -> Vec<DirEntryDto> {
    vec![DirEntryDto { name: "/".into(), kind: "dir".into(), size: 0, mtime: 0, attrs: String::new() }]
}

/// 系统特殊目录（桌面/文档/下载等），本地窗格快速链接
#[tauri::command]
pub fn fs_special_dir(kind: String) -> Result<String, String> {
    let path = match kind.as_str() {
        "home" => dirs::home_dir(),
        "desktop" => dirs::desktop_dir().or_else(dirs::home_dir),
        "documents" => dirs::document_dir().or_else(dirs::home_dir),
        "downloads" => dirs::download_dir().or_else(dirs::home_dir),
        "pictures" => dirs::picture_dir().or_else(dirs::home_dir),
        "music" => dirs::audio_dir().or_else(dirs::home_dir),
        "videos" => dirs::video_dir().or_else(dirs::home_dir),
        _ => None,
    };
    path.map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| format!("无法解析特殊目录: {kind}"))
}
