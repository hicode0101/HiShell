//! 本地终端：portable-pty 驱动的本机 shell（Windows ConPTY / Unix openpty）。
//! 事件流与 SSH 会话一致：local-data（base64）下行、local-status 状态。

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use portable_pty::PtySize;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/* ---------------- 状态 ---------------- */

pub struct LocalHandle {
    pub closed: AtomicBool,
    pub exited_emitted: AtomicBool,
    pub writer: Mutex<Option<Box<dyn std::io::Write + Send>>>,
    pub master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    pub child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
    pub shell: String,
}

#[derive(Default)]
struct LocalInner {
    next_id: AtomicU64,
    items: Mutex<HashMap<u32, Arc<LocalHandle>>>,
}

/// 全局本地终端管理器（tauri State，可克隆进线程）
#[derive(Default, Clone)]
pub struct LocalManager {
    inner: Arc<LocalInner>,
}

impl LocalManager {
    fn new_id(&self) -> u32 {
        (self.inner.next_id.fetch_add(1, Ordering::Relaxed) + 1) as u32
    }
    pub fn get(&self, id: u32) -> Option<Arc<LocalHandle>> {
        self.inner.items.lock().unwrap().get(&id).cloned()
    }
    fn remove(&self, id: u32) -> Option<Arc<LocalHandle>> {
        self.inner.items.lock().unwrap().remove(&id)
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LocalDataEvent {
    pub lid: u32,
    pub data: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LocalStatusEvent {
    pub lid: u32,
    pub status: String, // exited
}

/* ---------------- shell 选择 ---------------- */

#[cfg(windows)]
fn shell_candidates() -> Vec<&'static str> {
    vec!["powershell.exe", "cmd.exe"]
}

#[cfg(not(windows))]
fn shell_candidates() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(s) = std::env::var("SHELL") {
        if !s.trim().is_empty() {
            out.push(s);
        }
    }
    for c in ["/bin/bash", "/bin/sh", "/bin/zsh"] {
        if std::path::Path::new(c).exists() {
            out.push(c.to_string());
        }
    }
    out
}

/* ---------------- 命令 ---------------- */

/// 启动本地 shell，返回 lid；输出经 local-data 事件推送
#[tauri::command]
pub fn local_start(app: AppHandle, mgr: State<'_, LocalManager>) -> Result<u32, String> {
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("PTY 分配失败: {e}"))?;

    let candidates = shell_candidates();
    if candidates.is_empty() {
        return Err("未找到可用的本地 shell".into());
    }

    let home = dirs::home_dir();
    let mut spawned: Option<(Box<dyn portable_pty::Child + Send + Sync>, String)> = None;
    let mut last_err = String::new();
    for shell in &candidates {
        let mut cmd = portable_pty::CommandBuilder::new(shell);
        if let Some(h) = &home {
            let _ = cmd.cwd(h);
        }
        cmd.env("TERM", "xterm-256color");
        match pair.slave.spawn_command(cmd) {
            Ok(child) => {
                spawned = Some((child, shell.to_string()));
                break;
            }
            Err(e) => last_err = format!("{shell}: {e}"),
        }
    }
    let Some((child, shell)) = spawned else {
        return Err(format!("本地 shell 启动失败（{last_err}）"));
    };
    drop(pair.slave);

    let writer = pair.master.take_writer().map_err(|e| format!("PTY 写端获取失败: {e}"))?;
    let reader = pair.master.try_clone_reader().map_err(|e| format!("PTY 读端获取失败: {e}"))?;

    let lid = mgr.new_id();
    let handle = Arc::new(LocalHandle {
        closed: AtomicBool::new(false),
        exited_emitted: AtomicBool::new(false),
        writer: Mutex::new(Some(writer)),
        master: Mutex::new(Some(pair.master)),
        child: Mutex::new(Some(child)),
        shell,
    });
    mgr.inner.items.lock().unwrap().insert(lid, handle.clone());

    // 读泵：阻塞读 → local-data 事件（独立线程，退出时回收子进程并广播）
    let app2 = app.clone();
    let mgr2 = mgr.inner.clone();
    let handle2 = handle.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if handle2.closed.load(Ordering::Relaxed) {
                        break;
                    }
                    let data = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                    let _ = app2.emit("local-data", LocalDataEvent { lid, data });
                }
                Err(_) => break,
            }
        }
        // 回收子进程（避免 unix 僵尸），随后广播退出
        if let Some(mut child) = handle2.child.lock().unwrap().take() {
            let _ = child.wait();
        }
        if !handle2.exited_emitted.swap(true, Ordering::Relaxed) {
            let _ = app2.emit("local-status", LocalStatusEvent { lid, status: "exited".into() });
        }
        mgr2.items.lock().unwrap().remove(&lid);
    });

    crate::logger::log(&format!("local shell started lid={lid} shell={}", handle.shell));
    Ok(lid)
}

/// 写入本地 shell（base64）
#[tauri::command]
pub fn local_write(mgr: State<'_, LocalManager>, lid: u32, data: String) -> Result<(), String> {
    let handle = mgr.get(lid).ok_or("本地终端不存在")?;
    if handle.closed.load(Ordering::Relaxed) {
        return Err("本地终端已退出".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("数据解码失败: {e}"))?;
    use std::io::Write;
    let mut guard = handle.writer.lock().unwrap();
    if let Some(w) = guard.as_mut() {
        w.write_all(&bytes).map_err(|e| format!("写入失败: {e}"))?;
        w.flush().map_err(|e| format!("写入失败: {e}"))?;
    }
    Ok(())
}

/// 调整本地终端尺寸
#[tauri::command]
pub fn local_resize(mgr: State<'_, LocalManager>, lid: u32, cols: u16, rows: u16) -> Result<(), String> {
    let handle = mgr.get(lid).ok_or("本地终端不存在")?;
    let guard = handle.master.lock().unwrap();
    if let Some(m) = guard.as_ref() {
        m.resize(PtySize { rows: rows.max(2), cols: cols.max(2), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("resize 失败: {e}"))?;
    }
    Ok(())
}

/// 关闭本地终端：杀进程 + 清理注册
#[tauri::command]
pub fn local_stop(app: AppHandle, mgr: State<'_, LocalManager>, lid: u32) -> Result<(), String> {
    if let Some(handle) = mgr.remove(lid) {
        handle.closed.store(true, Ordering::Relaxed);
        if let Some(mut child) = handle.child.lock().unwrap().take() {
            let _ = child.kill();
        }
        if !handle.exited_emitted.swap(true, Ordering::Relaxed) {
            let _ = app.emit("local-status", LocalStatusEvent { lid, status: "exited".into() });
        }
    }
    Ok(())
}
