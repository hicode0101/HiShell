/** 全局事件接线：tauri 后端事件 → 各 store（仅 tauri 环境调用） */
import { isTauri } from '../api/adapter'
import {
  listenEvent,
  type SshDataEvent,
  type SshStatusEvent,
  type SshPromptEvent,
  type TransferProgressEvent,
  type TransferDoneEvent,
  b64ToBytes,
} from '../api/backend'
import { useSessionsStore } from './sessions'
import { useTransfersStore } from './transfers'

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

let inited = false

export function initGlobalEvents() {
  if (!isTauri || inited) return
  inited = true
  const sessions = useSessionsStore()
  const transfers = useTransfersStore()

  listenEvent<SshStatusEvent>('ssh-status', (e) => {
    sessions.setStatusBySid(e.sid, e.status, e.message)
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
}
