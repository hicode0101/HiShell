/**
 * 本地（本机文件系统）路径工具。
 * 远程 SFTP 路径恒用 /，但本地分隔符随平台不同（Windows 为 \），
 * 拼接本地路径一律走这里，禁止手写分隔符。
 */

/** 运行环境是否 Windows（Tauri WebView 的 UA 含平台信息） */
export const isWinPlatform = /windows/i.test(navigator.userAgent)

/** 本地路径分隔符 */
export const LOCAL_SEP = isWinPlatform ? '\\' : '/'

/** 去掉结尾的分隔符后按平台分隔符拼接；base 为空时直接返回 name */
export function joinLocalPath(base: string, name: string): string {
  if (!base) return name
  return base.replace(/[\\/]+$/, '') + LOCAL_SEP + name
}
