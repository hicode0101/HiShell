/** AI 通过 MCP 执行操作的实时动态（mcp-activity 事件流，仅 UI 展示用） */
import { defineStore } from 'pinia'
import type { McpActivityEvent } from '../api/backend'

export interface McpActivityEntry {
  callId: number
  ts: number
  tool: string
  args?: Record<string, unknown>
  /** done 阶段的结果；start 阶段为 undefined（执行中） */
  ok?: boolean
  error?: string
  ms?: number
}

const MAX_ENTRIES = 100

let seq = 0

export const useMcpActivityStore = defineStore('mcpActivity', {
  state: () => ({
    entries: [] as McpActivityEntry[],
    /** 面板折叠态（会话内记忆） */
    collapsed: false,
  }),
  actions: {
    handle(e: McpActivityEvent) {
      if (e.phase === 'start') {
        this.entries.unshift({ callId: e.callId, ts: Date.now(), tool: e.tool, args: e.args })
      } else {
        // done：找到对应 start 条目原位更新（找不到则直接追加）
        const idx = this.entries.findIndex((x) => x.callId === e.callId)
        const patch = { ok: !!e.ok, error: e.error, ms: e.ms }
        if (idx >= 0) {
          Object.assign(this.entries[idx], patch)
        } else {
          this.entries.unshift({ callId: e.callId, ts: Date.now(), tool: e.tool, args: e.args, ...patch })
        }
      }
      if (this.entries.length > MAX_ENTRIES) this.entries.length = MAX_ENTRIES
    },
    clear() {
      this.entries = []
    },
  },
})
