/**
 * 终端复制/粘贴快捷键（全平台统一）：
 * Ctrl+Shift+C / Ctrl+Shift+V（终端惯例，Ctrl+C 保留给 ^C/SIGINT）。
 */
export const COPY_SHORTCUT = 'Ctrl+Shift+C'
export const PASTE_SHORTCUT = 'Ctrl+Shift+V'

/** 运行环境是否 macOS（拖放坐标等平台差异判定用） */
export const isMac = /macintosh|mac os x/i.test(navigator.userAgent)
