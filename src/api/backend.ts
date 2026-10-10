/**
 * 后端 API 接口层：组件只调用这里，不感知真实/ mock 环境。
 * - Tauri 环境：invoke 真实 Rust command + 事件订阅
 * - 浏览器环境：mock 实现（本地文件树/模拟传输），便于 UI 预览
 */
import { isTauri, tauriInvoke } from './adapter'
import type { DirEntry, LocalTermSession, RemoteFileContent, SshProfile, ForwardRule } from './types'
import pkg from '../../package.json'

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
/** 本地终端数据/状态事件 */
export interface LocalDataEvent {
  lid: number
  data: string
}
export interface LocalStatusEvent {
  lid: number
  status: 'exited'
}
/** 会话导入候选项（Xshell / Tabby） */
export interface ImportCandidate {
  source: 'xshell' | 'tabby'
  name: string
  host: string
  port: number
  username: string
  auth: 'password' | 'priFileKey' | 'keyboardInteractive'
  privateKeyPath: string
  originPath: string
}
export interface ImportApplyResult {
  imported: number
  skipped: number
}

/* ==================== 本地文件系统 ==================== */

export async function listLocalDir(path: string): Promise<DirEntry[]> {
  if (isTauri) return tauriInvoke<DirEntry[]>('fs_list_dir', { path })
  return mockListDir(path)
}

/* 本地文件操作（右键菜单：重命名/删除/新建文件夹/属性/打开） */
export async function fsRename(oldPath: string, newPath: string): Promise<void> {
  return tauriInvoke<void>('fs_rename', { oldPath, newPath })
}
export async function fsDelete(path: string): Promise<void> {
  return tauriInvoke<void>('fs_delete', { path })
}
export async function fsMkdir(path: string): Promise<void> {
  return tauriInvoke<void>('fs_mkdir', { path })
}
export interface FileStat {
  name: string
  kind: 'dir' | 'file' | 'symlink'
  size: number
  mtime: number
  attrs: string
  readonly: boolean
}
export async function fsStat(path: string): Promise<FileStat> {
  return tauriInvoke<FileStat>('fs_stat', { path })
}
/** 用系统默认程序打开本地文件/文件夹 */
export async function openPath(path: string): Promise<void> {
  return tauriInvoke<void>('open_path', { path })
}

export async function specialDir(kind: string): Promise<string> {
  if (isTauri) return tauriInvoke<string>('fs_special_dir', { kind })
  return mockSpecialDirs[kind as keyof typeof mockSpecialDirs] ?? 'C:\\Users\\demo'
}

export async function appInfo(): Promise<{ name: string; version: string; mode: string }> {
  if (isTauri) return tauriInvoke('app_info')
  return { name: 'HiShell', version: pkg.version, mode: '浏览器预览 (mock)' }
}

/** 在系统文件管理器中打开配置目录 */
export async function openConfigDir(): Promise<void> {
  if (isTauri) return tauriInvoke<void>('open_config_dir')
  toastMockOnly()
}

/** 用系统默认浏览器打开 http/https 链接 */
export async function openUrl(url: string): Promise<void> {
  if (isTauri) return tauriInvoke<void>('open_url', { url })
  toastMockOnly()
}

function toastMockOnly() {
  /* 浏览器预览模式无后端，忽略 */
}

/* ==================== 连接配置 ==================== */

/** 保存时提交的明文凭据（undefined = 保持已存值） */
export interface ProfileSecretsIn {
  password?: string
  passphrase?: string
  proxyPassword?: string
  clear?: boolean
  /** 仅清除已存密码 */
  clearPassword?: boolean
  /** 仅清除已存私钥口令 */
  clearPassphrase?: boolean
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

/** 按给定 id 顺序重排连接配置 */
export async function profileReorder(ids: string[]): Promise<void> {
  if (isTauri) return tauriInvoke<void>('profile_reorder', { ids })
}

export async function profileDelete(id: string): Promise<void> {
  if (isTauri) return tauriInvoke<void>('profile_delete', { id })
  mockProfileStore.remove(id)
}

/** 复制连接配置：新 id + 指定名称（重名自动递增），凭据一并复制 */
export async function profileCopy(sourceId: string, newName: string): Promise<SshProfile> {
  return tauriInvoke<SshProfile>('profile_copy', { sourceId, newName })
}

/* ==================== 会话配置导出 / 导入 ==================== */

export interface ProfileExportResult {
  exported: number
  path: string
}

export interface ProfileImportResult {
  imported: number
  skipped: number
}

/** 导出全部连接配置为 JSON 文件（密码等凭据不随文件迁移） */
export async function profileExport(path: string): Promise<ProfileExportResult> {
  if (isTauri) return tauriInvoke<ProfileExportResult>('profile_export', { path })
  return { exported: 0, path }
}

/** 从 JSON 文件导入连接配置（按 id 与 host+port+username 去重） */
export async function profileImport(path: string): Promise<ProfileImportResult> {
  if (isTauri) return tauriInvoke<ProfileImportResult>('profile_import', { path })
  return { imported: 0, skipped: 0 }
}

/* ==================== SSH 会话 ==================== */

export async function sshConnect(profileId: string): Promise<number> {
  return tauriInvoke<number>('ssh_connect', { profileId })
}

export async function sshDisconnect(sid: number): Promise<void> {
  return tauriInvoke<void>('ssh_disconnect', { sid })
}

/** 复制 SSH 隧道：在已有连接上开新 shell 通道，返回新 sid（不重新握手认证） */
export async function sshDuplicateChannel(sid: number): Promise<number> {
  return tauriInvoke<number>('ssh_duplicate_channel', { sid })
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

export interface TestResult {
  ok: boolean
  authenticated: boolean
  message: string
}

/** 测试连接：真实握手 + 认证尝试（不弹窗、不保存主机密钥变更） */
export async function sshTest(
  profile: SshProfile,
  credentials?: { password?: string; passphrase?: string },
): Promise<TestResult> {
  return tauriInvoke<TestResult>('ssh_test', {
    profile,
    password: credentials?.password ?? null,
    passphrase: credentials?.passphrase ?? null,
  })
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

export async function sftpMkdirs(sid: number, paths: string[]): Promise<void> {
  return tauriInvoke<void>('sftp_mkdirs', { sid, paths })
}

export async function sftpRm(sid: number, path: string): Promise<void> {
  return tauriInvoke<void>('sftp_rm', { sid, path })
}

export async function sftpRename(sid: number, path: string, newPath: string): Promise<void> {
  return tauriInvoke<void>('sftp_rename', { sid, path, newPath })
}

/** 更改远程权限（八进制 mode 如 0o644；目录可递归）。返回实际修改的条目数 */
export async function sftpChmod(sid: number, path: string, mode: number, recursive: boolean): Promise<number> {
  return tauriInvoke<number>('sftp_chmod', { sid, path, mode, recursive })
}

export interface RemoteStat {
  name: string
  kind: 'dir' | 'file' | 'symlink'
  size: number
  mtime: number
  attrs?: string
}

export async function sftpStat(sid: number, path: string): Promise<RemoteStat | null> {
  return tauriInvoke<RemoteStat | null>('sftp_stat', { sid, path })
}

/* ---------- 远程文本文件在线编辑 ---------- */

export async function readRemoteFile(sid: number | undefined, path: string): Promise<RemoteFileContent> {
  if (isTauri) return tauriInvoke<RemoteFileContent>('sftp_read_file', { sid, path })
  return mockReadRemoteFile(path)
}

/** 保存远程文本文件（按读取时检测到的编码/换行/BOM 写回），返回写入字节数 */
export async function writeRemoteFile(
  sid: number | undefined,
  path: string,
  content: string,
  encoding: string,
  bom: boolean,
  crlf: boolean,
): Promise<number> {
  if (isTauri) return tauriInvoke<number>('sftp_write_file', { sid, path, content, encoding, bom, crlf })
  return content.length
}

/* ==================== 传输 ==================== */

export interface EnqueueItem {
  sid: number
  direction: 'upload' | 'download'
  isDir: boolean
  size: number
  localPath: string
  remotePath: string
  /** 断点续传：从目标端已有偏移继续 */
  resume?: boolean
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
  // 后端 ForwardRequest 结构：{ sid, rule }
  return tauriInvoke<void>('forward_add', { req: { sid, rule } })
}

export async function forwardRemove(sid: number, ruleId: string): Promise<void> {
  // 后端 ForwardRemoveRequest 结构：{ sid, ruleId }
  return tauriInvoke<void>('forward_remove', { req: { sid, ruleId } })
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

/* ==================== 会话导入（Xshell / Tabby） ==================== */

export async function importScan(): Promise<ImportCandidate[]> {
  if (isTauri) return tauriInvoke<ImportCandidate[]>('import_scan')
  return []
}

export async function importApply(candidates: ImportCandidate[]): Promise<ImportApplyResult> {
  if (isTauri) return tauriInvoke<ImportApplyResult>('import_apply', { candidates })
  return { imported: 0, skipped: 0 }
}

/* ==================== 用户密钥管理 ==================== */

export interface UserKeyInfo {
  name: string
  keyType: string
  bits: number
  comment: string
  fingerprint: string
  publicPath: string
  privatePath?: string
}

export async function userKeyList(): Promise<UserKeyInfo[]> {
  if (isTauri) return tauriInvoke<UserKeyInfo[]>('user_key_list')
  return []
}

export async function userKeyDelete(publicPath: string, privatePath?: string): Promise<void> {
  return tauriInvoke<void>('user_key_delete', { publicPath, privatePath: privatePath ?? null })
}

export interface GeneratedKeyInfo {
  genId: string
  keyType: string
  bits: number
  /** OpenSSH 单行公钥（type blob） */
  publicBlob: string
}

export interface GeneratedKeyPaths {
  name: string
  privatePath: string
  publicKeyPath: string
}

/** 生成密钥对（RSA/ECDSA/ED25519），genId 由前端生成便于取消 */
export async function userKeyGenerateStart(genId: string, keyType: string, bits: number): Promise<GeneratedKeyInfo> {
  return tauriInvoke<GeneratedKeyInfo>('user_key_generate_start', { genId, keyType, bits })
}

/** 把生成好的密钥按名称/注释/口令写入 ~/.ssh */
export async function userKeyGenerateFinish(req: {
  genId: string
  name: string
  comment: string
  passphrase: string
}): Promise<GeneratedKeyPaths> {
  return tauriInvoke<GeneratedKeyPaths>('user_key_generate_finish', { req })
}

/** 取消生成（生成中或已暂存均可调用） */
export async function userKeyGenerateCancel(genId: string): Promise<void> {
  try {
    await tauriInvoke<void>('user_key_generate_cancel', { genId })
  } catch {
    /* 取消失败可忽略 */
  }
}

/** 重命名用户密钥（私钥/公钥文件一并改名），返回刷新后的信息 */
export async function userKeyRename(
  publicPath: string,
  privatePath: string | undefined,
  newName: string,
): Promise<UserKeyInfo> {
  return tauriInvoke<UserKeyInfo>('user_key_rename', { publicPath, privatePath: privatePath ?? null, newName })
}

/** 把公钥文本保存为文件（下载目录，自动重名递增） */
export async function userKeySavePublic(name: string, content: string): Promise<string> {
  return tauriInvoke<string>('user_key_save_public', { name, content })
}

/* ==================== 密钥导入 / 导出 ==================== */

export interface KeyInspect {
  encrypted: boolean
  keyType?: string | null
  bits?: number | null
  comment?: string
}

/** 导入前探测私钥文件：是否加密、类型、位数 */
export async function userKeyInspect(sourcePath: string): Promise<KeyInspect> {
  return tauriInvoke<KeyInspect>('user_key_inspect', { sourcePath })
}

/** 导入私钥到 ~/.ssh（口令用于解密源文件，也用于加密落盘副本） */
export async function userKeyImport(req: {
  sourcePath: string
  name: string
  passphrase: string
}): Promise<UserKeyInfo> {
  return tauriInvoke<UserKeyInfo>('user_key_import', { req })
}

/** 读取公钥文件内容（OpenSSH 单行） */
export async function userKeyPublicContent(publicPath: string): Promise<string> {
  return tauriInvoke<string>('user_key_public_content', { publicPath })
}

/** 导出私钥到指定路径（校验当前口令，format: openssh/pem/ppk） */
export async function userKeyExportPrivateTo(req: {
  privatePath: string
  currentPassphrase: string
  newPassphrase: string
  format: 'openssh' | 'pem' | 'ppk'
  destPath: string
}): Promise<void> {
  return tauriInvoke<void>('user_key_export_private_to', { req })
}

/** 把文本写到指定路径（公钥导出用） */
export async function userKeyWriteText(path: string, content: string): Promise<void> {
  return tauriInvoke<void>('user_key_write_text', { path, content })
}

/* ==================== 本地终端（PTY） ==================== */

export async function localStart(cwd?: string): Promise<number> {
  return tauriInvoke<number>('local_start', { cwd: cwd ?? null })
}

export async function localWrite(lid: number, bytes: Uint8Array): Promise<void> {
  return tauriInvoke<void>('local_write', { lid, data: bytesToB64(bytes) })
}

export async function localResize(lid: number, cols: number, rows: number): Promise<void> {
  return tauriInvoke<void>('local_resize', { lid, cols, rows })
}

export async function localStop(lid: number): Promise<void> {
  return tauriInvoke<void>('local_stop', { lid })
}

/* ==================== 本地终端会话配置 ==================== */

export async function localSessionList(): Promise<LocalTermSession[]> {
  return tauriInvoke<LocalTermSession[]>('local_session_list')
}

export async function localSessionSave(items: LocalTermSession[]): Promise<void> {
  return tauriInvoke<void>('local_session_save', { items })
}

/* ==================== MCP Server ==================== */

/** MCP 服务状态（状态栏指示灯；mcp-status 事件与配置接口共用） */
export interface McpStatusInfo {
  enabled: boolean
  listenHost: string
  port: number
  /** 服务当前是否运行中 */
  running: boolean
  error?: string | null
}

export interface McpConfigInfo extends McpStatusInfo {
  /** 访问令牌（客户端配置 JSON 用） */
  token: string
}

/** MCP 服务启停状态推送事件（后端启动/停止逻辑发出） */
export interface McpStatusEvent extends McpStatusInfo {}

/** MCP 工具调用实时活动事件（start/done 两阶段，按 callId 配对） */
export interface McpActivityEvent {
  callId: number
  phase: 'start' | 'done'
  tool: string
  args?: Record<string, unknown>
  ok?: boolean
  error?: string
  /** 耗时毫秒（done 阶段） */
  ms?: number
}

export async function mcpConfigGet(): Promise<McpConfigInfo> {
  return tauriInvoke<McpConfigInfo>('mcp_config_get')
}

/** 保存并立即生效：enabled=true 启动（或按新地址重启），false 停止 */
export async function mcpConfigSave(enabled: boolean, listenHost: string, port: number): Promise<McpConfigInfo> {
  return tauriInvoke<McpConfigInfo>('mcp_config_save', { enabled, listenHost, port })
}

/** 重新生成访问令牌（旧令牌立即失效，运行中自动重启） */
export async function mcpTokenRegen(): Promise<McpConfigInfo> {
  return tauriInvoke<McpConfigInfo>('mcp_token_regen')
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

import { mockListDir, mockListRemoteDir, mockReadRemoteFile, mockSpecialDirs } from './mock'

/** 远程目录：tauri 且会话连接时走真实 SFTP，否则 mock */
export async function listRemoteDir(sid: number | undefined, path: string, connected: boolean): Promise<DirEntry[]> {
  if (isTauri && sid !== undefined && connected) {
    return sftpList(sid, path)
  }
  return mockListRemoteDir(path)
}
