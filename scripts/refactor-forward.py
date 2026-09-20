# -*- coding: utf-8 -*-
"""forward.rs / commands 重构：handle Arc<AsyncMutex> 共享"""
import io

# ---- forward.rs ----
p = 'src-tauri/src/ssh/forward.rs'
s = io.open(p, encoding='utf-8').read()

s = s.replace('use super::manager::SessionHandle;', 'use super::manager::{SessionHandle, SshHandler};')

old = '''    /// 停止指定转发（本地关闭 listener；远程调用 cancel-forward）
    pub async fn stop(&self, rule_id: &str, handle: Option<&mut Handle<super::manager::SshHandler>>) {
        let running = {
            let mut map = self.running.lock().unwrap();
            map.remove(rule_id)
        };
        if let Some(r) = running {
            r.shutdown.store(true, Ordering::Relaxed);
            r.task.abort();
            if r.rule.kind == "remote" {
                if let Some(h) = handle {
                    let _ = h.cancel_tcpip_forward(r.rule.bind_host.clone(), r.rule.bind_port as u32).await;
                }
            }
        }
    }

    pub async fn stop_all(&self, handle: Option<&mut Handle<super::manager::SshHandler>>) {
        let ids: Vec<String> = self.running.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.stop(&id, handle).await;
        }
    }'''
new = '''    /// 停止指定转发（本地关闭 listener；远程调用 cancel-forward）
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
    }'''
assert old in s
s = s.replace(old, new)

old = '''    let shutdown = shutdown_flag();
    let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handle = session.handle.lock().unwrap().clone();
    let handle = handle.ok_or_else(|| AppError::msg("会话未连接"))?;'''
new = '''    let shutdown = shutdown_flag();
    let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handle = session.handle.clone();'''
assert old in s
s = s.replace(old, new)

s = s.replace('''    let task = match rule.kind.as_str() {
        "local" => tokio::spawn(local_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        "dynamic" => tokio::spawn(dynamic_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        "remote" => tokio::spawn(remote_forward(session.clone(), handle, rule.clone(), shutdown.clone(), error.clone())),
        other => return Err(AppError::msg(format!("未知转发类型: {other}"))),
    };
    map.insert(rule.id.clone(), Running { rule, shutdown, task, error });
    Ok(())''',
'''    let task = match rule.kind.as_str() {
        "local" => tokio::spawn(local_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        "dynamic" => tokio::spawn(dynamic_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        "remote" => tokio::spawn(remote_forward(handle, rule.clone(), shutdown.clone(), error.clone())),
        other => return Err(AppError::msg(format!("未知转发类型: {other}"))),
    };
    session.forwards.running.lock().unwrap().insert(
        rule.id.clone(),
        Running { rule, shutdown, task, error },
    );
    Ok(())''')

# start() 上半段包含 map 声明 — 修正引用
old = '''    let mut map = session.forwards.running.lock().unwrap();
    if map.contains_key(&rule.id) {
        return Ok(());
    }'''
new = '''    {
        let mut map = session.forwards.running.lock().unwrap();
        if map.contains_key(&rule.id) {
            return Ok(());
        }
    }'''
assert old in s
s = s.replace(old, new)

s = s.replace('''async fn local_forward(
    handle: Handle<super::manager::SshHandler>,
    rule: ForwardRule,''',
'''async fn local_forward(
    handle: Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    rule: ForwardRule,''')
s = s.replace('''async fn dynamic_forward(
    handle: Handle<super::manager::SshHandler>,
    rule: ForwardRule,''',
'''async fn dynamic_forward(
    handle: Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    rule: ForwardRule,''')
s = s.replace('''async fn remote_forward(
    session: Arc<SessionHandle>,
    mut handle: Handle<super::manager::SshHandler>,
    rule: ForwardRule,''',
'''async fn remote_forward(
    handle: Arc<tokio::sync::Mutex<Option<Handle<SshHandler>>>>,
    rule: ForwardRule,''')

s = s.replace('''async fn open_direct(
    handle: &Handle<super::manager::SshHandler>,
    host: &str,
    port: u16,
    originator: &str,
) -> AppResult<impl AsyncRead + AsyncWrite + Unpin> {
    let channel = handle
        .channel_open_direct_tcpip(host.to_string(), port as u32, originator.to_string(), 0)
        .await?;
    Ok(channel.into_stream())
}''',
'''async fn open_direct(
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
}''')

s = s.replace('let mut handle = handle.clone();', 'let handle = handle.clone();')

old = '''    match handle.tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32).await {
        Ok(_) => {}
        Err(e) => {
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
    let _ = session.shared.clone();
}'''
new = '''    {
        let mut guard = handle.lock().await;
        let handle = guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
        if let Err(e) = handle.tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32).await {
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
}'''
assert old in s
s = s.replace(old, new)

io.open(p, 'w', encoding='utf-8').write(s)
print('forward.rs done')

# ---- commands/ssh.rs ----
p = 'src-tauri/src/commands/ssh.rs'
s = io.open(p, encoding='utf-8').read()
old = '''#[tauri::command]
pub async fn ssh_resize(sessions: State<'_, Sessions>, sid: u32, cols: u32, rows: u32) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let mut ch = session.shell_channel.lock().unwrap();
    let Some(channel) = ch.as_mut() else { return Ok(()) };
    channel.window_change(cols, rows, 0, 0).await.map_err(|e| format!("resize 失败: {e}"))
}'''
new = '''#[tauri::command]
pub async fn ssh_resize(sessions: State<'_, Sessions>, sid: u32, cols: u32, rows: u32) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let _ = session.resize_tx.send((cols, rows));
    Ok(())
}'''
assert old in s
s = s.replace(old, new)

old = '''    session.closed.store(true, Ordering::Relaxed);
    let handle = session.handle.lock().unwrap().clone();
    if let Some(mut h) = handle {
        let _ = h.disconnect(russh::Disconnect::ByApplication, "user disconnect", "en").await;
    }
    ssh::cleanup_session(&sessions.0, sid, &app, "用户断开");
    Ok(())'''
new = '''    session.closed.store(true, Ordering::Relaxed);
    let mut guard = session.handle.lock().await;
    if let Some(h) = guard.as_mut() {
        let _ = h.disconnect(russh::Disconnect::ByApplication, "user disconnect", "en").await;
    }
    ssh::cleanup_session(&sessions.0, sid, &app, "用户断开");
    Ok(())'''
assert old in s
s = s.replace(old, new)
io.open(p, 'w', encoding='utf-8').write(s)
print('commands/ssh.rs done')

# ---- commands/forwards.rs ----
p = 'src-tauri/src/commands/forwards.rs'
s = io.open(p, encoding='utf-8').read()
old = '''    let mut handle = session.handle.lock().unwrap().clone();
    session
        .forwards
        .stop(&req.rule_id, handle.as_mut())
        .await;
    Ok(())'''
new = '''    session.forwards.stop(&req.rule_id, &session.handle).await;
    Ok(())'''
assert old in s
s = s.replace(old, new)
io.open(p, 'w', encoding='utf-8').write(s)
print('commands/forwards.rs done')

# ---- commands/sftp.rs ----
p = 'src-tauri/src/commands/sftp.rs'
s = io.open(p, encoding='utf-8').read()
s = s.replace('Ok(entries.iter().map(map_entry).collect())', 'Ok(entries.into_iter().map(|e| map_entry(&e)).collect())')
s = s.replace('for e in &entries {', 'for e in entries {')
old = '''            let boxed: Box<dyn std::future::Future<Output = Result<(), russh_sftp::client::error::Error>> + Send> =
                if is_dir {
                    Box::new(delete_recursive(&sftp, &child))
                } else {
                    Box::new(sftp.remove_file(&child))
                };
            boxed.await.map_err(|e| e.to_string())?;'''
new = '''            if is_dir {
                delete_recursive(&sftp, &child).await.map_err(|e| e.to_string())?;
            } else {
                sftp.remove_file(&child).await.map_err(|e| e.to_string())?;
            }'''
assert old in s
s = s.replace(old, new)
io.open(p, 'w', encoding='utf-8').write(s)
print('commands/sftp.rs done')
