//! MCP Server 设置命令：配置读写 / 启停 / 令牌管理

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::mcp;
use crate::ssh::Sessions;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigOut {
    pub enabled: bool,
    pub listen_host: String,
    pub port: u16,
    /// 访问令牌（用户本机 UI 自用，用于拼装客户端配置）
    pub token: String,
    pub running: bool,
    pub error: Option<String>,
}

fn config_out() -> McpConfigOut {
    let cfg = mcp::store::load();
    let st = mcp::status();
    McpConfigOut {
        enabled: st.enabled,
        listen_host: cfg.listen_host,
        port: cfg.port,
        token: crate::secrets::mcp_token().unwrap_or_default(),
        running: st.running,
        error: st.error,
    }
}

#[tauri::command]
pub fn mcp_config_get() -> Result<McpConfigOut, String> {
    Ok(config_out())
}

/// 保存配置并立即生效：enabled=true 启动（或按新地址重启）服务，false 停止
#[tauri::command]
pub async fn mcp_config_save(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    enabled: bool,
    listen_host: String,
    port: u16,
) -> Result<McpConfigOut, String> {
    listen_host
        .parse::<std::net::IpAddr>()
        .map_err(|_| format!("监听地址无效: {listen_host}（应为合法 IP，如 127.0.0.1）"))?;
    if port == 0 {
        return Err("端口无效：应为 1–65535".into());
    }
    let cfg = mcp::store::McpConfig { enabled, listen_host, port };
    mcp::store::save(&cfg).map_err(|e| e.to_string())?;
    mcp::store::ensure_token().map_err(|e| e.to_string())?;

    if enabled {
        mcp::start(&app, sessions.0.clone()).await?;
    } else {
        mcp::stop_and_wait(&app).await;
    }
    Ok(config_out())
}

/// 重新生成访问令牌（旧令牌立即失效；服务运行中会自动重启加载新令牌）
#[tauri::command]
pub async fn mcp_token_regen(app: AppHandle, sessions: State<'_, Sessions>) -> Result<McpConfigOut, String> {
    let token = mcp::store::generate_token();
    crate::secrets::set_mcp_token(&token).map_err(|e| e.to_string())?;
    let cfg = mcp::store::load();
    if cfg.enabled {
        mcp::start(&app, sessions.0.clone()).await?;
    }
    Ok(config_out())
}
