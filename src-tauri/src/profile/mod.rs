//! 连接配置存储（M2 里程碑填充）
//!
//! 规划：
//! - ProfileStore：%APPDATA%\hishell\profiles.json + .backup 原子双写（借鉴 Tabby）
//! - SecretStore：keyring crate（Windows Credential Manager）存密码/私钥口令
//! - 导入器：Xshell / Tabby config.yaml / OpenSSH config
