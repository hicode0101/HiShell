/** 会话标签状态：多标签、激活、视图模式、连接状态机（tauri 真实连接 / mock 模拟） */
import { defineStore } from 'pinia'
import { isTauri } from '../api/adapter'
import { localStart, localStop, sshConnect, sshDisconnect, sshDuplicateChannel } from '../api/backend'
import type { SessionTab, SshProfile, ViewMode } from '../api/types'
import { t } from '../i18n'

let nextId = 1

function makeTab(partial: Partial<SessionTab> & { title: string }): SessionTab {
  return {
    id: `tab-${nextId++}`,
    kind: 'session',
    address: '',
    status: 'disconnected',
    viewMode: 'terminal',
    cols: 0,
    rows: 0,
    ...partial,
  }
}

export const useSessionsStore = defineStore('sessions', {
  state: () => ({
    tabs: [] as SessionTab[],
    activeTabId: '' as string,
    connectDialogOpen: false,
    /** 对话框是否处于编辑已有 profile */
    connectDialogProfileId: '' as string,
    /** 端口转发管理弹窗（针对当前激活会话） */
    portsDialogOpen: false,
    /** 待处理的后端提示（hostkey / password / KI），队列式 */
    prompts: [] as { sid: number; key: string; payload: Record<string, unknown> }[],
    /** 事件早于 sid 回传到达时的状态缓冲 */
    pendingStatus: {} as Record<number, { status: SessionTab['status']; message?: string }>,
  }),
  getters: {
    activeTab: (st): SessionTab | undefined => st.tabs.find((t) => t.id === st.activeTabId),
    sessionCount: (st) => st.tabs.filter((t) => t.kind === 'session').length,
    connectedCount: (st) => st.tabs.filter((t) => t.status === 'connected').length,
    addressBarText(): string {
      const t = this.activeTab
      if (!t || t.kind !== 'session' || !t.address) return ''
      return t.address
    },
  },
  actions: {
    activate(id: string) {
      this.activeTabId = id
    },
    /** 拖拽排序：把 dragId 标签移动到 targetId 之前/之后 */
    moveTab(dragId: string, targetId: string, before: boolean) {
      if (dragId === targetId) return
      const from = this.tabs.findIndex((t) => t.id === dragId)
      const targetIdx = this.tabs.findIndex((t) => t.id === targetId)
      if (from < 0 || targetIdx < 0) return
      const [tab] = this.tabs.splice(from, 1)
      let insertIdx = this.tabs.findIndex((t) => t.id === targetId)
      if (insertIdx < 0) {
        this.tabs.splice(from, 0, tab) // 目标消失则原位放回
        return
      }
      if (!before) insertIdx += 1
      this.tabs.splice(insertIdx, 0, tab)
    },
    openConnectDialog(profileId?: string) {
      this.connectDialogProfileId = profileId ?? ''
      this.connectDialogOpen = true
    },
    openPortsDialog() {
      this.portsDialogOpen = true
    },
    openSettings() {
      const existing = this.tabs.find((t) => t.kind === 'settings')
      if (existing) {
        this.activeTabId = existing.id
        return
      }
      const tab = makeTab({ kind: 'settings', title: t('tabs.settings') })
      this.tabs.push(tab)
      this.activeTabId = tab.id
    },
    /** 从 profile 建立会话标签：tauri 走 ssh_connect（事件驱动状态），浏览器 mock 延迟置连 */
    createSession(profile: SshProfile): SessionTab {
      const raw = makeTab({
        title: profile.name || profile.host,
        address: `ssh://${profile.username}@${profile.host}:${profile.port}`,
        profileId: profile.id,
        status: 'connecting',
      })
      this.tabs.push(raw)
      // 后续修改必须走响应式代理对象（直接改 raw 不会触发任何 watch/渲染）
      const tab = this.tabs[this.tabs.length - 1] as SessionTab
      this.activeTabId = tab.id
      if (isTauri) {
        sshConnect(profile.id)
          .then((sid) => {
            tab.sid = sid
            // 应用早于 sid 回传到达的状态事件（避免竞态丢失）
            const pending = this.pendingStatus[sid]
            if (pending) {
              tab.status = pending.status
              if (pending.message) tab.statusMessage = pending.message
              delete this.pendingStatus[sid]
            }
          })
          .catch((e) => {
            tab.status = 'disconnected'
            tab.statusMessage = String(e)
          })
      } else {
        // 模拟连接延迟
        setTimeout(() => {
          if (tab.status === 'connecting') tab.status = 'connected'
        }, 900)
      }
      return tab
    },
    /**
     * 复制 SSH 隧道：新建一个标签，复用源标签已建立的 SSH 连接
     * （后端在同一连接上开新 shell 通道，不重新握手认证）。
     * 源连接断开时所有别名一起掉线；关闭别名不影响源连接。
     */
    async createAliasSession(sourceTabId: string): Promise<SessionTab | undefined> {
      const src = this.tabs.find((x) => x.id === sourceTabId)
      if (!src || src.sid === undefined || !isTauri) return undefined
      const raw = makeTab({
        title: src.title,
        address: src.address,
        profileId: src.profileId,
        status: 'connecting',
      })
      this.tabs.push(raw)
      const tab = this.tabs[this.tabs.length - 1] as SessionTab
      this.activeTabId = tab.id
      try {
        const newSid = await sshDuplicateChannel(src.sid)
        tab.sid = newSid
        // 状态事件可能早于 sid 回传（缓冲机制同 createSession）
        const pending = this.pendingStatus[newSid]
        if (pending) {
          tab.status = pending.status
          if (pending.message) tab.statusMessage = pending.message
          delete this.pendingStatus[newSid]
        }
      } catch (e) {
        tab.status = 'disconnected'
        tab.statusMessage = String(e)
      }
      return tab
    },
    /** 重连：复用标签，重置后端会话 */
    async reconnect(tabId: string) {      const tab = this.tabs.find((x) => x.id === tabId)
      if (!tab || tab.kind !== 'session') return
      if (tab.sid !== undefined && isTauri) {
        await sshDisconnect(tab.sid).catch(() => {})
        tab.sid = undefined
      }
      tab.status = 'connecting'
      if (isTauri && tab.profileId) {
        try {
          const sid = await sshConnect(tab.profileId)
          tab.sid = sid
        } catch (e) {
          tab.status = 'disconnected'
          console.warn('ssh_connect 失败', e)
        }
      } else {
        setTimeout(() => {
          if (tab.status === 'connecting') tab.status = 'connected'
        }, 900)
      }
    },
    setStatusById(id: string, status: SessionTab['status']) {
      const tab = this.tabs.find((x) => x.id === id)
      if (tab) tab.status = status
    },
    /** 主动断开（菜单/功能区） */
    async disconnectById(id: string) {
      const tab = this.tabs.find((x) => x.id === id)
      if (!tab) return
      if (tab.sid !== undefined && isTauri) {
        await sshDisconnect(tab.sid).catch(() => {})
      }
      tab.sid = undefined // 后端会话已销毁，旧 sid 置空
      tab.status = 'disconnected'
    },
    /** 由 ssh-status 事件驱动 */
    setStatusBySid(sid: number, status: SessionTab['status'], message?: string) {
      const tab = this.tabs.find((x) => x.sid === sid)
      if (tab) {
        tab.status = status
        if (message) tab.statusMessage = message
      } else {
        // 事件先于 sid 回传到达：缓冲，待 createSession 回填
        this.pendingStatus[sid] = { status, message }
      }
    },
    /** 由 local-status 事件驱动（本地终端进程退出） */
    setStatusByLid(lid: number, status: SessionTab['status'], message?: string) {
      const tab = this.tabs.find((x) => x.lid === lid)
      if (tab) {
        tab.status = status
        if (message) tab.statusMessage = message
      }
    },
    /** 新建本地终端标签：tauri 走 local_start（PTY），浏览器 mock 模拟 shell */
    createLocalTab(): SessionTab {
      const raw = makeTab({
        kind: 'local',
        title: t('tabs.localTerminal'),
        address: 'local',
        status: 'connected',
      })
      this.tabs.push(raw)
      const tab = this.tabs[this.tabs.length - 1] as SessionTab
      this.activeTabId = tab.id
      if (isTauri) {
        localStart()
          .then((lid) => {
            tab.lid = lid
          })
          .catch((e) => {
            tab.status = 'disconnected'
            tab.statusMessage = String(e)
          })
      }
      return tab
    },
    /** 推入后端提示（key 由后端下发，应答时原样带回） */
    pushPrompt(sid: number, payload: Record<string, unknown>): string {
      const key = (payload.key as string) ?? `prompt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
      this.prompts.push({ sid, key, payload })
      return key
    },
    answerPrompt(key: string): { sid: number; key: string } | undefined {
      const idx = this.prompts.findIndex((p) => p.key === key)
      if (idx >= 0) {
        const p = this.prompts[idx]
        this.prompts.splice(idx, 1)
        return { sid: p.sid, key: p.key }
      }
      return undefined
    },
    setViewMode(id: string, mode: ViewMode) {
      const t = this.tabs.find((x) => x.id === id)
      if (t) t.viewMode = mode
    },
    renameTab(id: string, title: string) {
      const t = this.tabs.find((x) => x.id === id)
      if (t && title.trim()) t.title = title.trim()
    },
    closeTab(id: string) {
      const idx = this.tabs.findIndex((t) => t.id === id)
      if (idx < 0) return
      const tab = this.tabs[idx]
      // 断开后端会话（fire-and-forget）
      if (tab.sid !== undefined && isTauri) {
        sshDisconnect(tab.sid).catch(() => {})
      }
      if (tab.lid !== undefined && isTauri) {
        localStop(tab.lid).catch(() => {})
      }
      const wasActive = this.activeTabId === id
      this.tabs.splice(idx, 1)
      if (wasActive) {
        const next = this.tabs[Math.min(idx, this.tabs.length - 1)]
        this.activeTabId = next?.id ?? ''
      }
    },
    closeOthers(id: string) {
      this.tabs = this.tabs.filter((t) => t.id === id || t.kind === 'settings')
      this.activeTabId = id
    },
    closeToRight(id: string) {
      const idx = this.tabs.findIndex((t) => t.id === id)
      if (idx >= 0) this.tabs = this.tabs.slice(0, idx + 1)
      if (!this.tabs.find((t) => t.id === this.activeTabId)) {
        this.activeTabId = this.tabs[this.tabs.length - 1]?.id ?? ''
      }
    },
    cycleTab(dir: 1 | -1) {
      if (this.tabs.length === 0) return
      const idx = this.tabs.findIndex((t) => t.id === this.activeTabId)
      const next = this.tabs[(idx + dir + this.tabs.length) % this.tabs.length]
      this.activeTabId = next.id
    },
  },
})
