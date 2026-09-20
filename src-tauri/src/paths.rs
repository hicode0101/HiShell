//! 跨平台配置目录与文件读写（Windows %APPDATA% / macOS ~/Library/Application Support / Linux XDG）

use std::fs;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// 应用配置目录（hishell/）
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hishell")
}

pub fn ensure_config_dir() -> AppResult<PathBuf> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| AppError::msg(format!("无法创建配置目录 {}: {e}", dir.display())))?;
    Ok(dir)
}

/// 读取 JSON 文件；文件不存在时返回 None
pub fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> AppResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => {
            let v = serde_json::from_slice(&bytes)
                .map_err(|e| AppError::msg(format!("解析 {} 失败: {e}", path.display())))?;
            Ok(Some(v))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::Io(e)),
    }
}

/// 原子写入 JSON（先写 .tmp 再 rename，借鉴 Tabby 双写思路的简化版）
pub fn write_json<T: serde::Serialize>(path: &PathBuf, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &bytes)?;
    // Windows 上 rename 不能覆盖已存在文件，先移除
    let _ = fs::remove_file(path);
    fs::rename(&tmp, path)?;
    // 备份副本
    let backup = path.with_extension("backup");
    let _ = fs::copy(path, &backup);
    Ok(())
}
