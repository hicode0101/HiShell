use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::ssh::{self, Sessions};
use russh_sftp::client::fs::DirEntry;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub mtime: u64,
    pub attrs: String,
}

fn attrs_string(permissions: Option<u32>) -> String {
    let Some(p) = permissions else { return String::new() };
    let file_type = match p & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };
    let mut s = String::with_capacity(10);
    s.push(file_type);
    for shift in [6u32, 3, 0] {
        let bits = (p >> shift) & 0o7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

fn map_entry(e: &DirEntry) -> RemoteEntry {
    let meta = e.metadata();
    let kind = if meta.is_dir() { "dir" } else { "file" };
    RemoteEntry {
        name: e.file_name(),
        kind: kind.into(),
        size: meta.size.unwrap_or(0),
        mtime: meta.mtime.map(|t| (t as u64) * 1000).unwrap_or(0),
        attrs: attrs_string(meta.permissions),
    }
}

/// 列出远程目录
#[tauri::command]
pub async fn sftp_list(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<Vec<RemoteEntry>, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    let entries = sftp.read_dir(&path).await.map_err(|e| ssh::sftp_error("读取目录", &path, e))?;
    Ok(entries.into_iter().map(|e| map_entry(&e)).collect())
}

#[tauri::command]
pub async fn sftp_mkdir(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    // 预检同名：SFTP 对已存在条目返回的是通用 Failure，无法区分原因，这里直接给出准确提示
    if sftp.metadata(&path).await.is_ok() {
        return Err(format!("创建文件夹失败: {path}: 已存在同名文件或文件夹"));
    }
    sftp.create_dir(&path).await.map_err(|e| ssh::sftp_error("创建文件夹", &path, e))
}

/// 删除文件/目录（目录递归）
#[tauri::command]
pub async fn sftp_rm(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    let meta = sftp.metadata(&path).await.map_err(|e| ssh::sftp_error("删除", &path, e))?;
    if meta.is_dir() {
        // 递归删除
        let entries = sftp.read_dir(&path).await.map_err(|e| ssh::sftp_error("删除", &path, e))?;
        for e in entries {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = format!("{}/{}", path.trim_end_matches('/'), name);
            let is_dir = e.metadata().is_dir();
            if is_dir {
                delete_recursive(sftp.clone(), child).await?;
            } else {
                sftp.remove_file(&child).await.map_err(|e| ssh::sftp_error("删除", &child, e))?;
            }
        }
        sftp.remove_dir(&path).await.map_err(|e| ssh::sftp_error("删除", &path, e))
    } else {
        sftp.remove_file(&path).await.map_err(|e| ssh::sftp_error("删除", &path, e))
    }
}

fn delete_recursive(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    dir: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
    Box::pin(delete_recursive_impl(sftp, dir))
}

async fn delete_recursive_impl(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    dir: String,
) -> Result<(), String> {
    let entries = sftp.read_dir(dir.clone()).await.map_err(|e| ssh::sftp_error("删除", &dir, e))?;
    for e in entries {
        let name = e.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let child = format!("{}/{}", dir.trim_end_matches('/'), name);
        if e.metadata().is_dir() {
            delete_recursive(sftp.clone(), child.clone()).await?;
        } else {
            sftp.remove_file(&child).await.map_err(|e| ssh::sftp_error("删除", &child, e))?;
        }
    }
    sftp.remove_dir(&dir).await.map_err(|e| ssh::sftp_error("删除", &dir, e))
}

#[tauri::command]
pub async fn sftp_rename(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
    new_path: String,
) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    // 预检目标已存在：SFTP rename 不覆盖同名条目，返回的通用 Failure 无法区分原因
    // （仅大小写变更时目标即源自身，跳过预检，避免在大小写不敏感的服务器上误报）
    if !new_path.eq_ignore_ascii_case(&path) && sftp.metadata(&new_path).await.is_ok() {
        return Err(format!("重命名失败: {new_path}: 目标路径已存在"));
    }
    sftp.rename(&path, &new_path).await.map_err(|e| ssh::sftp_error("重命名", &path, e))
}

#[tauri::command]
pub async fn sftp_stat(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
) -> Result<Option<RemoteEntry>, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    Ok(sftp.metadata(&path).await.ok().map(|m| RemoteEntry {
        name: path.split('/').last().unwrap_or("").to_string(),
        kind: if m.is_dir() { "dir".into() } else { "file".into() },
        size: m.size.unwrap_or(0),
        mtime: m.mtime.map(|t| (t as u64) * 1000).unwrap_or(0),
        attrs: attrs_string(m.permissions),
    }))
}

/// 远程路径是否存在 + 类型（供下载目录创建使用）
#[tauri::command]
pub async fn sftp_exists(sessions: tauri::State<'_, Sessions>, sid: u32, path: String) -> Result<bool, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    Ok(sftp.metadata(&path).await.is_ok())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MkdirsRequest {
    pub sid: u32,
    pub paths: Vec<String>,
}

/// 批量创建远程目录（递归上传文件夹前调用）
#[tauri::command]
pub async fn sftp_mkdirs(sessions: tauri::State<'_, Sessions>, sid: u32, paths: Vec<String>) -> Result<(), String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    for p in paths {
        if sftp.metadata(&p).await.is_ok() {
            continue;
        }
        // 逐级创建
        let mut partial = String::new();
        for seg in p.split('/').filter(|s| !s.is_empty()) {
            partial = format!("{}/{}", partial.trim_end_matches('/'), seg);
            if sftp.metadata(&partial).await.is_err() {
                sftp.create_dir(&partial).await.map_err(|e| ssh::sftp_error("创建目录", &partial, e))?;
            }
        }
    }
    Ok(())
}

/// 更改远程文件/目录权限（八进制，如 644；目录可递归）
#[tauri::command]
pub async fn sftp_chmod(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
    mode: u32,
    recursive: bool,
) -> Result<u32, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    if mode > 0o7777 {
        return Err("权限值无效（应为 0-7777 八进制）".into());
    }
    let count = chmod_recursive(sftp.clone(), path.clone(), mode, recursive)
        .await
        .map_err(|e| e.to_string())?;
    Ok(count)
}

fn chmod_recursive(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    path: String,
    mode: u32,
    recursive: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, String>> + Send>> {
    Box::pin(chmod_recursive_impl(sftp, path, mode, recursive))
}

async fn chmod_recursive_impl(
    sftp: std::sync::Arc<russh_sftp::client::SftpSession>,
    path: String,
    mode: u32,
    recursive: bool,
) -> Result<u32, String> {
    // SFTP SETSTAT 的 permissions 携带完整 st_mode：保留原类型位，仅替换权限位
    let meta = sftp.metadata(&path).await.map_err(|e| ssh::sftp_error("读取属性", &path, e))?;
    let type_bits = meta.permissions.unwrap_or(0o100000) & 0o170000;
    let attrs = russh_sftp::protocol::FileAttributes {
        permissions: Some(type_bits | (mode & 0o7777)),
        ..Default::default()
    };
    sftp.set_metadata(&path, attrs)
        .await
        .map_err(|e| ssh::sftp_error("更改权限", &path, e))?;
    let mut count = 1u32;
    if recursive && meta.is_dir() {
        let entries = sftp.read_dir(&path).await.map_err(|e| ssh::sftp_error("读取目录", &path, e))?;
        for e in entries {
            let name = e.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = format!("{}/{}", path.trim_end_matches('/'), name);
            count += chmod_recursive(sftp.clone(), child, mode, true).await?;
        }
    }
    Ok(count)
}

/// 上传/下载前确保本地父目录存在（下载递归时使用）
#[tauri::command]
pub async fn local_mkdirs(paths: Vec<String>) -> Result<(), String> {
    for p in paths {
        tokio::fs::create_dir_all(&p)
            .await
            .map_err(|e| AppError::msg(format!("创建目录 {p} 失败: {e}")))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/* ---------- 在线文本编辑（读/写远程文件内容） ---------- */

/// 在线编辑的文件大小上限：10 MB
const EDIT_MAX_SIZE: u64 = 10 * 1024 * 1024;
/// 写回时的字节上限（编辑中粘贴可能使内容增长，放宽到 20 MB）
const EDIT_WRITE_MAX: usize = 20 * 1024 * 1024;

/// 远程文本文件内容（已解码为 UTF-8 文本，行尾统一为 \n）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileContent {
    pub text: String,
    /// 检测到的编码名（WHATWG 标准名，如 UTF-8 / GBK / Shift_JIS）
    pub encoding: String,
    /// 原文件是否带 BOM
    pub bom: bool,
    /// 原文件行尾是否为 CRLF（保存时按此还原）
    pub crlf: bool,
    /// 解码时是否出现无法映射的字节（以替换符占位，保存将丢失原字节）
    pub lossy: bool,
    /// 原始字节数
    pub size: u64,
}

/// 三级识别并解码：BOM → 严格 UTF-8 → chardetng 特征探测。
/// 返回 (原始解码文本, 编码名, 是否带 BOM, 是否有不可映射字节)。
fn detect_and_decode(bytes: &[u8]) -> (String, String, bool, bool) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (text, _, had_errors) = encoding_rs::UTF_8.decode(&bytes[3..]);
        return (text.into_owned(), "UTF-8".into(), true, had_errors);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (text, _, had_errors) = encoding_rs::UTF_16LE.decode(bytes);
        return (text.into_owned(), "UTF-16LE".into(), true, had_errors);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, had_errors) = encoding_rs::UTF_16BE.decode(bytes);
        return (text.into_owned(), "UTF-16BE".into(), true, had_errors);
    }
    // 无 BOM 的 UTF-16：NUL 占比高时按 16 位解码。
    // 必须放在严格 UTF-8 检查之前——NUL 是合法 UTF-8 字节，否则会被误判为带 NUL 的 UTF-8
    let zeros = bytes.iter().filter(|&&b| b == 0).count();
    if zeros * 8 > bytes.len() {
        let enc = if bytes[0] == 0 { encoding_rs::UTF_16BE } else { encoding_rs::UTF_16LE };
        let (text, _, had_errors) = enc.decode(bytes);
        return (text.into_owned(), enc.name().to_string(), false, had_errors);
    }
    // 合法 UTF-8（含纯 ASCII）直接采用，避免特征探测误判
    if std::str::from_utf8(bytes).is_ok() {
        return (String::from_utf8_lossy(bytes).into_owned(), "UTF-8".into(), false, false);
    }
    let mut det = chardetng::EncodingDetector::new();
    det.feed(bytes, true);
    let enc = det.guess(None, true);
    let (text, _, had_errors) = enc.decode(bytes);
    (text.into_owned(), enc.name().to_string(), false, had_errors)
}

/// 文本按指定编码编码（CRLF 按需还原行尾；UTF-8/UTF-16 按需补 BOM）
fn encode_content(text: &str, encoding: &str, bom: bool, crlf: bool) -> Result<Vec<u8>, String> {
    let normalized = if crlf {
        text.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n")
    } else {
        text.replace("\r\n", "\n")
    };
    let Some(enc) = encoding_rs::Encoding::for_label(encoding.as_bytes()) else {
        return Err(format!("未知编码: {encoding}"));
    };
    // encoding_rs 的 encode() 对 UTF-16 目标会回退输出 UTF-8（见其文档第二返回值），
    // 这里手动打包 UTF-16 码元，避免把 UTF-16 文件写坏
    let utf16le = if std::ptr::eq(enc, encoding_rs::UTF_16LE) {
        Some(true)
    } else if std::ptr::eq(enc, encoding_rs::UTF_16BE) {
        Some(false)
    } else {
        None
    };
    let mut out: Vec<u8> = if let Some(le) = utf16le {
        let mut v = Vec::with_capacity(normalized.len() * 2);
        for unit in normalized.encode_utf16() {
            let bytes = if le { unit.to_le_bytes() } else { unit.to_be_bytes() };
            v.extend_from_slice(&bytes);
        }
        v
    } else {
        let (data, actual, _) = enc.encode(&normalized);
        if !std::ptr::eq(actual, enc) {
            return Err(format!("编码 {encoding} 不支持写回，已放弃保存"));
        }
        data.into_owned()
    };
    // encoding_rs 的 encode 不写 BOM：按原文件的 BOM 标记补回
    if bom {
        let prefix: &[u8] = if std::ptr::eq(enc, encoding_rs::UTF_8) {
            &[0xEF, 0xBB, 0xBF]
        } else if std::ptr::eq(enc, encoding_rs::UTF_16LE) {
            &[0xFF, 0xFE]
        } else if std::ptr::eq(enc, encoding_rs::UTF_16BE) {
            &[0xFE, 0xFF]
        } else {
            &[]
        };
        if !prefix.is_empty() {
            let mut with_bom = Vec::with_capacity(out.len() + prefix.len());
            with_bom.extend_from_slice(prefix);
            with_bom.append(&mut out);
            out = with_bom;
        }
    }
    Ok(out)
}

/// 读取远程文本文件内容（在线编辑用），自动识别编码/换行符
#[tauri::command]
pub async fn sftp_read_file(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
) -> Result<RemoteFileContent, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    let meta = sftp.metadata(&path).await.map_err(|e| ssh::sftp_error("读取文件", &path, e))?;
    if meta.is_dir() {
        return Err("无法编辑文件夹".into());
    }
    let size = meta.size.unwrap_or(0);
    if size > EDIT_MAX_SIZE {
        return Err(format!("文件过大（约 {} MB），不支持在线编辑（上限 10 MB）", size / 1024 / 1024));
    }
    use tokio::io::AsyncReadExt;
    let mut file = sftp.open(&path).await.map_err(|e| ssh::sftp_error("打开远程文件", &path, e))?;
    let mut bytes: Vec<u8> = Vec::with_capacity(size.min(EDIT_MAX_SIZE) as usize + 1);
    // take() 防御属性谎报/文件在读取期间被追加，避免内存失控
    (&mut file).take(EDIT_MAX_SIZE + 1).read_to_end(&mut bytes).await
        .map_err(|e| format!("读取远程文件失败: {e}"))?;
    if bytes.len() as u64 > EDIT_MAX_SIZE {
        return Err("文件过大，不支持在线编辑（上限 10 MB）".into());
    }
    let (raw_text, encoding, bom, lossy) = detect_and_decode(&bytes);
    let crlf = raw_text.contains("\r\n");
    let text = if crlf { raw_text.replace("\r\n", "\n").replace('\r', "\n") } else { raw_text };
    Ok(RemoteFileContent { text, encoding, bom, crlf, lossy, size })
}

/// 保存远程文本文件（按读取时识别的编码/换行/BOM 写回），返回写入字节数
#[tauri::command]
pub async fn sftp_write_file(
    sessions: tauri::State<'_, Sessions>,
    sid: u32,
    path: String,
    content: String,
    encoding: String,
    bom: bool,
    crlf: bool,
) -> Result<u64, String> {
    let Some(session) = sessions.0.get(sid) else { return Err("会话不存在".into()) };
    let bytes = encode_content(&content, &encoding, bom, crlf)?;
    if bytes.len() > EDIT_WRITE_MAX {
        return Err("内容过大，保存失败（上限 20 MB）".into());
    }
    let sftp = ssh::get_sftp(&session).await.map_err(|e| e.to_string())?;
    use russh_sftp::protocol::OpenFlags;
    use tokio::io::AsyncWriteExt;
    let mut remote = sftp
        .open_with_flags(&path, OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNCATE)
        .await
        .map_err(|e| ssh::sftp_error("保存远程文件", &path, e))?;
    remote.write_all(&bytes).await.map_err(|e| format!("写入远程文件失败: {e}"))?;
    let _ = remote.shutdown().await;
    Ok(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_plain_utf8() {
        let (text, enc, bom, lossy) = detect_and_decode("hello 世界".as_bytes());
        assert_eq!(text, "hello 世界");
        assert_eq!(enc, "UTF-8");
        assert!(!bom && !lossy);
    }

    #[test]
    fn detect_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("# 注释".as_bytes());
        let (text, enc, bom, _) = detect_and_decode(&bytes);
        assert_eq!(text, "# 注释");
        assert_eq!(enc, "UTF-8");
        assert!(bom);
    }

    #[test]
    fn detect_utf16le_bom() {
        // BOM + 无 BOM 两种形态都要能还原（UTF-16 字节手动打包）
        let text = "server:\n  port: 22\n";
        let mut le_bytes: Vec<u8> = Vec::new();
        for unit in text.encode_utf16() {
            le_bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let mut with_bom = vec![0xFF, 0xFE];
        with_bom.extend_from_slice(&le_bytes);
        for bytes in [&with_bom, &le_bytes] {
            let (decoded, enc, _, _) = detect_and_decode(bytes);
            assert_eq!(decoded, text);
            assert_eq!(enc, "UTF-16LE");
        }
    }

    #[test]
    fn detect_gbk_by_chardetng() {
        let (bytes, _, _) = encoding_rs::GBK.encode("listen = 0.0.0.0\n# 网络监听配置，端口绑定到所有网卡\nserver_name example\n");
        let (text, enc, _, _) = detect_and_decode(&bytes);
        assert_eq!(enc, "GBK");
        assert!(text.contains("网络监听配置"));
    }

    #[test]
    fn encode_roundtrip_with_bom_and_crlf() {
        let bytes = encode_content("a\nb\n", "UTF-8", true, true).unwrap();
        assert_eq!(bytes, b"\xEF\xBB\xBFa\r\nb\r\n");
        // detect_and_decode 返回原始解码文本；行尾归一化由 sftp_read_file 完成
        let (raw, enc, bom, _) = detect_and_decode(&bytes);
        assert_eq!(raw.replace("\r\n", "\n"), "a\nb\n");
        assert_eq!(enc, "UTF-8");
        assert!(bom);
        // UTF-16LE 写回：BOM + 手动打包的码元
        let bytes16 = encode_content("hi\n", "UTF-16LE", true, false).unwrap();
        assert_eq!(bytes16, vec![0xFF, 0xFE, 0x68, 0x00, 0x69, 0x00, 0x0A, 0x00]);
        let (decoded, enc, _, _) = detect_and_decode(&bytes16);
        assert_eq!(decoded, "hi\n");
        assert_eq!(enc, "UTF-16LE");
    }

    #[test]
    fn encode_gbk_rejects_unknown_encoding() {
        assert!(encode_content("x", "no-such-encoding", false, false).is_err());
    }
}
