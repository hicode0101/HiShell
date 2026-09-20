//! 轻量文件日志（诊断用）：写入 配置目录/hishell.log

use std::io::Write;
use std::sync::Mutex;

use once_cell::sync::Lazy;

static LOG_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn log(msg: &str) {
    let path = crate::paths::config_dir().join("hishell.log");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _guard = LOG_LOCK.lock();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{ts}] {msg}");
    }
    // debug 构建同时输出到 stderr
    #[cfg(debug_assertions)]
    eprintln!("[hishell] {msg}");
}
