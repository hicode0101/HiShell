//! MCP Server：把 HiShell 已授权的 SSH 连接暴露给本机 AI 客户端。
//! 模块结构：store（配置持久化）/ server（HTTP+协议）/ tools（工具实现）。

pub mod server;
pub mod store;
pub mod tools;

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;

use tauri::AppHandle;

use crate::ssh::SessionManager;

/// 运行中的服务句柄（shutdown 通道 + serve 任务句柄）
struct Running {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<Result<(), String>>,
}

static RUNNING: Lazy<Mutex<Option<Running>>> = Lazy::new(|| Mutex::new(None));
static LAST_ERROR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub listen_host: String,
    pub port: u16,
    pub error: Option<String>,
}

/// 当前状态（设置页轮询/刷新用）
pub fn status() -> McpStatus {
    let cfg = store::load();
    let error = LAST_ERROR.lock().unwrap().clone();
    McpStatus {
        enabled: cfg.enabled,
        running: is_running(),
        listen_host: cfg.listen_host,
        port: cfg.port,
        error,
    }
}

/// 把当前状态推送给前端（状态栏指示灯同步）。启动/停止状态变化时调用。
pub fn emit_status(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("mcp-status", status());
}

/// 启动 MCP Server（已运行则先停止并等旧任务退出，再绑定新地址）。
/// 端口绑定同步完成：失败（占用/无权限等）立即返回错误给设置页。
/// 无论成败，结束后推送最新状态，保证状态栏指示灯与真实状态一致。
pub async fn start(app: &AppHandle, sessions: Arc<SessionManager>) -> Result<(), String> {
    let res = start_inner(app, sessions).await;
    emit_status(app);
    res
}

async fn start_inner(app: &AppHandle, sessions: Arc<SessionManager>) -> Result<(), String> {
    // 关键：等旧 serve 任务真正退出（监听 socket 已释放）再绑定，否则重启场景必报端口占用
    stop_and_wait(app).await;

    let cfg = store::load();
    let host: IpAddr = cfg
        .listen_host
        .parse()
        .map_err(|_| format!("监听地址无效: {}（应为合法 IP）", cfg.listen_host))?;
    let addr = SocketAddr::new(host, cfg.port);
    let token = store::ensure_token().map_err(|e| format!("访问令牌不可用: {e}"))?;

    let std_listener = std::net::TcpListener::bind(addr).map_err(|e| {
        let msg = format!("监听 {addr} 失败: {e}（端口可能被占用）");
        *LAST_ERROR.lock().unwrap() = Some(msg.clone());
        msg
    })?;
    std_listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| format!("监听 {addr} 失败: {e}"))?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    *LAST_ERROR.lock().unwrap() = None;
    let task = tauri::async_runtime::spawn(server::serve(listener, token, app.clone(), sessions, shutdown_rx));
    *RUNNING.lock().unwrap() = Some(Running { shutdown_tx, task });
    Ok(())
}

/// 停止 MCP Server 并等待 serve 任务退出（端口释放）。未运行则无操作。
/// 停止后推送最新状态，保证状态栏指示灯与真实状态一致。
pub async fn stop_and_wait(app: &AppHandle) {
    let running = RUNNING.lock().unwrap().take();
    if let Some(r) = running {
        let _ = r.shutdown_tx.send(true);
        let _ = r.task.await;
    }
    *LAST_ERROR.lock().unwrap() = None;
    emit_status(app);
}

pub fn is_running() -> bool {
    RUNNING.lock().unwrap().is_some()
}

/// 自启动失败时记录错误并推送状态（start 返回 Err 的场景下 RUNNING 尚未写入，需单独留痕给状态页）
pub fn mark_startup_error(app: &AppHandle, e: &str) {
    *LAST_ERROR.lock().unwrap() = Some(e.to_string());
    emit_status(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_loopback_only() {
        let cfg = store::McpConfig::default();
        assert_eq!(cfg.listen_host, "127.0.0.1");
        assert!(!cfg.enabled);
    }

    #[test]
    fn generated_token_has_prefix_and_entropy() {
        let t1 = store::generate_token();
        let t2 = store::generate_token();
        assert!(t1.starts_with("hs_"));
        assert!(t1.len() > 40);
        assert_ne!(t1, t2, "两次生成的令牌不应相同");
    }
}
