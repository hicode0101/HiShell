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
        .invoke_handler(tauri::generate_handler![
            commands::fs::fs_list_dir,
            commands::fs::fs_special_dir,
            commands::app::app_info,
            commands::profiles::profile_list,
            commands::profiles::profile_get,
            commands::profiles::profile_save,
            commands::profiles::profile_delete,
            commands::ssh::ssh_connect,
            commands::ssh::ssh_prompt_response,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_disconnect,
            commands::sftp::sftp_list,
            commands::sftp::sftp_mkdir,
            commands::sftp::sftp_rm,
            commands::sftp::sftp_rename,
            commands::sftp::sftp_stat,
            commands::sftp::sftp_exists,
            commands::sftp::sftp_mkdirs,
            commands::sftp::local_mkdirs,
            commands::transfers::transfer_enqueue,
            commands::transfers::transfer_pause,
            commands::transfers::transfer_resume,
            commands::transfers::transfer_cancel,
            commands::forwards::forward_add,
            commands::forwards::forward_remove,
            commands::forwards::forward_list,
        ])
        .run(tauri::generate_context!())
        .expect("HiShell 启动失败");
}
