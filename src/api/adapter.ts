/**
 * 后端适配器：同一套 backend 接口在两种环境下工作
 * - Tauri 环境（npm run tauri dev / 打包应用）：invoke 真实 Rust command
 * - 浏览器环境（npm run dev）：mock 实现，便于 UI 预览与开发
 */

export const isTauri: boolean =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** Windows 判定（Mica 等平台特效仅 Windows 启用） */
export const isWindows: boolean =
  typeof navigator !== 'undefined' && /Win/i.test(navigator.userAgent)

export async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

/** 窗口控制；浏览器环境返回 null（按钮降级为 no-op） */
export async function tauriWindow(): Promise<{
  minimize(): void
  toggleMaximize(): void
  close(): void
} | null> {
  if (!isTauri) return null
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  return getCurrentWindow()
}

export function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms))
}
