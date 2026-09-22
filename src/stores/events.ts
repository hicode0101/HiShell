/** 全局事件接线：tauri 后端事件 → 各 store（仅 tauri 环境调用） */
import { isTauri } from '../api/adapter'
import {
  listenEvent,
  type SshDataEvent,
  type SshStatusEvent,
  type SshPromptEvent,
  type TransferProgressEvent,
  type TransferDoneEvent,
  type LocalDataEvent,
  type LocalStatusEvent,
  b64ToBytes,
} from '../api/backend'
import { useSessionsStore } from './sessions'
import { useProfilesStore } from './profiles'
import { useTransfersStore } from './transfers'
import { forwardAdd } from '../api/backend'
import { t } from '../i18n'

type DataHandler = (bytes: Uint8Array) => void

/** 按会话注册的终端数据订阅者 */
const dataHandlers = new Map<number, Set<DataHandler>>()
/** 会话数据早于终端接线到达时按 sid 缓冲（最多 64 块，防止无限增长） */
const pendingData = new Map<number, Uint8Array[]>()

export function onSshDataFor(sid: number, handler: DataHandler): () => void {
  let set = dataHandlers.get(sid)
  if (!set) {
    set = new Set()
    dataHandlers.set(sid, set)
  }
  // 回放接线前缓冲的数据（如 banner）
  const pending = pendingData.get(sid)
  if (pending) {
    pending.forEach((b) => handler(b))
    pendingData.delete(sid)
  }
  set.add(handler)
  return () => {
    set!.delete(handler)
  }
}

/** 本地终端（lid 空间与 sid 相互独立，单独建表） */
const localDataHandlers = new Map<number, Set<DataHandler>>()
const pendingLocalData = new Map<number, Uint8Array[]>()

export function onLocalDataFor(lid: number, handler: DataHandler): () => void {
  let set = localDataHandlers.get(lid)
  if (!set) {
    set = new Set()
    localDataHandlers.set(lid, set)
  }
  const pending = pendingLocalData.get(lid)
  if (pending) {
    pending.forEach((b) => handler(b))
    pendingLocalData.delete(lid)
  }
  set.add(handler)
  return () => {
    set!.delete(handler)
  }
}

let inited = false

export function initGlobalEvents() {
  if (!isTauri || inited) return
  inited = true
  const sessions = useSessionsStore()
  const transfers = useTransfersStore()

  listenEvent<SshStatusEvent>('ssh-status', (e) => {
    sessions.setStatusBySid(e.sid, e.status, e.message)
    // 会话连上后补投「连接期间保存」的端口转发规则：
    // 后端在 shell 就绪时会自动应用配置里的规则，但连接过程中保存的规则会错过那一波，
    // 这里按规则 id 幂等补投（后端对运行中的规则自动跳过），确保所有已保存规则都生效。
    if (e.status === 'connected') {
      const tab = sessions.tabs.find((t) => t.sid === e.sid)
      const profile = tab?.profileId ? useProfilesStore().byId(tab.profileId) : undefined
      profile?.forwardedPorts.forEach((rule) => {
        forwardAdd(e.sid, rule).catch(() => {})
      })
    }
  })

  listenEvent<SshDataEvent>('ssh-data', (e) => {
    const bytes = b64ToBytes(e.data)
    const handlers = dataHandlers.get(e.sid)
    if (handlers && handlers.size) {
      handlers.forEach((h) => h(bytes))
    } else {
      const arr = pendingData.get(e.sid) ?? []
      if (arr.length < 64) arr.push(bytes)
      pendingData.set(e.sid, arr)
    }
  })

  listenEvent<SshPromptEvent>('ssh-prompt', (e) => {
    sessions.pushPrompt(e.sid, e as unknown as Record<string, unknown>)
  })

  listenEvent<TransferProgressEvent>('transfer-progress', (e) => {
    transfers.applyProgress(`xfer-${e.tid}`, e.transferred, e.total)
  })

  listenEvent<TransferDoneEvent>('transfer-done', (e) => {
    transfers.applyDone(`xfer-${e.tid}`, e.ok, e.error)
  })

  listenEvent<LocalDataEvent>('local-data', (e) => {
    const bytes = b64ToBytes(e.data)
    const handlers = localDataHandlers.get(e.lid)
    if (handlers && handlers.size) {
      handlers.forEach((h) => h(bytes))
    } else {
      const arr = pendingLocalData.get(e.lid) ?? []
      if (arr.length < 64) arr.push(bytes)
      pendingLocalData.set(e.lid, arr)
    }
  })

  listenEvent<LocalStatusEvent>('local-status', (e) => {
    sessions.setStatusByLid(e.lid, 'disconnected', t('terminal.localExited'))
  })
}
