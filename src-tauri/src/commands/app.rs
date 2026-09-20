/// 应用元信息
#[tauri::command]
pub fn app_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "name": "HiShell",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": "Tauri 桌面模式",
    }))
}
