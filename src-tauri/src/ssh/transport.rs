//! SSH 传输链：直连 / SOCKS5 / HTTP CONNECT / ProxyCommand / 跳板机（递归）
//!
//! 连接链构建：最终目标 + 跳板机序列，每跳的传输层由它的 proxy 配置决定
//! （跳板机之间的管道为上一跳的 direct-tcpip channel 流，语义对齐 Tabby）。

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::error::{AppError, AppResult};
use crate::profile_store::{Profile, ProxyConfig};

pub enum Transport {
    Tcp(TcpStream),
    Proxy(Box<dyn AsyncStream>),
    ProxyCommand {
        child: tokio::process::Child,
    },
    JumpChannel(Box<dyn AsyncStream>),
}

/// 统一的可读写流
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

async fn tcp_connect_async(host: &str, port: u16, timeout_ms: u64) -> AppResult<TcpStream> {
    let addr = format!("{host}:{port}");
    tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), TcpStream::connect(addr))
        .await
        .map_err(|_| AppError::Timeout)?
        .map_err(|e| AppError::msg(format!("连接 {host}:{port} 失败: {e}")))
}

/// 为一跳建立底层流（不含 SSH 握手）
pub async fn connect_transport(hop: &Profile, underlying: Option<Box<dyn AsyncStream>>) -> AppResult<Transport> {
    let timeout_ms = hop.connect_timeout.max(1000);
    match &hop.proxy {
        ProxyConfig::None => {
            let tcp = tcp_connect_async(&hop.host, hop.port, timeout_ms).await?;
            Ok(Transport::Tcp(tcp))
        }
        ProxyConfig::Socks5 { host, port, username, password } => {
            let tcp = tcp_connect_async(host, *port, timeout_ms).await?;
            let target = (hop.host.as_str(), hop.port);
            let stream = if let Some(u) = username {
                let fut = tokio_socks::tcp::Socks5Stream::connect_with_password_and_socket(
                    tcp, target, u.as_str(), password.as_deref().unwrap_or(""),
                );
                tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut)
                    .await
                    .map_err(|_| AppError::Timeout)?
                    .map_err(|e| AppError::Proxy(format!("SOCKS5 握手失败: {e}")))?
            } else {
                let fut = tokio_socks::tcp::Socks5Stream::connect_with_socket(tcp, target);
                tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut)
                    .await
                    .map_err(|_| AppError::Timeout)?
                    .map_err(|e| AppError::Proxy(format!("SOCKS5 握手失败: {e}")))?
            };
            Ok(Transport::Proxy(Box::new(stream)))
        }
        ProxyConfig::Http { host, port, username, password } => {
            let mut tcp = tcp_connect_async(host, *port, timeout_ms).await?;
            let authority = format!("{}:{}", hop.host, hop.port);
            let mut req = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
            if let Some(u) = username {
                use base64::Engine;
                let cred = format!("{}:{}", u, password.clone().unwrap_or_default());
                req.push_str(&format!(
                    "Proxy-Authorization: Basic {}\r\n",
                    base64::engine::general_purpose::STANDARD.encode(cred.as_bytes())
                ));
            }
            req.push_str("\r\n");
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
                tcp.write_all(req.as_bytes()).await?;
                let mut buf = [0u8; 1024];
                let n = tcp.read(&mut buf).await?;
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                if !head.contains(" 200") {
                    return Err(AppError::Proxy(format!("HTTP 代理 CONNECT 失败: {}", head.lines().next().unwrap_or(""))));
                }
                Ok(())
            })
            .await
            .map_err(|_| AppError::Timeout)??;
            Ok(Transport::Proxy(Box::new(tcp)))
        }
        ProxyConfig::ProxyCommand { command } => {
            let mut parts = command.split_whitespace();
            let prog = parts.next().ok_or_else(|| AppError::msg("ProxyCommand 为空"))?;
            let child = tokio::process::Command::new(prog)
                .args(parts)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| AppError::msg(format!("ProxyCommand 启动失败: {e}")))?;
            Ok(Transport::ProxyCommand { child })
        }
        ProxyConfig::Jump { .. } => match underlying {
            Some(s) => Ok(Transport::JumpChannel(s)),
            None => Err(AppError::msg("跳板链缺少底层通道")),
        },
    }
}

/// 把 Transport 转换为 russh 可用的流
pub fn transport_to_stream(t: Transport) -> AppResult<Box<dyn AsyncStream>> {
    match t {
        Transport::Tcp(tcp) => Ok(Box::new(tcp)),
        Transport::Proxy(s) => Ok(s),
        Transport::JumpChannel(s) => Ok(s),
        Transport::ProxyCommand { mut child } => {
            let stdin = child.stdin.take().ok_or_else(|| AppError::msg("ProxyCommand 无 stdin"))?;
            let stdout = child.stdout.take().ok_or_else(|| AppError::msg("ProxyCommand 无 stdout"))?;
            Ok(Box::new(ProxyCommandStream { stdin, stdout }))
        }
    }
}

/// 子进程 stdin/stdout 组合为双工流（进程生命周期与连接绑定，由 Session 关闭时 kill）
pub struct ProxyCommandStream {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
}

impl AsyncRead for ProxyCommandStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProxyCommandStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stdin).poll_write(cx, buf)
    }
    fn poll_flush(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_flush(cx)
    }
    fn poll_shutdown(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

pub fn arc_stream(s: Box<dyn AsyncStream>) -> Arc<Box<dyn AsyncStream>> {
    Arc::new(s)
}
