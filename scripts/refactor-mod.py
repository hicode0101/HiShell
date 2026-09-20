# -*- coding: utf-8 -*-
"""ssh/mod.rs 重构：handle 共享 + resize 通道"""
import io

p = 'src-tauri/src/ssh/mod.rs'
s = io.open(p, encoding='utf-8').read()

old_setup = s[s.index('    let mut handle = manager::connect_and_auth'):]
old_setup = old_setup[:old_setup.index('    mgr.insert(session.clone());') + len('    mgr.insert(session.clone());')]

new_setup = '''    let handle = manager::connect_and_auth(shared.clone(), profile.clone()).await?;
    let handle = Arc::new(tokio::sync::Mutex::new(Some(handle)));

    // 远程转发入站通道
    let (fwd_tx, fwd_rx) = mpsc::unbounded_channel();
    *shared.forwarded_tx.lock().unwrap() = Some(fwd_tx);

    // 打开 shell 通道（PTY + shell），语义对齐 Tabby openShellChannel
    {
        let mut guard = handle.lock().await;
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
        std::mem::forget(channel); // 通道 id 由 russh 会话管理，此处仅弃用句柄
    }

    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u32, u32)>();

    // 写入序列化通道
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let session = Arc::new(SessionHandle {
        sid,
        profile_id: profile.id.clone(),
        shared: shared.clone(),
        handle,
        closed: AtomicBool::new(false),
        write_tx,
        forwards: Arc::new(forward::ForwardManager::new()),
        sftp: Mutex::new(None),
        resize_tx,
    });
    mgr.insert(session.clone());'''

assert old_setup.startswith('    let mut handle'), old_setup[:80]
s = s.replace(old_setup, new_setup)

# 状态推送后替换旧数据泵为 pump_channel
old_pump = s[s.index('    emit_status(&app, sid, "connected", None);'):]
old_pump = old_pump[:old_pump.index('    cleanup_session(&mgr, sid, &app, "会话已关闭");\n    Ok(())\n}') + len('    cleanup_session(&mgr, sid, &app, "会话已关闭");\n    Ok(())\n}')]

new_pump = '''    emit_status(&app, sid, "connected", None);

    // 重新打开会话通道作为数据泵（读取流 + resize 请求）
    {
        let mut guard = handle.lock().await;
        let handle = guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
        let channel = handle.channel_open_session().await?;
        drop(guard);
        pump_channel(app.clone(), session.clone(), channel, resize_rx).await;
    }

    cleanup_session(&mgr, sid, &app, "会话已关闭");
    Ok(())
}

/// 数据泵：通道 → 前端（ssh-data 事件，base64），并处理 resize 请求
async fn pump_channel(
    app: AppHandle,
    session: Arc<SessionHandle>,
    mut channel: russh::Channel<russh::client::Msg>,
    mut resize_rx: mpsc::UnboundedReceiver<(u32, u32)>,
) {
    let sid = session.sid;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                if session.closed.load(Ordering::Relaxed) { break }
            }
            Some((cols, rows)) = resize_rx.recv() => {
                let _ = channel.window_change(cols, rows, 0, 0).await;
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
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
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                    _ => {}
                }
            }
        }
    }
}'''

s = s.replace(old_pump, new_pump)

# get_sftp async lock
s = s.replace('''    let mut handle_guard = session.handle.lock().unwrap();
    let handle = handle_guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
    let channel = handle.channel_open_session().await?;''',
'''    let mut handle_guard = session.handle.lock().await;
    let handle = handle_guard.as_mut().ok_or_else(|| AppError::msg("会话未连接"))?;
    let channel = handle.channel_open_session().await?;''')

# cleanup_session async
old_cleanup = '''    if let Some(session) = mgr.remove(sid) {
        session.closed.store(true, Ordering::Relaxed);
        let handle = session.handle.lock().unwrap().clone();
        let forwards = session.forwards.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(mut h) = handle {
                forwards.stop_all(Some(&mut h)).await;
            }
        });
        shared_closed(&session);
    }
    emit_status(app, sid, "disconnected", Some(message));'''
new_cleanup = '''    if let Some(session) = mgr.remove(sid) {
        session.closed.store(true, Ordering::Relaxed);
        let handle = session.handle.clone();
        let forwards = session.forwards.clone();
        tauri::async_runtime::spawn(async move {
            let mut guard = handle.lock().await;
            if let Some(h) = guard.as_mut() {
                forwards.stop_all(Some(h)).await;
            }
        });
    }
    emit_status(app, sid, "disconnected", Some(message));'''
assert old_cleanup in s
s = s.replace(old_cleanup, new_cleanup)
s = s.replace('''fn shared_closed(_s: &Arc<SessionHandle>) {
    // closed 标志已由 cleanup_session 设置
}

''', '')

io.open(p, 'w', encoding='utf-8').write(s)
print('mod.rs refactored')
