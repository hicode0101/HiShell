use serde::Deserialize;
use tauri::State;

use crate::profile_store::ForwardRule;
use crate::ssh::{forward, Sessions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRequest {
    pub sid: u32,
    pub rule: ForwardRule,
}

/// 启动一条转发（会话需已连接）
#[tauri::command]
pub async fn forward_add(sessions: State<'_, Sessions>, req: ForwardRequest) -> Result<(), String> {
    let Some(session) = sessions.0.get(req.sid) else { return Err("会话不存在".into()) };
    forward::start(session, req.rule).await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRemoveRequest {
    pub sid: u32,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
}

/// 停止并移除转发
#[tauri::command]
pub async fn forward_remove(sessions: State<'_, Sessions>, req: ForwardRemoveRequest) -> Result<(), String> {
    let Some(session) = sessions.0.get(req.sid) else { return Err("会话不存在".into()) };
    session.forwards.stop(&req.rule_id, &session.handle).await;
    Ok(())
}

/// 列出运行中的转发
#[tauri::command]
pub async fn forward_list(sessions: State<'_, Sessions>, sid: u32) -> Result<Vec<forward::ForwardStatus>, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    Ok(session.forwards.list())
}
