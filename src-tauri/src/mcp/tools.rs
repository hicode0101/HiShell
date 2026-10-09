//! MCP 工具集：把 HiShell 的 SSH 能力暴露给 AI。
//! 铁律：所有工具只允许操作 mcpAuthorized = true 且非临时（temporary）的连接配置。

use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::logger;
use crate::profile_store::{self, ForwardRule, Profile};
use crate::ssh::{self, forward, SessionManager};

/// 单条输出上限（防止海量输出撑爆 AI 上下文）
const OUTPUT_CAP: usize = 512 * 1024;
/// exec 默认/最大超时
const EXEC_TIMEOUT_DEFAULT_MS: u64 = 60_000;
const EXEC_TIMEOUT_MAX_MS: u64 = 600_000;
/// connect 总等待上限（hostkey/密码确认可能需要用户在 HiShell 界面人工应答）
const CONNECT_WAIT_MAX: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileArgs {
    profile_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecArgs {
    profile_id: String,
    command: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferArgs {
    profile_id: String,
    local_path: String,
    remote_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForwardArgs {
    profile_id: String,
    /// local | remote | dynamic
    #[serde(rename = "type")]
    kind: String,
    bind_host: String,
    bind_port: u16,
    #[serde(default)]
    target_host: Option<String>,
    #[serde(default)]
    target_port: Option<u16>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForwardRemoveArgs {
    profile_id: String,
    forward_id: String,
}

static FORWARD_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn gen_forward_id() -> String {
    use std::sync::atomic::Ordering;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = FORWARD_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("fwd-mcp-{ts}-{seq}")
}

/// 校验该连接配置允许被 MCP 操作，返回 Profile
fn authorized_profile(profile_id: &str) -> Result<Profile, String> {
    let p = profile_store::by_id(profile_id).ok_or_else(|| format!("连接配置不存在: {profile_id}"))?;
    if p.temporary {
        return Err("快速连接的临时配置不参与 MCP 控制".into());
    }
    if !p.mcp_authorized {
        return Err(format!(
            "连接「{}」未授权 MCP 控制。请在 HiShell 的连接配置中勾选「授权 MCP 完全控制」",
            p.name
        ));
    }
    Ok(p)
}

/// 找到某配置当前已连接的会话（取第一个：同配置多开时语义等价）
async fn connected_session(mgr: &Arc<SessionManager>, profile_id: &str) -> Result<Arc<crate::ssh::manager::SessionHandle>, String> {
    for sid in mgr.list_ids() {
        if let Some(s) = mgr.get(sid) {
            if s.profile_id == profile_id && !s.closed.load(Ordering::Relaxed) {
                let connected = { s.handle.lock().await.is_some() };
                if connected {
                    return Ok(s);
                }
            }
        }
    }
    Err(format!(
        "该连接当前没有已建立的会话，请先调用 connect_session（profileId={profile_id}）"
    ))
}

/// 工具定义（JSON Schema 描述，MCP tools/list 用）
pub fn definitions() -> serde_json::Value {
    json!([
        {
            "name": "list_sessions",
            "description": "列出 HiShell 中已授权给 MCP 控制的 SSH 连接配置及其连接状态。返回 id、名称、主机、端口、用户名、认证方式和当前是否已连接。",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "connect_session",
            "description": "建立到指定服务器的 SSH 连接（需要连接配置已授权 MCP 控制）。若主机密钥未确认或凭据缺失，HiShell 界面会弹出确认/认证对话框，等待用户人工应答，因此本调用可能耗时较长。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id（来自 list_sessions）" }
                },
                "required": ["profileId"]
            }
        },
        {
            "name": "disconnect_session",
            "description": "断开指定连接配置的所有 SSH 会话（包括 HiShell 界面里打开的该配置的会话）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" }
                },
                "required": ["profileId"]
            }
        },
        {
            "name": "exec_command",
            "description": "在已连接的服务器上执行一条 shell 命令并返回 stdout/stderr/退出码。要求该连接已通过 connect_session 建立会话。超时后返回已收到的部分输出并标记 truncated。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" },
                    "command": { "type": "string", "description": "要执行的 shell 命令" },
                    "timeoutMs": { "type": "number", "description": "超时毫秒数，默认 60000，最大 600000" }
                },
                "required": ["profileId", "command"]
            }
        },
        {
            "name": "upload_file",
            "description": "通过 SFTP 把本机（运行 HiShell 的机器）文件上传到服务器。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" },
                    "localPath": { "type": "string", "description": "本机文件路径" },
                    "remotePath": { "type": "string", "description": "服务器目标路径" }
                },
                "required": ["profileId", "localPath", "remotePath"]
            }
        },
        {
            "name": "download_file",
            "description": "通过 SFTP 把服务器文件下载到本机（运行 HiShell 的机器）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" },
                    "localPath": { "type": "string", "description": "本机保存路径" },
                    "remotePath": { "type": "string", "description": "服务器文件路径" }
                },
                "required": ["profileId", "localPath", "remotePath"]
            }
        },
        {
            "name": "list_forwards",
            "description": "列出该连接当前会话上运行中的端口转发规则及状态（active/error）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" }
                },
                "required": ["profileId"]
            }
        },
        {
            "name": "add_forward",
            "description": "在该连接的会话上添加端口转发（需已连接）。三种类型：local=本地转发（访问本机 bindHost:bindPort 即经 SSH 服务器访问 targetHost:targetPort）；remote=远程转发（SSH 服务器监听 bindHost:bindPort，入站流量转到本机 targetHost:targetPort）；dynamic=动态转发（本机 bindHost:bindPort 提供 SOCKS5 代理，无需目标）。监听端口被占用等失败会返回错误。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" },
                    "type": { "type": "string", "enum": ["local", "remote", "dynamic"], "description": "转发类型" },
                    "bindHost": { "type": "string", "description": "监听地址（通常 127.0.0.1；remote 时为服务器侧监听地址）" },
                    "bindPort": { "type": "number", "description": "监听端口 1–65535" },
                    "targetHost": { "type": "string", "description": "目标地址（local/remote 必填，dynamic 忽略；默认 127.0.0.1）" },
                    "targetPort": { "type": "number", "description": "目标端口（local/remote 必填 1–65535，dynamic 忽略）" },
                    "description": { "type": "string", "description": "备注（可选）" }
                },
                "required": ["profileId", "type", "bindHost", "bindPort"]
            }
        },
        {
            "name": "remove_forward",
            "description": "停止并移除一条端口转发（按 forwardId，来自 list_forwards 或 add_forward 的返回）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "profileId": { "type": "string", "description": "连接配置 id" },
                    "forwardId": { "type": "string", "description": "转发规则 id" }
                },
                "required": ["profileId", "forwardId"]
            }
        },
    ])
}

static CALL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// 工具调用入口：向 UI 发实时活动事件（mcp-activity，start/done 两阶段，
/// 前端按 callId 配对展示"AI 操作动态"），随后执行工具。
pub async fn call(app: &AppHandle, mgr: &Arc<SessionManager>, name: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    let call_id = CALL_SEQ.fetch_add(1, Ordering::Relaxed);
    let started = std::time::Instant::now();
    let _ = app.emit(
        "mcp-activity",
        json!({ "callId": call_id, "phase": "start", "tool": name, "args": args }),
    );
    let result = call_inner(app, mgr, name, args).await;
    let ms = started.elapsed().as_millis() as u64;
    let done = match &result {
        Ok(_) => json!({ "callId": call_id, "phase": "done", "tool": name, "args": args, "ok": true, "ms": ms }),
        Err(e) => json!({ "callId": call_id, "phase": "done", "tool": name, "args": args, "ok": false, "error": e, "ms": ms }),
    };
    let _ = app.emit("mcp-activity", done);
    result
}

async fn call_inner(app: &AppHandle, mgr: &Arc<SessionManager>, name: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match name {
        "list_sessions" => list_sessions(mgr).await,
        "connect_session" => {
            let a: ProfileArgs = parse_args(args)?;
            connect_session(app, mgr, &a.profile_id).await
        }
        "disconnect_session" => {
            let a: ProfileArgs = parse_args(args)?;
            disconnect_session(app, mgr, &a.profile_id).await
        }
        "exec_command" => {
            let a: ExecArgs = parse_args(args)?;
            exec_command(mgr, &a.profile_id, &a.command, a.timeout_ms).await
        }
        "upload_file" => {
            let a: TransferArgs = parse_args(args)?;
            transfer_file(mgr, &a.profile_id, &a.local_path, &a.remote_path, true).await
        }
        "download_file" => {
            let a: TransferArgs = parse_args(args)?;
            transfer_file(mgr, &a.profile_id, &a.local_path, &a.remote_path, false).await
        }
        "list_forwards" => {
            let a: ProfileArgs = parse_args(args)?;
            list_forwards(mgr, &a.profile_id).await
        }
        "add_forward" => {
            let a: ForwardArgs = parse_args(args)?;
            add_forward(mgr, &a).await
        }
        "remove_forward" => {
            let a: ForwardRemoveArgs = parse_args(args)?;
            remove_forward(mgr, &a.profile_id, &a.forward_id).await
        }
        _ => Err(format!("未知工具: {name}")),
    }
}

fn parse_args<T: for<'de> Deserialize<'de>>(args: &serde_json::Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("参数错误: {e}"))
}

/* ---------------- list_sessions ---------------- */

async fn list_sessions(mgr: &Arc<SessionManager>) -> Result<serde_json::Value, String> {
    let connected_ids: Vec<String> = {
        let mut v = Vec::new();
        for sid in mgr.list_ids() {
            if let Some(s) = mgr.get(sid) {
                let live = { s.handle.lock().await.is_some() } && !s.closed.load(Ordering::Relaxed);
                if live {
                    v.push(s.profile_id.clone());
                }
            }
        }
        v
    };
    let mut list: Vec<serde_json::Value> = profile_store::load()
        .into_iter()
        .filter(|p| p.mcp_authorized && !p.temporary)
        .map(|p| {
            json!({
                "profileId": p.id,
                "name": p.name,
                "host": p.host,
                "port": p.port,
                "username": p.username,
                "auth": p.auth,
                "connected": connected_ids.contains(&p.id),
            })
        })
        .collect();
    list.sort_by(|a, b| {
        a.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(b.get("name").and_then(|v| v.as_str()).unwrap_or(""))
    });
    Ok(json!({ "sessions": list, "count": list.len() }))
}

/* ---------------- connect_session ---------------- */

async fn connect_session(app: &AppHandle, mgr: &Arc<SessionManager>, profile_id: &str) -> Result<serde_json::Value, String> {
    let profile = authorized_profile(profile_id)?;

    // 同配置已有活会话则直接复用（幂等）
    if let Ok(s) = connected_session(mgr, profile_id).await {
        return Ok(json!({
            "sid": s.sid, "host": profile.host, "port": profile.port,
            "username": profile.username, "message": "已有连接，直接复用",
        }));
    }

    let sid = ssh::spawn_connect(app.clone(), mgr.clone(), profile.clone());
    logger::log(&format!(
        "MCP connect sid={sid} profile={} -> {}:{} user={}",
        profile.id, profile.host, profile.port, profile.username
    ));

    // 轮询等待连接完成 / 失败（失败时后端会把会话从管理器移除）。
    // 注意：会话句柄在 run_session 任务内异步注册，刚 spawn 完可能还没入库，
    // 用 ever_seen 区分「调度延迟」与「真失败」（见过后消失才算失败）。
    let start = Instant::now();
    let mut ever_seen = false;
    let deadline = Instant::now() + CONNECT_WAIT_MAX;
    loop {
        match mgr.get(sid) {
            Some(s) => {
                ever_seen = true;
                let connected = { s.handle.lock().await.is_some() };
                if connected {
                    return Ok(json!({
                        "sid": sid, "host": profile.host, "port": profile.port,
                        "username": profile.username, "message": "连接成功",
                    }));
                }
            }
            None => {
                if ever_seen || start.elapsed() > Duration::from_secs(5) {
                    return Err(format!(
                        "连接失败（会话已终止，sid={sid}）。常见原因：凭据错误、用户在 HiShell 界面取消了确认、网络不可达。详见 HiShell 日志。"
                    ));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err("连接等待超时（300s）：可能正在等待用户在 HiShell 界面确认主机密钥/输入凭据".into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/* ---------------- disconnect_session ---------------- */

async fn disconnect_session(app: &AppHandle, mgr: &Arc<SessionManager>, profile_id: &str) -> Result<serde_json::Value, String> {
    authorized_profile(profile_id)?;
    let sids: Vec<u32> = mgr
        .list_ids()
        .into_iter()
        .filter(|sid| mgr.get(*sid).map(|s| s.profile_id == profile_id).unwrap_or(false))
        .collect();
    let n = sids.len();
    for sid in sids {
        if let Some(s) = mgr.get(sid) {
            s.closed.store(true, Ordering::Relaxed);
            if !s.is_alias {
                let mut guard = s.handle.lock().await;
                if let Some(h) = guard.as_mut() {
                    let _ = h.disconnect(russh::Disconnect::ByApplication, "mcp disconnect", "en").await;
                }
            }
            ssh::cleanup_session(mgr, sid, app, "MCP 断开");
            logger::log(&format!("MCP disconnect sid={sid} profile={profile_id}"));
        }
    }
    Ok(json!({ "disconnected": n }))
}

/* ---------------- exec_command ---------------- */

async fn exec_command(mgr: &Arc<SessionManager>, profile_id: &str, command: &str, timeout_ms: Option<u64>) -> Result<serde_json::Value, String> {
    authorized_profile(profile_id)?;
    let session = connected_session(mgr, profile_id).await?;
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(EXEC_TIMEOUT_DEFAULT_MS).clamp(1000, EXEC_TIMEOUT_MAX_MS));
    logger::log(&format!("MCP exec sid={} profile={} cmd={:?}", session.sid, profile_id, command));

    // 打开 exec 通道（非 PTY：输出干净，无回显/控制序列）
    let mut channel = {
        let mut guard = session.handle.lock().await;
        let handle = guard.as_mut().ok_or("会话未连接")?;
        let ch = handle.channel_open_session().await.map_err(|e| format!("打开通道失败: {e}"))?;
        ch.exec(true, command.to_string()).await.map_err(|e| format!("执行请求失败: {e}"))?;
        ch
    };

    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let mut out_truncated = false;
    let mut err_truncated = false;
    let mut exit_code: Option<u32> = None;
    let mut timed_out = false;
    let deadline = tokio::time::Instant::now() + timeout;
    // OpenSSH 消息顺序为 EOF → exit-status → close：EOF 后须继续等到 exit-status/close，
    // 仅加 1.5s 宽限防止个别服务器不发 close 导致挂起
    let mut eof_at: Option<tokio::time::Instant> = None;

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(russh::ChannelMsg::Data { ref data }) => {
                        if stdout.len() < OUTPUT_CAP {
                            stdout.extend_from_slice(data);
                        } else {
                            out_truncated = true;
                        }
                    }
                    Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                        if stderr.len() < OUTPUT_CAP {
                            stderr.extend_from_slice(data);
                        } else {
                            err_truncated = true;
                        }
                    }
                    Some(russh::ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status),
                    Some(russh::ChannelMsg::Eof) => {
                        if eof_at.is_none() {
                            eof_at = Some(tokio::time::Instant::now() + std::time::Duration::from_millis(1500));
                        }
                    }
                    Some(russh::ChannelMsg::Close) | None => break,
                    _ => {}
                }
            }
            _ = async { tokio::time::sleep_until(eof_at.unwrap()).await }, if eof_at.is_some() => break,
            _ = tokio::time::sleep_until(deadline) => {
                timed_out = true;
                let _ = channel.close().await;
                break;
            }
        }
    }

    let strip = |mut v: Vec<u8>| -> String {
        while v.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            v.pop();
        }
        String::from_utf8_lossy(&v).to_string()
    };

    Ok(json!({
        "exitCode": exit_code,
        "stdout": strip(stdout),
        "stderr": strip(stderr),
        "timedOut": timed_out,
        "truncated": out_truncated || err_truncated,
    }))
}

/* ---------------- upload / download ---------------- */

async fn transfer_file(mgr: &Arc<SessionManager>, profile_id: &str, local_path: &str, remote_path: &str, upload: bool) -> Result<serde_json::Value, String> {
    authorized_profile(profile_id)?;
    let session = connected_session(mgr, profile_id).await?;
    let dir = if upload { "上传" } else { "下载" };
    logger::log(&format!("MCP {dir} sid={} local={local_path} remote={remote_path}", session.sid));

    let sftp = ssh::get_sftp(&session).await.map_err(|e| format!("SFTP 初始化失败: {e}"))?;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    if upload {
        let meta = tokio::fs::metadata(local_path).await.map_err(|e| format!("本地文件不存在: {e}"))?;
        if meta.is_dir() {
            return Err("仅支持单文件上传（不支持目录）".into());
        }
        use russh_sftp::protocol::OpenFlags;
        let mut local = tokio::fs::File::open(local_path).await.map_err(|e| format!("打开本地文件失败: {e}"))?;
        let mut remote = sftp
            .open_with_flags(remote_path, OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE)
            .await
            .map_err(|e| format!("打开远程文件失败: {e}"))?;
        let n = tokio::io::copy(&mut local, &mut remote).await.map_err(|e| format!("传输失败: {e}"))?;
        let _ = remote.shutdown().await;
        Ok(json!({ "bytes": n, "message": format!("上传完成: {remote_path}") }))
    } else {
        let mut remote = sftp.open(remote_path).await.map_err(|e| format!("打开远程文件失败: {e}"))?;
        let mut local = tokio::fs::File::create(local_path).await.map_err(|e| format!("创建本地文件失败: {e}"))?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = remote.read(&mut buf).await.map_err(|e| format!("读取远程数据失败: {e}"))?;
            if n == 0 {
                break;
            }
            local.write_all(&buf[..n]).await.map_err(|e| format!("写入本地文件失败: {e}"))?;
            total += n as u64;
        }
        local.flush().await.map_err(|e| format!("写入本地文件失败: {e}"))?;
        let _ = remote.shutdown().await;
        Ok(json!({ "bytes": total, "message": format!("下载完成: {local_path}") }))
    }
}

/* ---------------- 端口转发（local / remote / dynamic） ---------------- */

async fn list_forwards(mgr: &Arc<SessionManager>, profile_id: &str) -> Result<serde_json::Value, String> {
    authorized_profile(profile_id)?;
    let session = connected_session(mgr, profile_id).await?;
    let list = session.forwards.list();
    let value = serde_json::to_value(&list).map_err(|e| e.to_string())?;
    Ok(json!({ "forwards": value, "count": list.len() }))
}

async fn add_forward(mgr: &Arc<SessionManager>, a: &ForwardArgs) -> Result<serde_json::Value, String> {
    let profile = authorized_profile(&a.profile_id)?;
    let session = connected_session(mgr, &a.profile_id).await?;

    if a.bind_port == 0 {
        return Err("bindPort 无效：应为 1–65535".into());
    }
    let bind_host = a.bind_host.trim().to_string();
    if bind_host.is_empty() {
        return Err("bindHost 不能为空".into());
    }
    let kind = match a.kind.as_str() {
        "local" | "remote" | "dynamic" => a.kind.clone(),
        other => return Err(format!("未知转发类型: {other}（支持 local / remote / dynamic）")),
    };
    // local/remote 必须有目标；dynamic 无目标（SOCKS5 由客户端请求时决定）
    let (target_host, target_port) = match kind.as_str() {
        "dynamic" => (None, None),
        _ => {
            let tp = a
                .target_port
                .filter(|p| *p > 0)
                .ok_or("local/remote 类型必须提供 targetPort（1–65535）")?;
            let th = a.target_host.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "127.0.0.1".into());
            (Some(th), Some(tp))
        }
    };
    // 同会话同监听去重（避免 AI 重复添加后端绑定冲突难排查）
    {
        let running = session.forwards.list();
        if running
            .iter()
            .any(|s| s.kind == kind && s.bind_host == bind_host && s.bind_port == a.bind_port)
        {
            return Err(format!("该会话已有相同监听的转发在运行：{kind} {bind_host}:{}", a.bind_port));
        }
    }

    let rule = ForwardRule {
        id: gen_forward_id(),
        kind,
        bind_host,
        bind_port: a.bind_port,
        target_host,
        target_port,
        description: a.description.clone(),
    };
    forward::start(session.clone(), rule.clone()).await.map_err(|e| e.to_string())?;
    logger::log(&format!(
        "MCP add_forward sid={} profile={} {} {:?}:{:?} -> {:?}:{:?}",
        session.sid, profile.id, rule.kind, rule.bind_host, rule.bind_port, rule.target_host, rule.target_port
    ));

    // 监听绑定失败是异步体现在 error 字段的：短暂等待后检查，失败即停掉并报错给 AI
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    match session.forwards.list().into_iter().find(|s| s.id == rule.id) {
        Some(s) if s.error.is_some() => {
            session.forwards.stop(&rule.id, &session.handle).await;
            Err(s.error.unwrap_or_else(|| "转发启动失败".into()))
        }
        Some(s) => Ok(json!({
            "forwardId": s.id,
            "type": s.kind,
            "bindHost": s.bind_host,
            "bindPort": s.bind_port,
            "targetHost": s.target_host,
            "targetPort": s.target_port,
            "active": s.active,
        })),
        None => Err("转发启动失败".into()),
    }
}

async fn remove_forward(mgr: &Arc<SessionManager>, profile_id: &str, forward_id: &str) -> Result<serde_json::Value, String> {
    authorized_profile(profile_id)?;
    let session = connected_session(mgr, profile_id).await?;
    if !session.forwards.is_running(forward_id) {
        return Err(format!("转发不存在（或已停止）: {forward_id}，可用 list_forwards 查看现有规则"));
    }
    session.forwards.stop(forward_id, &session.handle).await;
    logger::log(&format!("MCP remove_forward sid={} id={forward_id}", session.sid));
    Ok(json!({ "removed": forward_id }))
}
