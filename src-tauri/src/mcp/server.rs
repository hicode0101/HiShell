//! MCP Server 传输层：Streamable HTTP（JSON 响应模式）+ Bearer Token 鉴权。
//! 手写最小 HTTP/1.1（请求行 + 头 + Content-Length 体），不引入 axum 等重依赖：
//! 本服务只服务 localhost 场景的单端点 /mcp，协议面非常窄。

use std::sync::Arc;

use sha2::{Digest, Sha256};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use tauri::AppHandle;

use crate::logger;
use crate::ssh::SessionManager;

use super::tools;

/// 已知的 MCP 协议版本（initialize 协商用）
const KNOWN_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];
const FALLBACK_VERSION: &str = "2025-03-26";
const HEAD_CAP: usize = 64 * 1024;
const BODY_CAP: usize = 2 * 1024 * 1024;

pub async fn serve(
    listener: TcpListener,
    token: String,
    app: AppHandle,
    sessions: Arc<SessionManager>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".into());
    logger::log(&format!("MCP Server 已启动 http://{addr}/mcp"));

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                logger::log("MCP Server 已停止");
                return Ok(());
            }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(v) => v,
                    Err(e) => {
                        logger::log(&format!("MCP accept 失败: {e}"));
                        continue;
                    }
                };
                let app = app.clone();
                let sessions = sessions.clone();
                let token = token.clone();
                tokio::spawn(async move {
                    let _ = handle_conn(stream, peer, app, sessions, token).await;
                });
            }
        }
    }
}

/* ---------------- HTTP 基础处理 ---------------- */

struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> Result<Option<Request>, String> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    // 读到头部结束标记
    let head_end = loop {
        if buf.len() > HEAD_CAP {
            return Err("请求头过大".into());
        }
        let n = stream.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None); // 连接关闭
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_head_end(&buf) {
            break pos;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let path = parts.next().unwrap_or("/").to_string();
    let mut authorization = None;
    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "authorization" {
            authorization = Some(value.to_string());
        } else if name == "content-length" {
            content_length = value.parse().unwrap_or(0);
        }
    }
    if content_length > BODY_CAP {
        return Err("请求体过大".into());
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Ok(Some(Request { method, path, authorization, body }))
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn respond(stream: &mut TcpStream, status: u16, reason: &str, content_type: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body).await;
    let _ = stream.flush().await;
}

fn json_response(v: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap_or_default()
}

/* ---------------- 连接处理 + 鉴权 ---------------- */

async fn handle_conn(
    mut stream: TcpStream,
    peer: std::net::SocketAddr,
    app: AppHandle,
    sessions: Arc<SessionManager>,
    token: String,
) -> Result<(), String> {
    let Some(req) = read_request(&mut stream).await? else { return Ok(()) };

    // 路径归一化：/mcp（忽略查询串）
    let path = req.path.split('?').next().unwrap_or("/").to_string();
    if path != "/mcp" {
        respond(&mut stream, 404, "Not Found", "application/json", b"{\"error\":\"not found\"}").await;
        return Ok(());
    }

    match req.method.as_str() {
        // CORS 预检（浏览器场景客户端）
        "OPTIONS" => {
            let head = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, Mcp-Session-Id, Mcp-Protocol-Version\r\nAccess-Control-Max-Age: 86400\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.flush().await;
            Ok(())
        }
        // GET = 服务器主动推送流（本服务无此需求，按规范回 405）
        "GET" | "DELETE" => {
            respond(&mut stream, 405, "Method Not Allowed", "application/json", b"{\"error\":\"method not allowed\"}").await;
            Ok(())
        }
        "POST" => {
            // Bearer Token 校验（哈希后比对，避免时序/长度泄露）
            let provided = req
                .authorization
                .as_deref()
                .and_then(|a| a.strip_prefix("Bearer "))
                .unwrap_or("");
            if sha256_hex(provided.as_bytes()) != sha256_hex(token.as_bytes()) {
                logger::log(&format!("MCP 鉴权失败 peer={peer}"));
                respond(&mut stream, 401, "Unauthorized", "application/json", b"{\"error\":\"invalid token\"}").await;
                return Ok(());
            }
            // JSON 解析失败必须回 -32700 错误响应，而不是断开连接
            let v = match serde_json::from_slice::<serde_json::Value>(&req.body) {
                Ok(v) => v,
                Err(e) => {
                    let body = json_response(&json!({
                        "jsonrpc": "2.0", "id": serde_json::Value::Null,
                        "error": { "code": -32700, "message": format!("解析错误: {e}") }
                    }));
                    respond(&mut stream, 400, "Bad Request", "application/json", &body).await;
                    return Ok(());
                }
            };
            let (status, resp) = dispatch_rpc(&app, &sessions, &v).await;
            if status == 202 {
                // 规范要求：通知类请求 202 Accepted 且无响应体
                respond(&mut stream, 202, "Accepted", "application/json", b"").await;
            } else {
                let body = json_response(&resp);
                respond(&mut stream, 200, "OK", "application/json", &body).await;
            }
            Ok(())
        }
        _ => {
            respond(&mut stream, 405, "Method Not Allowed", "application/json", b"{\"error\":\"method not allowed\"}").await;
            Ok(())
        }
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let out = h.finalize();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/* ---------------- JSON-RPC / MCP 协议分发 ---------------- */

/// 返回 (HTTP 状态码, JSON-RPC 响应)；通知类请求返回 202 + 空 json
async fn dispatch_rpc(app: &AppHandle, sessions: &Arc<SessionManager>, v: &serde_json::Value) -> (u16, serde_json::Value) {
    // 批处理（JSON 数组）逐条处理
    if let Some(items) = v.as_array() {
        let mut responses = Vec::new();
        for item in items {
            let (code, r) = dispatch_single(app, sessions, item).await;
            if code == 200 && !r.is_null() {
                responses.push(r);
            }
        }
        if responses.is_empty() {
            return (202, serde_json::json!({}));
        }
        return (200, serde_json::Value::Array(responses));
    }
    let (code, r) = dispatch_single(app, sessions, v).await;
    if code == 202 {
        (202, serde_json::json!({}))
    } else {
        (200, r)
    }
}

async fn dispatch_single(app: &AppHandle, sessions: &Arc<SessionManager>, v: &serde_json::Value) -> (u16, serde_json::Value) {
    let Some(method) = v.get("method").and_then(|m| m.as_str()) else {
        return (200, rpc_error(v, -32600, "无效请求：缺少 method"));
    };
    let id = v.get("id").cloned();

    // 通知（无 id）：不回包
    let Some(id) = id else {
        logger::log(&format!("MCP 通知: {method}"));
        return (202, serde_json::Value::Null);
    };

    let result: Result<serde_json::Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": negotiate_version(v),
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": {},
                "prompts": {},
            },
            "serverInfo": {
                "name": "HiShell",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": "HiShell MCP Server：通过工具操作 HiShell 中已授权（mcpAuthorized）的 SSH 连接。先 list_sessions 查看可用连接，connect_session 建立连接（可能需要在 HiShell 界面人工确认），再用 exec_command 执行命令、upload_file/download_file 传输文件。",
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "resources/list" => Ok(json!({ "resources": [] })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "tools/call" => {
            let name = v.pointer("/params/name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let empty = serde_json::json!({});
            let args = v.pointer("/params/arguments").cloned().unwrap_or(empty);
            logger::log(&format!("MCP tools/call: {name} args={args}"));
            match tools::call(app, sessions, &name, &args).await {
                Ok(value) => Ok(json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
                    "isError": false,
                })),
                Err(e) => Ok(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true,
                })),
            }
        }
        m if m.starts_with("notifications/") => return (202, serde_json::Value::Null),
        other => Err((-32601, format!("方法不存在: {other}"))),
    };

    match result {
        Ok(r) => (200, json!({ "jsonrpc": "2.0", "id": id, "result": r })),
        Err((code, message)) => (200, json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })),
    }
}

/// 协议版本协商：客户端版本在已知集合内则回显，否则回落
fn negotiate_version(v: &serde_json::Value) -> String {
    let requested = v.pointer("/params/protocolVersion").and_then(|p| p.as_str()).unwrap_or("");
    if KNOWN_VERSIONS.contains(&requested) {
        requested.to_string()
    } else {
        FALLBACK_VERSION.into()
    }
}

fn rpc_error(req: &serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": req.get("id").cloned().unwrap_or(serde_json::Value::Null),
        "error": { "code": code, "message": message },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_end_detection() {
        assert_eq!(find_head_end(b"POST /mcp HTTP/1.1\r\nHost: x\r\n\r\nbody"), Some(27));
        assert_eq!(find_head_end(b"no terminator yet"), None);
    }

    #[test]
    fn version_negotiation() {
        // 已知版本回显
        assert_eq!(
            negotiate_version(&json!({ "params": { "protocolVersion": "2025-06-18" } })),
            "2025-06-18"
        );
        // 未知/缺失版本回落
        assert_eq!(
            negotiate_version(&json!({ "params": { "protocolVersion": "1999-01-01" } })),
            FALLBACK_VERSION
        );
        assert_eq!(negotiate_version(&json!({})), FALLBACK_VERSION);
    }

    #[test]
    fn token_compare_is_hash_based() {
        // 相同 → 相同哈希；不同长度也不 panic（避免时序/长度泄露）
        assert_eq!(sha256_hex(b"hs_abc"), sha256_hex(b"hs_abc"));
        assert_ne!(sha256_hex(b"hs_abc"), sha256_hex(b"hs_abcdef"));
    }

    #[test]
    fn rpc_error_echoes_id() {
        let req = json!({ "jsonrpc": "2.0", "id": 42, "method": "nope" });
        let resp = rpc_error(&req, -32601, "x");
        assert_eq!(resp["id"], 42);
        assert_eq!(resp["error"]["code"], -32601);
    }
}
