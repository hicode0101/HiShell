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
    // 复制 SSH 隧道出的别名会话：只关自己的通道，绝不能发送 SSH DISCONNECT
    // （那会断开共享连接，父会话与兄弟别名全部掉线）
    if !session.is_alias {
        let mut guard = session.handle.lock().await;
        if let Some(h) = guard.as_mut() {
            let _ = h.disconnect(russh::Disconnect::ByApplication, "user disconnect", "en").await;
        }
    }
    ssh::cleanup_session(&sessions.0, sid, &app, "用户断开");
    Ok(())
}

/// 复制 SSH 隧道：在已有连接上开一个新 shell 通道，返回新 sid。
/// 不重新握手/认证；共享连接上下文与端口转发；父连接断开时别名一并掉线。
#[tauri::command]
pub async fn ssh_duplicate_channel(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    sid: u32,
) -> Result<u32, String> {
    let Some(parent) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    ssh::open_shell_alias(app, sessions.0.clone(), parent)
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    /// 凭据是否验证通过（无存储凭据时为 false 但 ok 可为 true）
    pub authenticated: bool,
    pub message: String,
}

/// 测试连接：真实走传输链（代理/ProxyCommand）+ SSH 握手 + 已存凭据认证尝试。
/// 自动接受主机密钥（不弹窗）；未存凭据时仅验证到握手层并在 message 中说明。
#[tauri::command]
pub async fn ssh_test(
    profile: crate::profile_store::Profile,
    password: Option<String>,
    passphrase: Option<String>,
) -> Result<TestResult, String> {
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex as StdMutex;

    let timeout_ms = profile.connect_timeout.max(1000);

    // userKey：按名称引用用户密钥管理中的密钥，解析为 ~/.ssh/<name> 的公钥认证
    let mut profile = profile;
    if profile.auth == "userKey" {
        profile.auth = "publicKey".into();
        if profile.private_key_path.trim().is_empty() {
            if let Some(home) = dirs::home_dir() {
                profile.private_key_path = home
                    .join(".ssh")
                    .join(&profile.user_key_name)
                    .to_string_lossy()
                    .to_string();
            }
        }
    }

    // 传输链（SOCKS5 / HTTP CONNECT / ProxyCommand / 直连）
    let transport = crate::ssh::transport::connect_transport(&profile, None)
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let stream = crate::ssh::transport::transport_to_stream(transport).map_err(|e| e.to_string())?;

    let shared = std::sync::Arc::new(ssh::manager::SessionShared {
        sid: 0,
        profile_id: profile.id.clone(),
        emit: std::sync::Arc::new(|_: &str, _: serde_json::Value| {}),
        host: profile.host.clone(),
        port: profile.port,
        user: profile.username.clone(),
        password: StdMutex::new(password.clone()),
        known_hosts: ssh::manager::KNOWN_HOSTS,
        pending: StdMutex::new(std::collections::HashMap::new()),
        forwarded_tx: StdMutex::new(None),
        closed: AtomicBool::new(false),
        auto_accept_hostkey: true,
    });

    let config = std::sync::Arc::new(russh::client::Config {
        keepalive_interval: Some(std::time::Duration::from_millis(profile.keepalive_interval.max(1000))),
        keepalive_max: profile.keepalive_count_max as usize,
        ..russh::client::Config::default()
    });
    let handler = ssh::manager::SshHandler { shared };
    let mut handle: russh::client::Handle<ssh::manager::SshHandler> =
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms * 2), russh::client::connect_stream(config, stream, handler))
            .await
            .map_err(|_| "握手超时".to_string())?
            .map_err(|e| format!("SSH 握手失败（{}:{}）：{e}", profile.host, profile.port))?;

    // 认证尝试：只用表单/已存凭据，绝不弹窗
    let bundle = crate::secrets::get(&profile.id).unwrap_or_default();
    let pwd = password.filter(|p| !p.is_empty()).or(bundle.password.clone());
    let mut authenticated = false;
    let mut notes: Vec<String> = Vec::new();

    if let Some(p) = pwd {
        match tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            handle.authenticate_password(&profile.username, &p),
        )
        .await
        {
            Ok(Ok(true)) => authenticated = true,
            Ok(Ok(false)) => notes.push("密码认证被拒绝".into()),
            Ok(Err(e)) => notes.push(format!("密码认证错误: {e}")),
            Err(_) => notes.push("密码认证超时".into()),
        }
    } else {
        notes.push("未提供密码".into());
    }

    if !authenticated && !profile.private_key_path.trim().is_empty() {
        let pass = passphrase.filter(|p| !p.is_empty()).or(bundle.passphrase.clone());
        let key_path = ssh::manager::expand_tilde(&profile.private_key_path);
        match russh::keys::load_secret_key(&key_path, pass.as_deref()) {
            Ok(key) => match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                handle.authenticate_publickey(&profile.username, std::sync::Arc::new(key)),
            )
            .await
            {
                Ok(Ok(true)) => authenticated = true,
                Ok(Ok(false)) => notes.push("公钥认证被拒绝".into()),
                Ok(Err(e)) => notes.push(format!("公钥认证错误: {e}")),
                Err(_) => notes.push("公钥认证超时".into()),
            },
            Err(e) => notes.push(format!("私钥加载失败: {e}")),
        }
    }

    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "test done", "en")
        .await;

    if authenticated {
        Ok(TestResult { ok: true, authenticated: true, message: "连接与认证成功".into() })
    } else if notes.iter().all(|n| n == "未提供密码") && profile.private_key_path.trim().is_empty() && profile.auth != "agent" {
        Ok(TestResult { ok: true, authenticated: false, message: "网络与握手正常，但未提供凭据，无法验证认证".into() })
    } else {
        Ok(TestResult { ok: false, authenticated: false, message: format!("网络与握手正常，但认证未通过（{}）", notes.join("；")) })
    }
}
