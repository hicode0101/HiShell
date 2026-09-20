use std::sync::atomic::Ordering;

use serde::Deserialize;
use tauri::{AppHandle, State};

use crate::profile_store;
use crate::ssh::{self, Sessions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAnswer {
    pub key: String,
    #[serde(default)]
    pub value: serde_json::Value,
}

/// 发起连接：返回会话 id（连接过程异步，状态经 ssh-status 事件推送）
#[tauri::command]
pub async fn ssh_connect(app: AppHandle, sessions: State<'_, Sessions>, profile_id: String) -> Result<u32, String> {
    let profile =
        profile_store::by_id(&profile_id).ok_or_else(|| format!("连接配置不存在: {profile_id}"))?;
    Ok(ssh::spawn_connect(app, sessions.0.clone(), profile))
}

/// 应答前端提示（hostkey 保存/一次性/取消；密码；KI 答案）
#[tauri::command]
pub async fn ssh_prompt_response(
    sessions: State<'_, Sessions>,
    sid: u32,
    answer: PromptAnswer,
) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let mut pending = session.shared.pending.lock().unwrap();
    if let Some(tx) = pending.remove(&answer.key) {
        let _ = tx.send(answer.value);
    }
    Ok(())
}

#[tauri::command]
pub async fn ssh_write(sessions: State<'_, Sessions>, sid: u32, data: String) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let bytes = ssh::manager::b64_decode(&data).map_err(|e| e.to_string())?;
    session.write_tx.send(bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ssh_resize(sessions: State<'_, Sessions>, sid: u32, cols: u32, rows: u32) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let _ = session.resize_tx.send((cols, rows));
    Ok(())
}

#[tauri::command]
pub async fn ssh_disconnect(app: AppHandle, sessions: State<'_, Sessions>, sid: u32) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    session.closed.store(true, Ordering::Relaxed);
    let mut guard = session.handle.lock().await;
    if let Some(h) = guard.as_mut() {
        let _ = h.disconnect(russh::Disconnect::ByApplication, "user disconnect", "en").await;
    }
    ssh::cleanup_session(&sessions.0, sid, &app, "用户断开");
    Ok(())
}
