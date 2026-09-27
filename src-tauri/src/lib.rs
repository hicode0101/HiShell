#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod known_hosts;
mod logger;
mod paths;
pub mod profile_store;
pub mod secrets;
pub mod ssh;

use ssh::{Sessions, Transfers};

pub fn run() {
    let _ = paths::ensure_config_dir();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Sessions(std::sync::Arc::new(ssh::SessionManager::new())))
        .manage(Transfers(std::sync::Arc::new(ssh::TransferManager::new())))
        .manage(commands::local::LocalManager::default())
        .manage(commands::userkeys::KeyGenSessions::default())
        .invoke_handler(tauri::generate_handler![
            commands::fs::fs_list_dir,
            commands::fs::fs_special_dir,
            commands::fs::fs_rename,
            commands::fs::fs_delete,
            commands::fs::fs_mkdir,
            commands::fs::fs_stat,
            commands::fs::open_path,
            commands::app::app_info,
            commands::app::open_config_dir,
            commands::app::open_url,
            commands::profiles::profile_list,
            commands::profiles::profile_get,
            commands::profiles::profile_save,
            commands::profiles::profile_copy,
            commands::profiles::profile_delete,
            commands::profiles::profile_reorder,
            commands::profiles::profile_export,
            commands::profiles::profile_import,
            commands::ssh::ssh_connect,
            commands::ssh::ssh_prompt_response,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_disconnect,
            commands::ssh::ssh_duplicate_channel,
            commands::ssh::ssh_test,
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_rm,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_chmod,
            commands::sftp::sftp_stat,
            commands::sftp::sftp_exists,
            commands::sftp::sftp_mkdirs,
            commands::sftp::sftp_read_file,
            commands::sftp::sftp_write_file,
            commands::sftp::local_mkdirs,
            commands::transfers::transfer_enqueue,
            commands::transfers::transfer_pause,
            commands::transfers::transfer_resume,
            commands::transfers::transfer_cancel,
            commands::forwards::forward_add,
            commands::forwards::forward_remove,
            commands::forwards::forward_list,
            commands::import::import_scan,
            commands::import::import_apply,
            commands::userkeys::user_key_list,
            commands::userkeys::user_key_delete,
            commands::userkeys::user_key_generate_start,
            commands::userkeys::user_key_generate_finish,
            commands::userkeys::user_key_generate_cancel,
            commands::userkeys::user_key_save_public,
            commands::userkeys::user_key_rename,
            commands::userkeys::user_key_inspect,
            commands::userkeys::user_key_import,
            commands::userkeys::user_key_public_content,
            commands::userkeys::user_key_export_private_to,
            commands::userkeys::user_key_write_text,
            commands::local::local_start,
            commands::local::local_write,
            commands::local::local_resize,
            commands::local::local_stop,
        ])
        .setup(|_app| {
            // 启动完全打开后异步清理历史临时配置（快速连接每次连接都会落盘一份，
            // 对列表隐藏且无入口删除，反复使用会无限累积）。
            // 延迟数秒 + 独立后台线程执行：避开启动阶段的磁盘/凭据管理器争抢，
            // 全程不触碰 UI 线程，不产生卡顿感。
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let n = profile_store::purge_temporary();
                if n > 0 {
                    crate::logger::log(&format!("启动清理：已移除 {n} 条历史临时配置"));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("HiShell 启动失败");
}
