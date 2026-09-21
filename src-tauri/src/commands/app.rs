/// 应用元信息
#[tauri::command]
pub fn app_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "name": "HiShell",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "Tauri 桌面模式",
    }))
}

/// 用系统默认浏览器打开 http/https 链接
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("不支持的链接: {url}"));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("打开链接失败: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&url).spawn().map_err(|e| format!("打开链接失败: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&url).spawn().map_err(|e| format!("打开链接失败: {e}"))?;
    }
    Ok(())
}

/// 在系统文件管理器中打开 HiShell 配置目录（profiles.json 所在处）
#[tauri::command]
pub fn open_config_dir() -> Result<(), String> {
    let dir = crate::paths::config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();
    result.map(|_| ()).map_err(|e| format!("打开目录失败: {e}"))
}
