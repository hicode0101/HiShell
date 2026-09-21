use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::ssh::{self, Sessions};
use russh_sftp::client::fs::DirEntry;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub mtime: u64,
    pub attrs: String,
}

fn attrs_string(permissions: Option<u32>) -> String {
    let Some(p) = permissions else { return String::new() };
    let file_type = match p & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };
    let mut s = String::with_capacity(10);
    s.push(file_type);
    for shift in [6u32, 3, 0] {
        let bits = (p >> shift) & 0o7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

fn map_entry(e: &DirEntry) -> RemoteEntry {
    let meta = e.metadata();
    let kind = if meta.is_dir() { "dir" } else { "file" };
    RemoteEntry {
        name: e.file_name(),
        kind: kind.into(),
        size: meta.size.unwrap_or(0),
        mtime: meta.mtime.map(|t| (t as u64) * 1000).unwrap_or(0),
        attrs: attrs_string(meta.permissions),
    }
}

/// 列出远程目录
#[tauri::command]
pub async fn sftp_list(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<Vec<RemoteEntry>, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    let entries = sftp.read_dir(&path).await.map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(|e| map_entry(&e)).collect())
}

#[tauri::command]
pub async fn sftp_mkdir(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    sftp.create_dir(&path).await.map_err(|e| e.to_string())
}

/// 删除文件/目录（目录递归）
#[tauri::command]
pub async fn sftp_rm(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    let meta = sftp.metadata(&path).await.map_err(|e| e.to_string())?;
    if meta.is_dir() {
        // 递归删除
        let entries = sftp.read_dir(&path).await.map_err(|e| e.to_string())?;
        for e in entries {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = format!("{}/{}", path.trim_end_matches('/'), name);
            let is_dir = e.metadata().is_dir();
            if is_dir {
                delete_recursive(sftp.clone(), child).await.map_err(|e| e.to_string())?;
            } else {
                sftp.remove_file(&child).await.map_err(|e| e.to_string())?;
            }
        }
        sftp.remove_dir(&path).await.map_err(|e| e.to_string())
    } else {
        sftp.remove_file(&path).await.map_err(|e| e.to_string())
    }
}

fn delete_recursive(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    dir: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh_sftp::client::error::Error>> + Send>> {
    Box::pin(delete_recursive_impl(sftp, dir))
}

async fn delete_recursive_impl(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    dir: String,
) -> Result<(), russh_sftp::client::error::Error> {
    let entries = sftp.read_dir(dir.clone()).await?;
    for e in entries {
        let name = e.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let child = format!("{}/{}", dir.trim_end_matches('/'), name);
        if e.metadata().is_dir() {
            delete_recursive(sftp.clone(), child.clone()).await?;
        } else {
            sftp.remove_file(&child).await?;
        }
    }
    sftp.remove_dir(dir).await
}

#[tauri::command]
pub async fn sftp_rename(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
    new_path: String,
) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    sftp.rename(&path, &new_path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sftp_stat(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
) -> Result<Option<RemoteEntry>, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    Ok(sftp.metadata(&path).await.ok().map(|m| RemoteEntry {
        name: path.split('/').last().unwrap_or("").to_string(),
        kind: if m.is_dir() { "dir".into() } else { "file".into() },
        size: m.size.unwrap_or(0),
        mtime: m.mtime.map(|t| (t as u64) * 1000).unwrap_or(0),
        attrs: attrs_string(m.permissions),
    }))
}

/// 远程路径是否存在 + 类型（供下载目录创建使用）
#[tauri::command]
pub async fn sftp_exists(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<bool, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    Ok(sftp.metadata(&path).await.is_ok())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MkdirsRequest {
    pub sid: u32,
    pub paths: Vec<String>,
}

/// 批量创建远程目录（递归上传文件夹前调用）
#[tauri::command]
pub async fn sftp_mkdirs(sessions: tauri::State<'_, Sessions>, sid: u32, paths: Vec<String>) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    for p in paths {
        if sftp.metadata(&p).await.is_ok() {
            continue;
        }
        // 逐级创建
        let mut partial = String::new();
        for seg in p.split('/').filter(|s| !s.is_empty()) {
            partial = format!("{}/{}", partial.trim_end_matches('/'), seg);
            if sftp.metadata(&partial).await.is_err() {
                sftp.create_dir(&partial).await.map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// 更改远程文件/目录权限（八进制，如 644；目录可递归）
#[tauri::command]
pub async fn sftp_chmod(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
    mode: u32,
    recursive: bool,
) -> Result<u32, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    if mode > 0o7777 {
        return Err("权限值无效（应为 0-7777 八进制）".into());
    }
    let count = chmod_recursive(sftp.clone(), path.clone(), mode, recursive)
        .await
        .map_err(|e| e.to_string())?;
    Ok(count)
}

fn chmod_recursive(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    path: String,
    mode: u32,
    recursive: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, String>> + Send>> {
    Box::pin(chmod_recursive_impl(sftp, path, mode, recursive))
}

async fn chmod_recursive_impl(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    path: String,
    mode: u32,
    recursive: bool,
) -> Result<u32, String> {
    // SFTP SETSTAT 的 permissions 携带完整 st_mode：保留原类型位，仅替换权限位
    let meta = sftp.metadata(&path).await.map_err(|e| format!("读取属性失败: {e}"))?;
    let type_bits = meta.permissions.unwrap_or(0o100000) & 0o170000;
    let attrs = russh_sftp::protocol::FileAttributes {
        permissions: Some(type_bits | (mode & 0o7777)),
        ..Default::default()
    };
    sftp.set_metadata(&path, attrs)
        .await
        .map_err(|e| format!("更改权限失败: {e}"))?;
    let mut count = 1u32;
    if recursive && meta.is_dir() {
        let entries = sftp.read_dir(&path).await.map_err(|e| format!("读取目录失败: {e}"))?;
        for e in entries {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = format!("{}/{}", path.trim_end_matches('/'), name);
            count += chmod_recursive(sftp.clone(), child, mode, true).await?;
        }
    }
    Ok(count)
}

/// 上传/下载前确保本地父目录存在（下载递归时使用）
#[tauri::command]
pub async fn local_mkdirs(paths: Vec<String>) -> Result<(), String> {
    for p in paths {
        tokio::fs::create_dir_all(&p)
            .await
            .map_err(|e| AppError::msg(format!("创建目录 {p} 失败: {e}")))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
