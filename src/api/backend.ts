/**
 * 后端 API 接口层：组件只调用这里，不感知真实/ mock 环境。
 * - Tauri 环境：invoke 真实 Rust command + 事件订阅
 * - 浏览器环境：mock 实现（本地文件树/模拟传输），便于 UI 预览
 */
import { isTauri, tauriInvoke } from './adapter'
import type { DirEntry, SshProfile, ForwardRule } from './types'

/* ==================== 事件订阅 ==================== */

export type UnlistenFn = () => void

export async function listenEvent<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri) return () => {}
  const { listen } = await import('@tauri-apps/api/event')
  const un = await listen<T>(event, (e) => handler(e.payload))
  return un
}

/** SSH 数据/状态/提示事件 */
export interface SshDataEvent {
  sid: number
  data: string
}
export interface SshStatusEvent {
  sid: number
  status: 'connecting' | 'connected' | 'disconnected'
  message?: string
}
export interface SshPromptEvent {
  sid: number
  kind: 'hostkeyNew' | 'hostkeyMismatch' | 'password' | 'keyboardInteractive'
  host?: string
  port?: number
  user?: string
  fingerprint?: string
  keyType?: string
  oldFingerprint?: string
  newFingerprint?: string
  name?: string
  instruction?: string
  prompts?: { text: string; echo: boolean }[]
}
export interface TransferProgressEvent {
  tid: number
  transferred: number
  total: number
}
export interface TransferDoneEvent {
  tid: number
  ok: boolean
  error?: string
}

/* ==================== 本地文件系统 ==================== */

export async function listLocalDir(path: string): Promise<DirEntry[]> {
  if (isTauri) return tauriInvoke<DirEntry[]>('fs_list_dir', { path })
  return mockListDir(path)
}

export async function specialDir(kind: string): Promise<string> {
  if (isTauri) return tauriInvoke<string>('fs_special_dir', { kind })
  return mockSpecialDirs[kind as keyof typeof mockSpecialDirs] ?? 'C:\\Users\\demo'
}

export async function appInfo(): Promise<{ name: string; version: string; mode: string }> {
  if (isTauri) return tauriInvoke('app_info')
  return { name: 'HiShell', version: '0.1.0', mode: '浏览器预览 (mock)' }
}

/* ==================== 连接配置 ==================== */

/** 保存时提交的明文凭据（undefined = 保持已存值） */
export interface ProfileSecretsIn {
  password?: string
  passphrase?: string
  proxyPassword?: string
  clear?: boolean
}

export async function profileList(): Promise<SshProfile[]> {
  if (isTauri) return tauriInvoke<SshProfile[]>('profile_list')
  return mockProfiles()
}

export async function profileSave(profile: SshProfile, secrets?: ProfileSecretsIn): Promise<SshProfile> {
  if (isTauri) return tauriInvoke<SshProfile>('profile_save', { profile, secrets: secrets ?? null })
  mockProfileStore.upsert(profile)
  return profile
}

export async function profileDelete(id: string): Promise<void> {
  if (isTauri) return tauriInvoke<void>('profile_delete', { id })
  mockProfileStore.remove(id)
}

/* ==================== SSH 会话 ==================== */

export async function sshConnect(profileId: string): Promise<number> {
  return tauriInvoke<number>('ssh_connect', { profileId })
}

export async function sshDisconnect(sid: number): Promise<void> {
  return tauriInvoke<void>('ssh_disconnect', { sid })
}

export async function sshWrite(sid: number, bytes: Uint8Array): Promise<void> {
  const b64 = bytesToB64(bytes)
  return tauriInvoke<void>('ssh_write', { sid, data: b64 })
}

export async function sshResize(sid: number, cols: number, rows: number): Promise<void> {
  return tauriInvoke<void>('ssh_resize', { sid, cols, rows })
}

export async function sshPromptResponse(
  sid: number,
  key: string,
  value: Record<string, unknown>,
): Promise<void> {
  return tauriInvoke<void>('ssh_prompt_response', { sid, answer: { key, value } })
}

export function bytesToB64(bytes: Uint8Array): string {
  let bin = ''
  const CHUNK = 0x8000
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return btoa(bin)
}

export function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

/* ==================== SFTP ==================== */

export async function sftpList(sid: number, path: string): Promise<DirEntry[]> {
  return tauriInvoke<DirEntry[]>('sftp_list', { sid, path })
}

export async function sftpMkdir(sid: number, path: string): Promise<void> {
  return tauriInvoke<void>('sftp_mkdir', { sid, path })
}

export async function sftpRm(sid: number, path: string): Promise<void> {
  return tauriInvoke<void>('sftp_rm', { sid, path })
}

export async function sftpRename(sid: number, path: string, newPath: string): Promise<void> {
  return tauriInvoke<void>('sftp_rename', { sid, path, newPath })
}

/* ==================== 传输 ==================== */

export interface EnqueueItem {
  sid: number
  direction: 'upload' | 'download'
  isDir: boolean
  size: number
  localPath: string
  remotePath: string
}

export interface Enqueued {
  tid: number
  localPath: string
  remotePath: string
  size: number
}

export async function transferEnqueue(item: EnqueueItem): Promise<Enqueued[]> {
  return tauriInvoke<Enqueued[]>('transfer_enqueue', { item })
}

export async function transferPause(tid: number): Promise<void> {
  return tauriInvoke<void>('transfer_pause', { tid })
}
export async function transferResume(tid: number): Promise<void> {
  return tauriInvoke<void>('transfer_resume', { tid })
}
export async function transferCancel(tid: number): Promise<void> {
  return tauriInvoke<void>('transfer_cancel', { tid })
}

/** tid（数字）与前端传输 id（xfer-N）互转 */
export function transferTid(frontendId: string): number {
  return Number(frontendId.replace('xfer-', ''))
}

/* ==================== 端口转发 ==================== */

export async function forwardAdd(sid: number, rule: ForwardRule): Promise<void> {
  return tauriInvoke<void>('forward_add', { sid, rule })
}

export async function forwardRemove(sid: number, ruleId: string): Promise<void> {
  return tauriInvoke<void>('forward_remove', { sid, ruleId })
}

export interface ForwardStatus {
  id: string
  type: 'local' | 'remote' | 'dynamic'
  bindHost: string
  bindPort: number
  targetHost?: string
  targetPort?: number
  active: boolean
  error?: string
}

export async function forwardList(sid: number): Promise<ForwardStatus[]> {
  return tauriInvoke<ForwardStatus[]>('forward_list', { sid })
}

/* ==================== Mock 实现（浏览器预览） ==================== */

import { mockProfiles, mockProfileStore } from './mock'

export {
  mockListDir,
  mockListRemoteDir,
  mockSpecialDirs,
  mockProfiles,
  mockProfileStore,
} from './mock'

import { mockListDir, mockListRemoteDir, mockSpecialDirs } from './mock'

/** 远程目录：tauri 且会话连接时走真实 SFTP，否则 mock */
export async function listRemoteDir(sid: number | undefined, path: string, connected: boolean): Promise<DirEntry[]> {
  if (isTauri && sid !== undefined && connected) {
    return sftpList(sid, path)
  }
  return mockListRemoteDir(path)
}
