//! SSH 会话管理（russh 0.46）：连接、认证（严格按用户配置的方式，无回退）、
//! known_hosts 校验（前端确认弹窗）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use russh::client::{self, Handle};
use russh::keys::key::{self, PublicKey};
use russh::keys::{self, agent::client::AgentClient};
use russh_sftp::client::SftpSession;
use serde::Serialize;
use tauri::Emitter;

/// 事件发送回调（解耦 tauri，便于测试）
pub type EmitFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
use tokio::sync::{oneshot, Mutex as AsyncMutex};

use crate::error::{AppError, AppResult};
use crate::known_hosts::KnownHosts;
use crate::profile_store::{self, Profile};
use crate::profile_store::resolve_chain;
use crate::secrets;

pub const KNOWN_HOSTS: KnownHosts = KnownHosts;

static NEXT_SID: AtomicU32 = AtomicU32::new(1);

pub fn next_sid() -> u32 {
    NEXT_SID.fetch_add(1, Ordering::Relaxed)
}

/// 前端提示请求（hostkey / password / keyboard-interactive）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PromptPayload {
    #[serde(rename = "hostkeyNew")]
    HostkeyNew {
        host: String,
        port: u16,
        fingerprint: String,
        key_type: String,
    },
    #[serde(rename = "hostkeyMismatch")]
    HostkeyMismatch {
        host: String,
        port: u16,
        old_fingerprint: String,
        new_fingerprint: String,
        key_type: String,
    },
    /// 统一认证对话框：密码/密钥验证失败后，允许用户重新输入或切换认证方式
    #[serde(rename = "authRetry")]
    AuthRetry {
        host: String,
        user: String,
        /// 当前失败的认证方式（对话框默认选中）：password / publicKey / keyboardInteractive
        method: String,
        /// 失败原因（对话框顶部红字）
        error: String,
        /// 配置中的私钥文件路径（可为空）
        #[serde(rename = "keyPath")]
        key_path: String,
        /// 用户密钥管理中的可用密钥名
        #[serde(rename = "userKeys")]
        user_keys: Vec<String>,
        /// 会话配置中的原始认证方式：password / privateKey / userKey / keyboardInteractive / agent
        /// （对话框默认选中以此为准，而不是归一化后的 method）
        #[serde(rename = "profileAuth")]
        profile_auth: String,
        /// userKey 配置引用的密钥名（可为空）
        #[serde(rename = "userKeyName")]
        user_key_name: String,
    },
    KeyboardInteractive {
        host: String,
        user: String,
        name: String,
        instruction: String,
        prompts: Vec<KiPrompt>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct KiPrompt {
    pub text: String,
    pub echo: bool,
}

/// 会话共享状态（Handler 与管理器共用）
pub struct SessionShared {
    pub sid: u32,
    /// 所属连接配置 id（认证提示"记住密码"时写入该配置的凭据）
    pub profile_id: String,
    /// 事件发送（ssh-prompt 等）
    pub emit: EmitFn,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 用户认证口令（password / KI 自动填充用）
    pub password: Mutex<Option<String>>,
    pub known_hosts: KnownHosts,
    /// 待应答的提示：key -> 应答发送端
    pub pending: Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>,
    /// 远程转发入站通道发送端（会话建立时设置，dispatcher 持有接收端）
    pub forwarded_tx:
        Mutex<Option<tokio::sync::mpsc::UnboundedSender<(russh::Channel<russh::client::Msg>, String, u16)>>>,
    pub closed: AtomicBool,
    /// 测试专用：自动接受并记录 hostkey（跳过前端确认）
    pub auto_accept_hostkey: bool,
}

impl SessionShared {
    pub async fn ask_frontend(&self, key: &str, payload: PromptPayload, timeout_secs: u64) -> AppResult<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            pending.insert(key.to_string(), tx);
        }
        let mut json = serde_json::to_value(&payload)?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert("sid".into(), serde_json::json!(self.sid));
            obj.insert("key".into(), serde_json::json!(key));
        }
        (self.emit)("ssh-prompt", json);
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(v)) => Ok(v),
            _ => {
                self.pending.lock().unwrap().remove(key);
                Err(AppError::Cancelled)
            }
        }
    }
}

/// russh 客户端 Handler
pub struct SshHandler {
    pub shared: Arc<SessionShared>,
}

#[async_trait::async_trait]
impl client::Handler for SshHandler {
    type Error = AppError;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        let s = &self.shared;
        crate::logger::log(&format!(
            "sid={} hostkey 校验 {}:{} key={} fp={}",
            s.sid, s.host, s.port, server_public_key.name(), format!("SHA256:{}", server_public_key.fingerprint())
        ));
        if s.auto_accept_hostkey {
            let _ = s.known_hosts.remember(&s.host, s.port, server_public_key);
            return Ok(true);
        }
        match s.known_hosts.verify(&s.host, s.port, server_public_key) {
            crate::known_hosts::VerifyResult::Known => Ok(true),
            crate::known_hosts::VerifyResult::New { fingerprint, key_type } => {
                let answer = s
                    .ask_frontend(
                        "hostkey",
                        PromptPayload::HostkeyNew { host: s.host.clone(), port: s.port, fingerprint, key_type },
                        180,
                    )
                    .await
                    .unwrap_or(serde_json::json!({ "action": "cancel" }));
                Ok(match answer.get("action").and_then(|a| a.as_str()) {
                    Some("save") => {
                        let _ = s.known_hosts.remember(&s.host, s.port, server_public_key);
                        true
                    }
                    Some("once") => true,
                    _ => false,
                })
            }
            crate::known_hosts::VerifyResult::Mismatch { old, new, key_type } => {
                let answer = s
                    .ask_frontend(
                        "hostkey",
                        PromptPayload::HostkeyMismatch {
                            host: s.host.clone(),
                            port: s.port,
                            old_fingerprint: old,
                            new_fingerprint: new,
                            key_type,
                        },
                        180,
                    )
                    .await
                    .unwrap_or(serde_json::json!({ "action": "cancel" }));
                Ok(match answer.get("action").and_then(|a| a.as_str()) {
                    Some("save") => {
                        let _ = s.known_hosts.remember(&s.host, s.port, server_public_key);
                        true
                    }
                    Some("once") => true,
                    _ => false,
                })
            }
        }
    }

    /// 远程转发入站通道：交给会话 dispatcher 接管
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let tx = self.shared.forwarded_tx.lock().unwrap().clone();
        if let Some(tx) = tx {
            let _ = tx.send((channel, _connected_address.to_string(), connected_port as u16));
        }
        Ok(())
    }
}

/// 运行中的会话
pub struct SessionHandle {
    pub sid: u32,
    pub profile_id: String,
    pub shared: Arc<SessionShared>,
    /// russh Handle（不可克隆，经异步互斥锁共享）。
    /// "复制 SSH 隧道"出的别名会话与父会话共享同一个 Handle（同一条 SSH 连接）。
    pub handle: Arc<AsyncMutex<Option<Handle<SshHandler>>>>,
    pub closed: AtomicBool,
    /// 每会话写入序列化通道
    pub write_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// 端口转发管理器（别名会话与父会话共享，对齐"复制 SSH 隧道"语义）
    pub forwards: Arc<super::forward::ForwardManager>,
    /// SFTP 会话缓存（懒创建）
    pub sftp: Mutex<Option<Arc<SftpSession>>>,
    /// 终端 resize 请求通道（由数据泵任务消费）
    pub resize_tx: tokio::sync::mpsc::UnboundedSender<(u32, u32)>,
    /// 是否为"复制 SSH 隧道"产生的别名会话：断开时只关自己的通道，
    /// 不发送 SSH DISCONNECT、不停止共享的端口转发
    pub is_alias: bool,
}

pub struct SessionManager {
    sessions: Mutex<HashMap<u32, Arc<SessionHandle>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        SessionManager { sessions: Mutex::new(HashMap::new()) }
    }
}

impl SessionManager {
    pub fn new() -> Self {
        SessionManager::default()
    }

    pub fn get(&self, sid: u32) -> Option<Arc<SessionHandle>> {
        self.sessions.lock().unwrap().get(&sid).cloned()
    }

    pub fn insert(&self, handle: Arc<SessionHandle>) {
        self.sessions.lock().unwrap().insert(handle.sid, handle);
    }

    pub fn remove(&self, sid: u32) -> Option<Arc<SessionHandle>> {
        self.sessions.lock().unwrap().remove(&sid)
    }

    pub fn list_ids(&self) -> Vec<u32> {
        self.sessions.lock().unwrap().keys().copied().collect()
    }
}

pub fn emit_status(app: &tauri::AppHandle, sid: u32, status: &str, message: Option<&str>) {
    let _ = app.emit("ssh-status", serde_json::json!({ "sid": sid, "status": status, "message": message }));
}

pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// 加载私钥（含口令解析与加密错误识别）
fn load_private_key(path: &str, passphrase: Option<&str>) -> AppResult<key::KeyPair> {
    let expanded = expand_tilde(path);
    keys::load_secret_key(&expanded, passphrase).map_err(|e| AppError::msg(format!("私钥加载失败: {e}")))
}

/// 认证：严格按用户配置的认证方式执行，不做静默回退——
/// 密码错误就报「密码认证被拒绝」，绝不落到 agent/publicKey 等未配置方式。
/// 失败后弹统一认证对话框：用户可重输密码、换密钥或切换认证方式，直至成功或取消。
async fn authenticate(
    handle: &mut Handle<SshHandler>,
    shared: &Arc<SessionShared>,
    profile: &Profile,
    secrets_bundle: &secrets::ProfileSecrets,
) -> AppResult<()> {
    // userKey：按名称引用用户密钥管理中的密钥，解析为 ~/.ssh/<name> 的公钥认证
    let profile_auth = profile.auth.clone();
    let user_key_name = profile.user_key_name.clone();
    let mut profile = profile.clone();
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
    let mut method = match profile.auth.as_str() {
        "agent" => "agent",
        "publicKey" => "publicKey",
        "keyboardInteractive" => "keyboardInteractive",
        _ => "password",
    };
    let mut override_creds: Option<serde_json::Value> = None;

    loop {
        if shared.closed.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let method_name = match method {
            "agent" => "agent",
            "publicKey" => "公钥",
            "keyboardInteractive" => "键盘交互",
            _ => "密码",
        };
        crate::logger::log(&format!(
            "开始认证 user={} 方式={method}{}",
            shared.user,
            if override_creds.is_some() { "（用户重试）" } else { "（严格按配置）" }
        ));
        let result: AppResult<bool> = match method {
            "agent" => auth_agent(handle, shared).await,
            "publicKey" => {
                auth_publickey(handle, shared, &profile, secrets_bundle, override_creds.as_ref()).await
            }
            "password" => auth_password(handle, shared, secrets_bundle, override_creds.as_ref()).await,
            "keyboardInteractive" => auth_ki(handle, shared, secrets_bundle).await,
            _ => Ok(false),
        };
        match result {
            Ok(true) => {
                crate::logger::log(&format!("认证成功 via {method}"));
                return Ok(());
            }
            Err(AppError::Cancelled) => return Err(AppError::Cancelled),
            _ => {}
        }
        let err_text = match &result {
            Ok(false) => format!("{method_name}认证被服务器拒绝"),
            Err(e) => format!("{method_name}认证出错: {e}"),
            Ok(true) => unreachable!(),
        };

        // 弹统一认证对话框：重输密码 / 换密钥 / 切换认证方式
        let user_keys: Vec<String> = crate::commands::userkeys::user_key_list()
            .map(|list| list.into_iter().map(|k| k.name).collect())
            .unwrap_or_default();
        let answer = shared
            .ask_frontend(
                "auth-retry",
                PromptPayload::AuthRetry {
                    host: shared.host.clone(),
                    user: shared.user.clone(),
                    method: method.to_string(),
                    error: err_text,
                    key_path: profile.private_key_path.clone(),
                    user_keys,
                    profile_auth: profile_auth.clone(),
                    user_key_name: user_key_name.clone(),
                },
                600,
            )
            .await?;
        if answer.get("cancel").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(AppError::Cancelled);
        }
        let m = answer.get("method").and_then(|v| v.as_str()).unwrap_or("password");
        method = match m {
            "userKey" | "publicKey" => "publicKey",
            "keyboardInteractive" => "keyboardInteractive",
            "agent" => "agent",
            _ => "password",
        };
        override_creds = Some(answer);
    }
}

/// 平台相关的 ssh-agent 连接
#[cfg(unix)]
async fn auth_agent(handle: &mut Handle<SshHandler>, shared: &Arc<SessionShared>) -> AppResult<bool> {
    let mut agent: AgentClient<tokio::net::UnixStream> = AgentClient::connect_env()
        .await
        .map_err(|e| AppError::msg(format!("无法连接 ssh-agent（SSH_AUTH_SOCK）: {e}")))?;
    let identities = agent.request_identities().await.unwrap_or_default();
    let user = shared.user.clone();
    for key in identities {
        let (agent2, result) = handle.authenticate_future(&user, key, agent).await;
        agent = agent2;
        match result {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) => return Err(AppError::msg(format!("agent 签名失败: {e}"))),
        }
    }
    Ok(false)
}

#[cfg(windows)]
async fn auth_agent(handle: &mut Handle<SshHandler>, shared: &Arc<SessionShared>) -> AppResult<bool> {
    // 优先 Windows OpenSSH 命名管道，回落 Pageant
    match AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        Ok(agent) => return run_agent_identities(handle, shared, agent).await,
        Err(_) => {}
    }
    let agent = AgentClient::connect_pageant().await;
    run_agent_identities(handle, shared, agent).await
}

#[cfg(windows)]
async fn run_agent_identities<S>(handle: &mut Handle<SshHandler>, shared: &Arc<SessionShared>, mut agent: AgentClient<S>) -> AppResult<bool>
where
    S: russh::keys::agent::client::AgentStream + Unpin + Send + 'static,
{
    let identities = agent.request_identities().await.unwrap_or_default();
    let user = shared.user.clone();
    for key in identities {
        let (agent2, result) = handle.authenticate_future(&user, key, agent).await;
        agent = agent2;
        match result {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) => return Err(AppError::msg(format!("agent 签名失败: {e}"))),
        }
    }
    Ok(false)
}

/// 公钥认证：加载私钥（口令来自已存凭据或统一认证对话框的临时输入）并认证。
/// 加载失败（含口令缺失/错误）与被服务器拒绝等一切失败，
/// 都交回 authenticate() 的统一认证对话框处理，不再弹旧版密码框。
async fn auth_publickey(
    handle: &mut Handle<SshHandler>,
    shared: &Arc<SessionShared>,
    profile: &Profile,
    secrets_bundle: &secrets::ProfileSecrets,
    creds: Option<&serde_json::Value>,
) -> AppResult<bool> {
    // 统一认证对话框重试：可换密钥（用户密钥名 → ~/.ssh/<name>，或直接指定配置中的文件路径）
    let mut profile = profile.clone();
    let mut pass_override: Option<String> = None;
    if let Some(v) = creds {
        let key_path = v.get("keyPath").and_then(|x| x.as_str()).unwrap_or("");
        let key_name = v.get("keyName").and_then(|x| x.as_str()).unwrap_or("");
        if !key_path.is_empty() {
            profile.private_key_path = key_path.to_string();
        } else if !key_name.is_empty() {
            if let Some(home) = dirs::home_dir() {
                profile.private_key_path = home.join(".ssh").join(key_name).to_string_lossy().to_string();
            }
        }
        let pp = v.get("passphrase").and_then(|x| x.as_str()).unwrap_or("");
        if !pp.is_empty() {
            pass_override = Some(pp.to_string());
        }
    }
    if profile.private_key_path.trim().is_empty() {
        return Err(AppError::msg("未配置私钥文件"));
    }
    let path = expand_tilde(&profile.private_key_path);

    // 口令尝试顺序（OpenSSH 同款策略）：对话框口令优先，其次已存凭据，最后总是兜底
    // 尝试「无口令」——密钥换成无口令密钥后，配置里残留的旧口令不应导致加载失败。
    let candidate = pass_override
        .or_else(|| secrets_bundle.passphrase.clone())
        .filter(|s| !s.is_empty());
    let mut attempts: Vec<Option<String>> = Vec::new();
    if let Some(pp) = &candidate {
        attempts.push(Some(pp.clone()));
    }
    attempts.push(None);

    let mut last_err = AppError::msg("私钥加载失败");
    for pp in attempts {
        match keys::load_secret_key(&path, pp.as_deref()) {
            Ok(key) => {
                let result = handle.authenticate_publickey(&shared.user.clone(), Arc::new(key)).await?;
                return Ok(result);
            }
            Err(e) => {
                last_err = AppError::msg(format!("私钥加载失败: {e}"));
            }
        }
    }
    Err(last_err)
}

/// 密码认证：优先统一认证对话框带来的临时密码（勾选记住才落盘），
/// 其次已存凭据。无任何凭据时返回错误，由 authenticate() 弹统一对话框让用户输入。
async fn auth_password(
    handle: &mut Handle<SshHandler>,
    shared: &Arc<SessionShared>,
    secrets_bundle: &secrets::ProfileSecrets,
    creds: Option<&serde_json::Value>,
) -> AppResult<bool> {
    let from_dialog = creds.and_then(|v| {
        v.get("password")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|p| {
                if v.get("rememberPassword").and_then(|x| x.as_bool()).unwrap_or(false) {
                    let mut bundle = secrets::get(&shared.profile_id).unwrap_or_default();
                    bundle.password = Some(p.to_string());
                    let _ = secrets::set(&shared.profile_id, &bundle);
                    let _ = crate::profile_store::mark_stored_password(&shared.profile_id);
                    crate::logger::log(&format!("profile={} 记住密码已更新", shared.profile_id));
                }
                *shared.password.lock().unwrap() = Some(p.to_string());
                p.to_string()
            })
    });
    let pwd = from_dialog
        .or_else(|| secrets_bundle.password.clone())
        .or_else(|| shared.password.lock().unwrap().clone());
    let Some(pwd) = pwd else {
        return Err(AppError::msg("未配置密码"));
    };
    handle
        .authenticate_password(&shared.user.clone(), pwd)
        .await
        .map_err(|e| AppError::msg(format!("密码认证错误: {e}")))
}

/// keyboard-interactive：调用方驱动的 InfoRequest 循环
async fn auth_ki(
    handle: &mut Handle<SshHandler>,
    shared: &Arc<SessionShared>,
    secrets_bundle: &secrets::ProfileSecrets,
) -> AppResult<bool> {
    let stored = secrets_bundle.password.clone();
    let mut response = handle
        .authenticate_keyboard_interactive_start(&shared.user.clone(), None)
        .await
        .map_err(|e| AppError::msg(format!("KI 认证错误: {e}")))?;

    loop {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest { name, instructions, prompts } => {
                let ki_prompts: Vec<KiPrompt> = prompts
                    .iter()
                    .map(|p| KiPrompt { text: p.prompt.clone(), echo: p.echo })
                    .collect();

                // 自动填充：全部为非回显且文本含 password 的提问时使用已存口令
                let answers: Vec<String> = if let Some(pwd) = &stored {
                    let auto = !ki_prompts.is_empty()
                        && ki_prompts.iter().all(|p| !p.echo && p.text.to_lowercase().contains("password"));
                    if auto {
                        vec![pwd.clone(); ki_prompts.len()]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                let answers = if answers.is_empty() {
                    let answer = shared
                        .ask_frontend(
                            "ki",
                            PromptPayload::KeyboardInteractive {
                                host: shared.host.clone(),
                                user: shared.user.clone(),
                                name: name.clone(),
                                instruction: instructions.clone(),
                                prompts: ki_prompts,
                            },
                            300,
                        )
                        .await
                        .unwrap_or(serde_json::json!({ "answers": Vec::<String>::new() }));
                    answer
                        .get("answers")
                        .and_then(|a| a.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
                        .unwrap_or_default()
                } else {
                    answers
                };

                response = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|e| AppError::msg(format!("KI 认证错误: {e}")))?;
            }
        }
    }
}

/// 建立完整会话链（跳板机递归 + 认证），成功返回最终已认证 handle
pub async fn connect_and_auth(shared: Arc<SessionShared>, profile: Profile) -> AppResult<Handle<SshHandler>> {
    let chain = resolve_chain(&profile, 0)?;
    let timeout_ms = profile.connect_timeout.max(1000);

    let mut last_handle: Option<Handle<SshHandler>> = None;
    let total = chain.len();

    for (idx, hop) in chain.iter().enumerate() {
        let is_final = idx + 1 == total;
        let hop_secrets = secrets::get(&hop.id).unwrap_or_default();

        let transport = if idx == 0 {
            crate::ssh::transport::connect_transport(hop, None).await?
        } else {
            // 上一跳（跳板）上开 direct-tcpip 通道作为本跳传输
            let prev = last_handle.as_mut().expect("跳板 handle 缺失");
            let ch = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                prev.channel_open_direct_tcpip::<String, String>(hop.host.clone(), hop.port as u32, "127.0.0.1".to_string(), 0),
            )
            .await
            .map_err(|_| AppError::Timeout)??;
            let stream = ch.into_stream();
            crate::ssh::transport::connect_transport(hop, Some(Box::new(stream))).await?
        };

        let stream = crate::ssh::transport::transport_to_stream(transport)?;

        // 每跳独立的 hostkey 校验上下文
        let hop_shared = if is_final {
            shared.clone()
        } else {
            Arc::new(SessionShared {
                sid: shared.sid,
                profile_id: hop.id.clone(),
                emit: shared.emit.clone(),
                host: hop.host.clone(),
                port: hop.port,
                user: hop.username.clone(),
                password: Mutex::new(hop_secrets.password.clone()),
                known_hosts: KnownHosts,
                pending: Mutex::new(HashMap::new()),
                forwarded_tx: Mutex::new(None),
                closed: AtomicBool::new(false),
                auto_accept_hostkey: false,
            })
        };

        let config = Arc::new(client::Config {
            keepalive_interval: Some(std::time::Duration::from_millis(hop.keepalive_interval.max(1000))),
            keepalive_max: hop.keepalive_count_max as usize,
            ..client::Config::default()
        });

        let handler = SshHandler { shared: hop_shared.clone() };
        let mut handle: Handle<SshHandler> = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms * 2),
            client::connect_stream(config, stream, handler),
        )
        .await
        .map_err(|_| AppError::Timeout)?
        .map_err(|e| AppError::msg(format!("SSH 握手失败（{}:{}）: {e}", hop.host, hop.port)))?;

        authenticate(&mut handle, &hop_shared, hop, &hop_secrets).await?;

        last_handle = Some(handle);
    }

    last_handle.ok_or_else(|| AppError::msg("连接失败"))
}

/// base64 工具
pub fn b64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub fn b64_decode(s: &str) -> AppResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| AppError::msg(format!("base64 解码失败: {e}")))
}
