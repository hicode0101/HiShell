use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::ssh::{self, Sessions, Transfers};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueItem {
    pub sid: u32,
    pub direction: String, // upload | download
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(rename = "localPath")]
    pub local_path: String,
    #[serde(rename = "remotePath")]
    pub remote_path: String,
    /// 断点续传：从已传字节数处继续（对齐目标端已有的部分文件）
    #[serde(default)]
    pub resume: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enqueued {
    pub tid: u64,
    #[serde(rename = "localPath")]
    pub local_path: String,
    #[serde(rename = "remotePath")]
    pub remote_path: String,
    pub size: u64,
}

/// 入队传输：目录会被展开为多个文件任务
#[tauri::command]
pub async fn transfer_enqueue(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    transfers: State<'_, Transfers>,
    item: EnqueueItem,
) -> Result<Vec<Enqueued>, String> {
    let Some(session) = sessions.0.get(item.sid) else { return Err("会话不存在".into()) };
    let expanded =
        ssh::expand_entries(&session, &item.direction, &item.local_path, &item.remote_path)
            .await
            .map_err(|e| e.to_string())?;

    // 目录下载：先创建本地目录树
    if item.direction == "download" && !expanded.is_empty() {
        let mut dirs: Vec<String> = expanded
            .iter()
            .map(|(l, _, _)| {
                std::path::Path::new(l)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect();
        dirs.sort();
        dirs.dedup();
        for d in dirs {
            let _ = tokio::fs::create_dir_all(&d).await;
        }
    }
    if item.direction == "upload" && item.is_dir {
        // 远程目录树：由前端收集或按 local 相对路径推导
        let mut remote_dirs: Vec<String> = expanded
            .iter()
            .map(|(_, r, _)| {
                std::path::Path::new(r)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect();
        remote_dirs.sort();
        remote_dirs.dedup();
        let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
        for d in remote_dirs {
            let mut partial = String::new();
            for seg in d.split('/').filter(|s| !s.is_empty()) {
                partial = format!("{}/{}", partial.trim_end_matches('/'), seg);
                if sftp.metadata(&partial).await.is_err() {
                    let _ = sftp.create_dir(&partial).await;
                }
            }
        }
    }

    let mut out = Vec::new();
    for (local, remote, size) in expanded {
        let tid = transfers.0.new_id();
        let state = Arc::new(ssh::TransferState {
            total: AtomicU64::new(size),
            transferred: Default::default(),
            paused: Default::default(),
            cancelled: Default::default(),
        });
        transfers.0.items.lock().unwrap().insert(tid, state.clone());
        let app2 = app.clone();
        let transfers2 = transfers.0.clone();
        let session2 = session.clone();
        let direction = item.direction.clone();
        let resume = item.resume;
        tokio::spawn(ssh::run_transfer(app2, transfers2, session2, tid, direction, local.clone(), remote.clone(), resume));
        out.push(Enqueued { tid, local_path: local, remote_path: remote, size });
    }
    Ok(out)
}

#[tauri::command]
pub async fn transfer_pause(transfers: State<'_, Transfers>, tid: u64) -> Result<(), String> {
    if let Some(state) = transfers.0.get(tid) {
        state.paused.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("任务不存在".into())
    }
}

#[tauri::command]
pub async fn transfer_resume(transfers: State<'_, Transfers>, tid: u64) -> Result<(), String> {
    if let Some(state) = transfers.0.get(tid) {
        state.paused.store(false, Ordering::Relaxed);
        Ok(())
    } else {
        Err("任务不存在".into())
    }
}

#[tauri::command]
pub async fn transfer_cancel(transfers: State<'_, Transfers>, tid: u64) -> Result<(), String> {
    if let Some(state) = transfers.0.get(tid) {
        state.cancelled.store(true, Ordering::Relaxed);
        state.paused.store(false, Ordering::Relaxed);
        Ok(())
    } else {
        Err("任务不存在".into())
    }
}

#[allow(unused)]
fn unused(e: AppError) -> String {
    e.to_string()
}
