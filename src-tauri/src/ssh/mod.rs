//! SSH 会话生命周期 + SFTP 服务 + 传输引擎

pub mod forward;
pub mod manager;
pub mod transport;

pub use manager::SessionManager;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use russh::ChannelMsg;
use russh_sftp::client::SftpSession;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::profile_store::{self, ForwardRule, Profile};
use crate::secrets;

use manager::{SessionHandle, SessionShared, SshHandler};

/// 全局会话管理器（tauri State）
pub struct Sessions(pub Arc<SessionManager>);

/// 全局传输管理器（tauri State）
pub struct Transfers(pub Arc<TransferManager>);

fn emit_status(app: &AppHandle, sid: u32, status: &str, message: Option<&str>) {
    let _ = app.emit("ssh-status", serde_json::json!({ "sid": sid, "status": status, "message": message }));
}

/// 发起连接：立即返回 sid，连接过程异步执行（状态经 ssh-status 事件推送）。
/// 会话句柄在连接前就注册进管理器——否则握手期 hostkey/密码提示的应答无法送达。
pub fn spawn_connect(app: AppHandle, mgr: Arc<SessionManager>, profile: Profile) -> u32 {
    let sid = manager::next_sid();
    crate::logger::log(&format!("ssh_connect sid={sid} profile={} -> {}:{} user={}", profile.id, profile.host, profile.port, profile.username));
    tokio::spawn(async move {
        match run_session(app.clone(), mgr.clone(), sid, profile).await {
            Ok(()) => {}
            Err(e) => {
                // 连接失败：清理注册并通知前端（前端在终端内展示原因）
                cleanup_session(&mgr, sid, &app, &e.to_string());
            }
        }
    });
    sid
}

async fn run_session(app: AppHandle, mgr: Arc<SessionManager>, sid: u32, profile: Profile) -> AppResult<()> {
    let bundle = secrets::get(&profile.id).unwrap_or_default();

    let app_for_emit = app.clone();
    let shared = Arc::new(SessionShared {
        sid,
        profile_id: profile.id.clone(),
        emit: Arc::new(move |event: &str, payload: serde_json::Value| {
            let _ = app_for_emit.emit(event, payload);
        }),
        host: profile.host.clone(),
        port: profile.port,
        user: profile.username.clone(),
        password: Mutex::new(None),
        known_hosts: manager::KNOWN_HOSTS,
        pending: Mutex::new(std::collections::HashMap::new()),
        forwarded_tx: Mutex::new(None),
        closed: AtomicBool::new(false),
        auto_accept_hostkey: false,
    });

    // 先注册（handle 稍后填充），保证提示应答通道可用
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u32, u32)>();
    let session = Arc::new(SessionHandle {
        sid,
        profile_id: profile.id.clone(),
        shared: shared.clone(),
        handle: Arc::new(tokio::sync::Mutex::new(None)),
        closed: AtomicBool::new(false),
        write_tx,
        forwards: Arc::new(forward::ForwardManager::new()),
        sftp: Mutex::new(None),
        resize_tx,
        is_alias: false,
    });
    mgr.insert(session.clone());

    // 连接 + 认证（含 hostkey / 密码 / KI 前端交互）；连接期间可被“断开”取消
    tokio::select! {
        r = manager::connect_and_auth(shared.clone(), profile.clone()) => {
            *session.handle.lock().await = Some(r?);
        }
        _ = async {
            loop {
                if session.closed.load(Ordering::Relaxed) {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        } => {
            return Err(AppError::Cancelled);
        }
    }

    // 远程转发入站通道
    let (fwd_tx, fwd_rx) = mpsc::unbounded_channel();
    *shared.forwarded_tx.lock().unwrap() = Some(fwd_tx);

    // 打开 shell 通道（PTY + shell），语义对齐 Tabby openShellChannel
    crate::logger::log(&format!("sid={sid} 认证成功，打开 shell 通道"));
    let shell_channel = {
        let mut guard = session.handle.lock().await;
        let handle = guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
        let channel = handle.channel_open_session().await?;
        channel
            .request_pty(true, &profile.term, 80, 24, 0, 0, &[])
            .await
            .map_err(|e| AppError::msg(format!("PTY 请求失败: {e}")))?;
        channel
            .request_shell(true)
            .await
            .map_err(|e| AppError::msg(format!("shell 请求失败: {e}")))?;
        channel
    };

    // 写入序列化：make_writer 返回独立写端（先建 writer，再移交通道给数据泵）
    let mut writer = shell_channel.make_writer();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Some(bytes) = write_rx.recv().await {
            if writer.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = writer.shutdown().await;
    });

    // 会话级远程转发分发器
    tokio::spawn(forward::spawn_remote_dispatcher(session.clone(), fwd_rx));

    // 自动应用 profile 中预置的端口转发规则（对齐 Tabby 连接后恢复 forwardedPorts）
    for rule in profile.forwarded_ports.clone() {
        let _ = forward::start(session.clone(), rule).await;
    }

    crate::logger::log(&format!("sid={sid} shell 通道就绪，通知前端 connected"));
    emit_status(&app, sid, "connected", None);

    // 数据泵：读取 shell 通道 → 前端（ssh-data 事件，base64），并处理 resize
    pump_channel(app.clone(), session.clone(), shell_channel, resize_rx).await;

    cleanup_session(&mgr, sid, &app, "会话已关闭");
    Ok(())
}

/// 在已连接的会话上开一个新 shell 通道（"复制 SSH 隧道"）：
/// 复用同一条 SSH 连接（不重新握手），共享提示上下文与端口转发。
/// 返回新 sid，终端数据经 ssh-data 事件按新 sid 推送。
pub async fn open_shell_alias(
    app: AppHandle,
    mgr: Arc<SessionManager>,
    parent: Arc<SessionHandle>,
) -> AppResult<u32> {
    {
        let guard = parent.handle.lock().await;
        if guard.is_none() {
            return Err(AppError::msg("会话未连接"));
        }
    }
    let term = crate::profile_store::by_id(&parent.profile_id)
        .map(|p| p.term)
        .unwrap_or_else(|| "xterm-256color".into());

    let new_sid = manager::next_sid();
    crate::logger::log(&format!("sid={new_sid} 复制 SSH 隧道（父 sid={}）", parent.sid));

    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (resize_tx, resize_rx) = mpsc::unbounded_channel::<(u32, u32)>();
    let alias = Arc::new(SessionHandle {
        sid: new_sid,
        profile_id: parent.profile_id.clone(),
        shared: parent.shared.clone(),     // 同一条连接的共享上下文（含 closed 联动）
        handle: parent.handle.clone(),     // 复用同一 russh 连接，不重新握手
        closed: AtomicBool::new(false),
        write_tx,
        forwards: parent.forwards.clone(), // 共用端口转发
        sftp: Mutex::new(None),
        resize_tx,
        is_alias: true,
    });
    mgr.insert(alias.clone());

    // 在共享连接上打开新 shell 通道（PTY + shell）
    let shell_channel = {
        let mut guard = alias.handle.lock().await;
        let handle = guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
        let channel = handle.channel_open_session().await?;
        channel
            .request_pty(true, &term, 80, 24, 0, 0, &[])
            .await
            .map_err(|e| AppError::msg(format!("PTY 请求失败: {e}")))?;
        channel
            .request_shell(true)
            .await
            .map_err(|e| AppError::msg(format!("shell 请求失败: {e}")))?;
        channel
    };

    // 写入序列化
    let mut writer = shell_channel.make_writer();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Some(bytes) = write_rx.recv().await {
            if writer.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = writer.shutdown().await;
    });

    emit_status(&app, new_sid, "connected", None);

    // 数据泵：通道结束（自身关闭 / 父连接断开）后仅清理别名注册，不动共享转发
    tokio::spawn(async move {
        pump_channel(app.clone(), alias.clone(), shell_channel, resize_rx).await;
        cleanup_session(&mgr, new_sid, &app, "会话已关闭");
    });

    Ok(new_sid)
}

/// 数据泵：通道 → 前端（ssh-data 事件，base64），并处理 resize 请求
async fn pump_channel(
    app: AppHandle,
    session: Arc<SessionHandle>,
    mut channel: russh::Channel<russh::client::Msg>,
    mut resize_rx: mpsc::UnboundedReceiver<(u32, u32)>,
) {
    let sid = session.sid;
    crate::logger::log(&format!("sid={sid} 数据泵启动"));
    let mut data_count: u64 = 0;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                // 自身关闭，或共享连接断开（父会话断开时别名一并掉线）
                if session.closed.load(Ordering::Relaxed) || session.shared.closed.load(Ordering::Relaxed) {
                    break
                }
            }
            Some((cols, rows)) = resize_rx.recv() => {
                let _ = channel.window_change(cols, rows, 0, 0).await;
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        data_count += 1;
                        if data_count == 1 {
                            crate::logger::log(&format!("sid={sid} 收到首块数据 {} 字节", data.len()));
                        }
                        let _ = app.emit("ssh-data", serde_json::json!({
                            "sid": sid,
                            "data": base64::engine::general_purpose::STANDARD.encode(data),
                        }));
                    }
                    Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                        let _ = app.emit("ssh-data", serde_json::json!({
                            "sid": sid,
                            "data": base64::engine::general_purpose::STANDARD.encode(data),
                        }));
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        crate::logger::log(&format!("sid={sid} 通道结束（共 {data_count} 块数据）"));
                        break
                    }
                    _ => {}
                }
            }
        }
    }
}

/// 清理会话（状态通知 + 停转发 + 移除注册）。
/// 别名会话（复制 SSH 隧道）与父会话共享转发管理器：只摘除自身注册，
/// 不停止共享的转发（父会话断开时统一停止）。
pub fn cleanup_session(mgr: &Arc<SessionManager>, sid: u32, app: &AppHandle, message: &str) {
    if let Some(session) = mgr.remove(sid) {
        session.closed.store(true, Ordering::Relaxed);
        let handle = session.handle.clone();
        let forwards = session.forwards.clone();
        let is_alias = session.is_alias;
        tauri::async_runtime::spawn(async move {
            if !is_alias {
                forwards.stop_all(&handle).await;
            }
        });
    }
    emit_status(app, sid, "disconnected", Some(message));
}

/// SFTP 会话访问（懒创建，每会话缓存）
pub struct SftpSlot(pub Mutex<Option<Arc<SftpSession>>>);

pub async fn get_sftp(session: &Arc<SessionHandle>) -> AppResult<Arc<SftpSession>> {
    {
        let slot = session.sftp.lock().unwrap();
        if let Some(s) = slot.as_ref() {
            return Ok(s.clone());
        }
    }
    let mut handle_guard = session.handle.lock().await;
    let handle = handle_guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
    let channel = handle.channel_open_session().await?;
    // 关键：先请求 sftp 子系统，否则服务器不会启动 SFTP 服务
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| AppError::msg(format!("请求 sftp 子系统失败: {e}")))?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| AppError::msg(format!("SFTP 初始化失败: {e}")))?;
    let arc = Arc::new(sftp);
    *session.sftp.lock().unwrap() = Some(arc.clone());
    Ok(arc)
}

/* ---------------- 传输引擎 ---------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub tid: u64,
    pub transferred: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDone {
    pub tid: u64,
    pub ok: bool,
    pub error: Option<String>,
}

pub struct TransferState {
    pub total: AtomicU64,
    pub transferred: AtomicU64,
    pub paused: AtomicBool,
    pub cancelled: AtomicBool,
}

pub struct TransferManager {
    pub items: Mutex<HashMap<u64, Arc<TransferState>>>,
    next_id: AtomicU64,
}

impl TransferManager {
    pub fn new() -> Self {
        TransferManager { items: Mutex::new(HashMap::new()), next_id: AtomicU64::new(1) }
    }

    pub fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn get(&self, tid: u64) -> Option<Arc<TransferState>> {
        self.items.lock().unwrap().get(&tid).cloned()
    }

    pub fn remove(&self, tid: u64) {
        self.items.lock().unwrap().remove(&tid);
    }
}

const CHUNK: usize = 256 * 1024; // 对齐 Tabby 256KB 读块

#[allow(clippy::too_many_arguments)]
pub async fn run_transfer(
    app: AppHandle,
    transfers: Arc<TransferManager>,
    session: Arc<SessionHandle>,
    tid: u64,
    direction: String,
    local_path: String,
    remote_path: String,
    resume: bool,
) {
    let result = transfer_loop(&app, &transfers, &session, tid, &direction, &local_path, &remote_path, resume).await;
    let (ok, error) = match result {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };
    transfers.remove(tid);
    let _ = app.emit("transfer-done", TransferDone { tid, ok, error });
}

async fn transfer_loop(
    app: &AppHandle,
    transfers: &Arc<TransferManager>,
    session: &Arc<SessionHandle>,
    tid: u64,
    direction: &str,
    local_path: &str,
    remote_path: &str,
    resume: bool,
) -> AppResult<()> {
    let state = transfers.get(tid).ok_or_else(|| AppError::NoSuchTransfer(tid))?;
    let sftp = get_sftp(session).await?;

    // 总大小
    let total: u64 = if direction == "upload" {
        tokio::fs::metadata(local_path).await.map(|m| m.len()).unwrap_or(0)
    } else {
        sftp.metadata(remote_path).await.ok().and_then(|m| m.size).unwrap_or(0)
    };
    state.total.store(total, Ordering::Relaxed);

    let mut last_emit = tokio::time::Instant::now();

    macro_rules! progress {
        ($transferred:expr) => {
            state.transferred.store($transferred, Ordering::Relaxed);
            if last_emit.elapsed() >= std::time::Duration::from_millis(100) {
                let _ = app.emit(
                    "transfer-progress",
                    TransferProgress { tid, transferred: $transferred, total },
                );
                last_emit = tokio::time::Instant::now();
            }
        };
    }

    async fn check_flags(state: &Arc<TransferState>) -> AppResult<bool> {
        if state.cancelled.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        while state.paused.load(Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            if state.cancelled.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
        }
        Ok(true)
    }

    if direction == "upload" {
        use russh_sftp::protocol::OpenFlags;
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
        let mut local = tokio::fs::File::open(local_path).await?;
        // 断点续传：远端已有部分文件（0 < remote < 本地大小）时从该偏移继续
        let mut start_offset: u64 = 0;
        if resume {
            let remote_size = sftp.metadata(remote_path).await.ok().and_then(|m| m.size).unwrap_or(0);
            let local_size = local.metadata().await.map(|m| m.len()).unwrap_or(0);
            if remote_size > 0 && remote_size < local_size {
                start_offset = remote_size;
                local.seek(std::io::SeekFrom::Start(remote_size)).await?;
                crate::logger::log(&format!("tid={tid} 上传续传：从偏移 {remote_size} 继续（本地共 {local_size}）"));
            }
        }
        let flags = if start_offset > 0 {
            // 续传：保留远端已有内容，从末尾追加
            OpenFlags::CREATE | OpenFlags::WRITE
        } else {
            OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE
        };
        let mut remote = sftp.open_with_flags(remote_path, flags).await.map_err(|e| AppError::msg(format!("打开远程文件失败: {e}")))?;
        if start_offset > 0 {
            remote.seek(std::io::SeekFrom::Start(start_offset)).await.map_err(|e| AppError::msg(format!("远端 seek 失败: {e}")))?;
        }
        state.transferred.store(start_offset, Ordering::Relaxed);
        let mut buf = vec![0u8; CHUNK];
        let mut offset: u64 = start_offset;
        loop {
            check_flags(&state).await?;
            let n = local.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            remote.write_all(&buf[..n]).await?;
            offset += n as u64;
            progress!(offset);
        }
        let _ = remote.shutdown().await;
    } else {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
        let mut remote = sftp.open(remote_path).await.map_err(|e| AppError::msg(format!("打开远程文件失败: {e}")))?;
        // 断点续传：本地已有部分文件（0 < local < 远端大小）时从该偏移继续
        let mut start_offset: u64 = 0;
        if resume {
            let local_size = tokio::fs::metadata(local_path).await.map(|m| m.len()).unwrap_or(0);
            if local_size > 0 && (total == 0 || local_size < total) {
                start_offset = local_size;
                remote.seek(std::io::SeekFrom::Start(local_size)).await.map_err(|e| AppError::msg(format!("远端 seek 失败: {e}")))?;
                crate::logger::log(&format!("tid={tid} 下载续传：从偏移 {local_size} 继续（远端共 {total}）"));
            }
        }
        let local_file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(start_offset == 0)
            .open(local_path)
            .await?;
        let mut local = tokio::io::BufWriter::new(local_file);
        if start_offset > 0 {
            local.seek(std::io::SeekFrom::Start(start_offset)).await?;
        }
        state.transferred.store(start_offset, Ordering::Relaxed);
        let mut buf = vec![0u8; CHUNK];
        loop {
            check_flags(&state).await?;
            let n = remote.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            local.write_all(&buf[..n]).await?;
            offset_written(&state, n as u64);
            progress_local(&app, tid, state.transferred.load(Ordering::Relaxed), total, &mut last_emit);
        }
        local.flush().await?;
        let _ = remote.shutdown().await;
    }
    let total_now = state.total.load(Ordering::Relaxed);
    let _ = app.emit("transfer-progress", TransferProgress { tid, transferred: total_now, total: total_now });
    Ok(())
}

fn offset_written(state: &Arc<TransferState>, n: u64) {
    state.transferred.fetch_add(n, Ordering::Relaxed);
}

fn progress_local(app: &AppHandle, tid: u64, transferred: u64, total: u64, last_emit: &mut tokio::time::Instant) {
    if last_emit.elapsed() >= std::time::Duration::from_millis(100) {
        let _ = app.emit("transfer-progress", TransferProgress { tid, transferred, total });
        *last_emit = tokio::time::Instant::now();
    }
}

/// 递归展开目录：上传（本地遍历）/ 下载（SFTP 遍历）
pub async fn expand_entries(
    session: &Arc<SessionHandle>,
    direction: &str,
    local_path: &str,
    remote_path: &str,
) -> AppResult<Vec<(String, String, u64)>> {
    let mut out = Vec::new();
    if direction == "upload" {
        let meta = tokio::fs::metadata(local_path).await?;
        if meta.is_dir() {
            let mut stack = vec![local_path.to_string()];
            while let Some(dir) = stack.pop() {
                let mut rd = tokio::fs::read_dir(&dir).await?;
                while let Some(entry) = rd.next_entry().await? {
                    let p = entry.path().to_string_lossy().to_string();
                    if entry.file_type().await?.is_dir() {
                        stack.push(p);
                    } else {
                        let rel = p.strip_prefix(local_path).unwrap_or("").replace('\\', "/");
                        let remote_child = format!("{}{}", remote_path.trim_end_matches('/'), rel);
                        let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                        out.push((p, remote_child, size));
                    }
                }
            }
        } else {
            out.push((local_path.to_string(), remote_path.to_string(), meta.len()));
        }
    } else {
        let sftp = get_sftp(session).await?;
        let meta = sftp.metadata(remote_path).await.ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            let mut stack = vec![remote_path.to_string()];
            while let Some(dir) = stack.pop() {
                let entries = sftp.read_dir(&dir).await.map_err(|e| AppError::msg(format!("读取目录失败: {e}")))?;
                for e in entries {
                    let name = e.file_name();
                    if name == "." || name == ".." {
                        continue;
                    }
                    let child = format!("{}/{}", remote_path.trim_end_matches('/'), name);
                    let is_child_dir = e.metadata().is_dir();
                    if is_child_dir {
                        stack.push(child);
                    } else {
                        let size = e.metadata().size.unwrap_or(0);
                        let rel = child.strip_prefix(remote_path).unwrap_or("");
                        let local_child = format!("{}{}", local_path.trim_end_matches('\\'), rel.replace('/', "\\"));
                        out.push((local_child, child, size));
                    }
                }
            }
        } else {
            let size = meta.and_then(|m| m.size).unwrap_or(0);
            out.push((local_path.to_string(), remote_path.to_string(), size));
        }
    }
    Ok(out)
}
