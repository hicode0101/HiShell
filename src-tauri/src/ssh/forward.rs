//! 端口转发管理：本地 / 远程 / 动态（SOCKS5，本模块内置精简实现，无认证 + CONNECT）
//! 语义对齐 Tabby：
//! - Local:   本地监听 → direct-tcpip 通道 → 目标
//! - Remote:  服务器监听（tcpip_forward）→ 入站 forwarded-tcpip 通道 → 本地目标
//! - Dynamic: 本地 SOCKS5 服务 → direct-tcpip 通道 → 客户端请求的目标

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use russh::client::Handle;
use russh::ChannelMsg;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::{AppError, AppResult};
use crate::profile_store::ForwardRule;
use tauri::Emitter;

use super::manager::{SessionHandle, SshHandler};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardStatus {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "bindHost")]
    pub bind_host: String,
    #[serde(rename = "bindPort")]
    pub bind_port: u16,
    #[serde(rename = "targetHost")]
    pub target_host: Option<String>,
    #[serde(rename = "targetPort")]
    pub target_port: Option<u16>,
    pub active: bool,
    pub error: Option<String>,
}

struct Running {
    rule: ForwardRule,
    shutdown: Arc<AtomicBool>,
    task: JoinHandle<()>,
    error: Arc<Mutex<Option<String>>>,
}

pub struct ForwardManager {
    pub running: Mutex<HashMap<String, Running>>,
}

impl ForwardManager {
    pub fn new() -> Self {
        ForwardManager { running: Mutex::new(HashMap::new()) }
    }

    pub fn list(&self) -> Vec<ForwardStatus> {
        self.running
            .lock()
            .unwrap()
            .values()
            .map(|r| ForwardStatus {
                id: r.rule.id.clone(),
                kind: r.rule.kind.clone(),
                bind_host: r.rule.bind_host.clone(),
                bind_port: r.rule.bind_port,
                target_host: r.rule.target_host.clone(),
                target_port: r.rule.target_port,
                active: !r.shutdown.load(Ordering::Relaxed),
                error: r.error.lock().unwrap().clone(),
            })
            .collect()
    }

    pub fn is_running(&self, rule_id: &str) -> bool {
        self.running.lock().unwrap().contains_key(rule_id)
    }

    /// 停止指定转发（本地关闭 listener；远程调用 cancel-forward）
    pub async fn stop(&self, rule_id: &str, handle: &Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>) {
        let running = {
            let mut map = self.running.lock().unwrap();
            map.remove(rule_id)
        };
        if let Some(r) = running {
            r.shutdown.store(true, Ordering::Relaxed);
            r.task.abort();
            if r.rule.kind == "remote" {
                let mut guard = handle.lock().await;
                if let Some(h) = guard.as_mut() {
                    let _ = h.cancel_tcpip_forward(r.rule.bind_host.clone(), r.rule.bind_port as u32).await;
                }
            }
        }
    }

    pub async fn stop_all(&self, handle: &Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>) {
        let ids: Vec<String> = self.running.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.stop(&id, handle).await;
        }
    }
}

fn shutdown_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

/// 启动一条转发规则（会话已连接）
pub async fn start(session: Arc<SessionHandle>, rule: ForwardRule) -> AppResult<()> {
    {
        let mut map = session.forwards.running.lock().unwrap();
        if map.contains_key(&rule.id) {
            return Ok(());
        }
    }
    let shutdown = shutdown_flag();
    let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handle = session.handle.clone();

    let task = match rule.kind.as_str() {
        "local" => tokio::spawn(local_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        "dynamic" => tokio::spawn(dynamic_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        "remote" => tokio::spawn(remote_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        other => return Err(AppError::msg(format!("未知转发类型: {other}"))),
    };
    session.forwards.running.lock().unwrap().insert(
        rule.id.clone(),
        Running { rule, shutdown, task, error },
    );
    Ok(())
}

fn set_error(err: &Arc<Mutex<Option<String>>>, e: String) {
    *err.lock().unwrap() = Some(e);
}

async fn pump<A, B>(a: &mut A, b: &mut B) -> Result<(), std::io::Error>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (mut ra, mut wa) = tokio::io::split(a);
    let (mut rb, mut wb) = tokio::io::split(b);
    let dir1 = async { tokio::io::copy(&mut ra, &mut wb).await };
    let dir2 = async { tokio::io::copy(&mut rb, &mut wa).await };
    tokio::select! {
        r = dir1 => { let _ = wb.shutdown().await; r.map(|_| ()) }
        r = dir2 => { let _ = wa.shutdown().await; r.map(|_| ()) }
    }
}

/// 打开 direct-tcpip 通道并返回可读写流
async fn open_direct(
    handle: &Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    host: &str,
    port: u16,
    originator: &str,
) -> AppResult<impl AsyncRead + AsyncWrite + Unpin> {
    let mut guard = handle.lock().await;
    let handle = guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
    let channel = handle
        .channel_open_direct_tcpip(host.to_string(), port as u32, originator.to_string(), 0)
        .await?;
    Ok(channel.into_stream())
}

async fn local_forward(
    handle: Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    rule: ForwardRule,
    shutdown: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    let listener = match tokio::net::TcpListener::bind((rule.bind_host.as_str(), rule.bind_port)).await {
        Ok(l) => l,
        Err(e) => {
            set_error(&error, format!("监听 {}:{} 失败: {e}", rule.bind_host, rule.bind_port));
            return;
        }
    };
    let target_host = rule.target_host.clone().unwrap_or_else(|| "127.0.0.1".into());
    let target_port = rule.target_port.unwrap_or(0);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if shutdown.load(Ordering::Relaxed) { break }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((mut tcp, peer)) => {
                        let handle = handle.clone();
                        let target_host = target_host.clone();
                        tokio::spawn(async move {
                            match open_direct(&handle, &target_host, target_port, &peer.ip().to_string()).await {
                                Ok(mut ch) => {
                                    let _ = pump(&mut ch, &mut tcp).await;
                                }
                                Err(_) => { let _ = tcp.shutdown().await; }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

/// 精简 SOCKS5 服务端：无认证 + CONNECT（动态转发专用）
async fn socks5_handshake(client: &mut TcpStream) -> AppResult<(String, u16)> {
    let mut buf = [0u8; 2];
    client.read_exact(&mut buf).await?;
    if buf[0] != 5 {
        return Err(AppError::msg("非 SOCKS5 客户端"));
    }
    let n_methods = buf[1] as usize;
    let mut methods = vec![0u8; n_methods];
    client.read_exact(&mut methods).await?;
    // 选择无认证
    client.write_all(&[5, 0]).await?;

    let mut head = [0u8; 4];
    client.read_exact(&mut head).await?;
    if head[0] != 5 || head[1] != 1 {
        return Err(AppError::msg("仅支持 SOCKS5 CONNECT"));
    }
    let (host, port) = match head[3] {
        1 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            let mut p = [0u8; 2];
            client.read_exact(&mut p).await?;
            (std::net::Ipv4Addr::from(ip).to_string(), u16::from_be_bytes(p))
        }
        3 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            client.read_exact(&mut domain).await?;
            let mut p = [0u8; 2];
            client.read_exact(&mut p).await?;
            (String::from_utf8_lossy(&domain).to_string(), u16::from_be_bytes(p))
        }
        4 => {
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await?;
            let mut p = [0u8; 2];
            client.read_exact(&mut p).await?;
            (std::net::Ipv6Addr::from(ip).to_string(), u16::from_be_bytes(p))
        }
        _ => return Err(AppError::msg("不支持的地址类型")),
    };
    Ok((host, port))
}

async fn dynamic_forward(
    handle: Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    rule: ForwardRule,
    shutdown: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    let listener = match tokio::net::TcpListener::bind((rule.bind_host.as_str(), rule.bind_port)).await {
        Ok(l) => l,
        Err(e) => {
            set_error(&error, format!("监听 {}:{} 失败: {e}", rule.bind_host, rule.bind_port));
            return;
        }
    };
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if shutdown.load(Ordering::Relaxed) { break }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((mut tcp, _)) => {
                        let handle = handle.clone();
                        tokio::spawn(async move {
                            let ok_reply = [5u8, 0, 0, 1, 127, 0, 0, 1, 0, 0];
                            let fail_reply = [5u8, 5, 0, 1, 0, 0, 0, 0, 0, 0];
                            match socks5_handshake(&mut tcp).await {
                                Ok((host, port)) => match open_direct(&handle, &host, port, "socks5").await {
                                    Ok(mut ch) => {
                                        let _ = tcp.write_all(&ok_reply).await;
                                        let _ = pump(&mut ch, &mut tcp).await;
                                    }
                                    Err(_) => {
                                        let _ = tcp.write_all(&fail_reply).await;
                                    }
                                },
                                Err(_) => {
                                    let _ = tcp.write_all(&fail_reply).await;
                                }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

/// 远程转发：请求服务器监听，入站通道由会话 dispatcher 投递到本任务的 rx
async fn remote_forward(
    handle: Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    rule: ForwardRule,
    shutdown: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    {
        let mut guard = handle.lock().await;
        let Some(h) = guard.as_mut() else {
            set_error(&error, "会话未连接".into());
            return;
        };
        if let Err(e) = h.tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32).await {
            set_error(&error, format!("请求服务器监听 {}:{} 失败: {e}", rule.bind_host, rule.bind_port));
            return;
        }
    }
    // 等待关闭信号；入站连接由会话级 dispatcher 按 bind_port 分发
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if shutdown.load(Ordering::Relaxed) { break }
            }
        }
    }
}

/// 会话级入站通道分发器：把服务器打开的 forwarded-tcpip 通道接到匹配的远程规则目标
pub async fn spawn_remote_dispatcher(
    session: Arc<SessionHandle>,
    mut rx: mpsc::UnboundedReceiver<(russh::Channel<russh::client::Msg>, String, u16)>,
) {
    loop {
        match rx.recv().await {
            None => break,
            Some((channel, bind_host, bind_port)) => {
                let session = session.clone();
                tokio::spawn(async move {
                    let _ = bind_host;
                    // 找到匹配 bind_port 的远程规则
                    let target = {
                        let map = session.forwards.running.lock().unwrap();
                        map.values()
                            .find(|r| r.rule.kind == "remote" && r.rule.bind_port == bind_port)
                            .map(|r| (r.rule.target_host.clone().unwrap_or_else(|| "127.0.0.1".into()), r.rule.target_port.unwrap_or(0)))
                    };
                    match target {
                        Some((thost, tport)) => match TcpStream::connect((thost.as_str(), tport)).await {
                            Ok(tcp) => relay_channel_tcp(channel, tcp).await,
                            Err(_) => {
                                let _ = channel.close().await;
                            }
                        },
                        None => {
                            let _ = channel.close().await;
                        }
                    }
                });
            }
        }
    }
}

/// 通道 <-> TCP 双向中继
async fn relay_channel_tcp(mut ch: russh::Channel<russh::client::Msg>, tcp: TcpStream) {
    use tokio::io::AsyncWriteExt;
    let mut writer = ch.make_writer();
    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp);

    let chan_to_tcp = async {
        loop {
            match ch.wait().await {
                Some(ChannelMsg::Data { ref data }) => {
                    if tcp_write.write_all(data).await.is_err() {
                        break;
                    }
                }
                Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                    let _ = tcp_write.write_all(data).await;
                }
                Some(ChannelMsg::Eof) => {
                    let _ = tcp_write.shutdown().await;
                }
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
    };
    let tcp_to_chan = async {
        let _ = tokio::io::copy(&mut tcp_read, &mut writer).await;
        let _ = writer.shutdown().await;
    };
    tokio::select! {
        _ = chan_to_tcp => {}
        _ = tcp_to_chan => {}
    }
    let _ = ch.close().await;
}

/// 供 app 退出/断开时统一清理
pub fn emit_forward_log(app: &tauri::AppHandle, sid: u32, message: &str) {
    let _ = app.emit("forward-log", serde_json::json!({ "sid": sid, "message": message }));
}
