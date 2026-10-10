/** MCP Server 服务状态：状态栏指示灯与设置页共享（设置页启停/改配置后 sync 保持一致） */
import { defineStore } from 'pinia'
import { isTauri } from '../api/adapter'
import { mcpConfigGet, type McpStatusInfo } from '../api/backend'

export const useMcpServerStore = defineStore('mcpServer', {
  state: () => ({
    enabled: false,
    /** 服务当前是否运行中（enabled 但未运行 = 启动失败等异常态） */
    running: false,
    listenHost: '127.0.0.1',
    port: 9500,
    error: '',
  }),
  actions: {
    sync(c: McpStatusInfo) {
      this.enabled = c.enabled
      this.running = c.running
      this.listenHost = c.listenHost
      this.port = c.port
      this.error = c.error ?? ''
    },
    /** 应用启动时拉取一次真实状态（浏览器原型模式 / 后端未就绪保持关闭态） */
    async load() {
      if (!isTauri) return
      try {
        this.sync(await mcpConfigGet())
      } catch {
        /* ignore */
      }
    },
  },
})
