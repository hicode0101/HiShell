//! SSH 端到端集成测试
//!
//! 默认：启动 test/ssh_server.py（paramiko 本地服务器 root/root123）
//! 真实服务器：设置环境变量 E2E_REAL=1 且提供
//!   E2E_HOST / E2E_PORT(默认22) / E2E_USER / E2E_PASS

use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use hishell_lib::profile_store::{self, Profile, ProxyConfig};
use hishell_lib::secrets;
use hishell_lib::ssh::manager::{self, SessionShared};
use russh::ChannelMsg;


struct TestServer(Child);

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn spawn_local_server() -> TestServer {
    let child = Command::new("python")
        .arg("test/ssh_server.py")
        .spawn()
        .expect("启动 python ssh 测试服务器失败（需要 python + paramiko）");
    TestServer(child)
}

fn wait_port(port: u16) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("ssh 测试服务器端口 {port} 未就绪");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn make_profile(host: &str, port: u16, user: &str, id: &str) -> Profile {
    Profile {
        id: id.into(),
        name: "e2e".into(),
        host: host.into(),
        port,
        username: user.into(),
        auth: "password".into(),
        password: String::new(),
        has_stored_password: false,
        private_key_path: String::new(),
        private_key_passphrase: String::new(),
        has_stored_passphrase: false,
        user_key_name: String::new(),
        temporary: false,
        mcp_authorized: false,
        proxy: ProxyConfig::None,
        keepalive_interval: 5000,
        keepalive_count_max: 3,
        connect_timeout: 10000,
        term: "xterm-256color".into(),
        forwarded_ports: vec![],
        behavior_on_session_end: "close".into(),
    }
}

#[tokio::main]
async fn main() {
    run_e2e().await;
}

async fn run_e2e() -> Result<(), Box<dyn std::error::Error>> {
    let real = std::env::var("E2E_REAL").is_ok();
    let host = env_or("E2E_HOST", "127.0.0.1");
    let port: u16 = env_or("E2E_PORT", "2222").parse().unwrap();
    let user = env_or("E2E_USER", "root");
    let pass = env_or("E2E_PASS", "root123");
    let profile_id = if real { "e2e-real" } else { "e2e-local" };

    let _server: Option<TestServer> = if real {
        println!("[e2e] 真实服务器 {host}:{port} user={user}");
        None
    } else {
        let s = spawn_local_server();
        wait_port(2222);
        Some(s)
    };

    // 写入测试凭据
    secrets::set(
        profile_id,
        &secrets::ProfileSecrets {
            password: Some(pass.clone()),
            ..Default::default()
        },
    );

    let profile = make_profile(&host, port, &user, profile_id);
    profile_store::upsert_with_secrets(profile.clone(), &Default::default()).unwrap();

    let shared = Arc::new(SessionShared {
        sid: 9001,
        profile_id: profile_id.to_string(),
        emit: Arc::new(move |event: &str, payload: serde_json::Value| {
            println!("[e2e][event] {event}: {payload}");
        }),
        host: profile.host.clone(),
        port: profile.port,
        user: profile.username.clone(),
        password: std::sync::Mutex::new(None),
        known_hosts: manager::KNOWN_HOSTS,
        pending: std::sync::Mutex::new(std::collections::HashMap::new()),
        forwarded_tx: std::sync::Mutex::new(None),
        closed: std::sync::atomic::AtomicBool::new(false),
        auto_accept_hostkey: true,
    });

    // 1. 连接 + 密码认证
    println!("[e2e] connect_and_auth ...");
    let mut handle = manager::connect_and_auth(shared, profile).await?;
    println!("[e2e] 认证成功");

    // 2. 打开 PTY + shell
    let mut channel = handle.channel_open_session().await?;
    channel.request_pty(true, "xterm-256color", 80, 24, 0, 0, &[]).await?;
    channel.request_shell(true).await?;

    // 3. 发送命令
    use tokio::io::AsyncWriteExt;
    let mut writer = channel.make_writer();
    writer.write_all(b"echo hi\r\n").await?;

    // 4. 读取回显（总 8 秒超时）
    let mut got = Vec::<u8>::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(format!("未在超时前收到回显，已收到: {}", String::from_utf8_lossy(&got)).into());
        }
        match tokio::time::timeout(Duration::from_millis(800), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { ref data })) => {
                got.extend_from_slice(data);
                let text = String::from_utf8_lossy(&got).to_string();
                println!("[e2e] data: {text:?}");
                if text.contains("OK-echo") || text.contains("hi") {
                    break;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("通道被服务器关闭，已收到: {:?}", String::from_utf8_lossy(&got)),
            Err(_) => {} // 单次等待超时，继续循环检查总 deadline
        }
    }
    assert!(String::from_utf8_lossy(&got).contains("hi"));
    println!("[e2e] PASS");
    Ok(())
}
