/** HiShell 前后端共享数据模型（与 docs/hishell-设计方案.md §5 对齐） */

/* ---------- 文件系统 ---------- */

export type FileKind = 'dir' | 'file' | 'symlink'

export interface DirEntry {
  name: string
  kind: FileKind
  size: number
  /** 毫秒时间戳 */
  mtime: number
  /** 远程文件权限串（如 drwxr-xr-x），本地为空 */
  attrs?: string
}

/** 远程文本文件内容（后端已按检测到的编码解码为 UTF-8，行尾统一 \n） */
export interface RemoteFileContent {
  text: string
  /** 检测到的编码名（如 UTF-8 / GBK / Shift_JIS），保存时按此写回 */
  encoding: string
  /** 原文件是否带 BOM */
  bom: boolean
  /** 原文件行尾是否为 CRLF（保存时还原） */
  crlf: boolean
  /** 解码时是否有无法映射的字节（保存将丢失原字节） */
  lossy: boolean
  /** 原始字节数 */
  size: number
}

export type SpecialDirKind =
  | 'home'
  | 'desktop'
  | 'documents'
  | 'downloads'
  | 'pictures'
  | 'music'
  | 'videos'

/* ---------- 连接配置 ---------- */

export type AuthKind = 'password' | 'priFileKey' | 'userKey' | 'keyboardInteractive' | 'agent' | 'auto'

export type ProxyConfig =
  | { kind: 'none' }
  | { kind: 'socks5'; host: string; port: number; username?: string; password?: string }
  | { kind: 'http'; host: string; port: number; username?: string; password?: string }
  | { kind: 'jump'; profileId: string }
  | { kind: 'proxyCommand'; command: string }

export interface SshProfile {
  id: string
  name: string
  host: string
  port: number
  username: string
  auth: AuthKind
  password: string
  /** 后端已存凭据标记（keyring） */
  hasStoredPassword?: boolean
  hasStoredPassphrase?: boolean
  privateKeyPath: string
  privateKeyPassphrase: string
  /** 用户密钥管理中的密钥名（auth = 'userKey' 时生效） */
  userKeyName: string
  /** 快速连接的临时配置：真实入库（连接需要）但对所有列表隐藏，不算用户会话 */
  temporary?: boolean
  proxy: ProxyConfig
  keepaliveInterval: number
  keepaliveCountMax: number
  connectTimeout: number
  term: string
  forwardedPorts: ForwardRule[]
  behaviorOnSessionEnd: 'reconnect' | 'keep' | 'close'
}

export interface ForwardRule {
  id: string
  type: 'local' | 'remote' | 'dynamic'
  bindHost: string
  bindPort: number
  targetHost?: string
  targetPort?: number
  description?: string
}

export function defaultProfile(partial: Partial<SshProfile> = {}): SshProfile {
  return {
    id: `profile-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name: '新建连接',
    host: '',
    port: 22,
    username: 'root',
    auth: 'password',
    password: '',
    privateKeyPath: '',
    privateKeyPassphrase: '',
    userKeyName: '',
    proxy: { kind: 'none' },
    keepaliveInterval: 5000,
    keepaliveCountMax: 10,
    connectTimeout: 10000,
    term: 'xterm-256color',
    forwardedPorts: [],
    behaviorOnSessionEnd: 'keep',
    ...partial,
  }
}

/* ---------- 会话 ---------- */

export type SessionStatus = 'connecting' | 'connected' | 'disconnected'
export type ViewMode = 'terminal' | 'sftp'

export interface SessionTab {
  id: string
  /** session=SSH 会话 / local=本地终端 / settings=设置页 */
  kind: 'session' | 'local' | 'settings'
  title: string
  address: string
  profileId?: string
  /** 后端会话 id（tauri 环境，SSH 会话） */
  sid?: number
  /** 后端本地终端 id（tauri 环境，本地终端标签） */
  lid?: number
  status: SessionStatus
  /** 断开/失败原因（后端推送） */
  statusMessage?: string
  viewMode: ViewMode
  cols: number
  rows: number
}

/* ---------- 传输队列 ---------- */

export type TransferStatus = 'queued' | 'running' | 'paused' | 'done' | 'cancelled' | 'failed'
export type TransferDirection = 'upload' | 'download'

export interface TransferItem {
  id: string
  name: string
  direction: TransferDirection
  localPath: string
  remotePath: string
  total: number
  transferred: number
  /** bytes/s */
  speed: number
  startedAt: number
  elapsedMs: number
  status: TransferStatus
  /** 所属会话（断点续传重试时需要重新入队） */
  sid?: number
}

/* ---------- 快捷连接 ---------- */

export interface QuickConnectInfo {
  username: string
  host: string
  port: number
}
