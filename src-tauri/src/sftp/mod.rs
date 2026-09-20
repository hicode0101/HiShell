//! SFTP 传输服务（M4 里程碑填充）
//!
//! 规划：
//! - 目录浏览：sftp://{sid}/entries 事件
//! - 传输任务：256KB 读块、.hishell-upload 临时文件 + rename 原子上传（对齐 Tabby）
//! - 并发 4/会话、进度事件 100ms 节流、断点续传（offset 记录，超越 Tabby）
