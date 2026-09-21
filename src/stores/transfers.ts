/** 传输队列：tauri 环境由后端事件驱动进度；浏览器环境用模拟定时器 */
import { defineStore } from 'pinia'
import { isTauri } from '../api/adapter'
import { transferCancel, transferEnqueue, transferPause, transferResume, type EnqueueItem } from '../api/backend'
import type { TransferDirection, TransferItem, TransferStatus } from '../api/types'
import { formatBytes, formatDuration } from '../utils/format'
import { t } from '../i18n'

function transferTid(frontendId: string): number {
  return Number(frontendId.replace('xfer-', ''))
}

let nextId = 1

interface StoredTransfer extends TransferItem {
  timer?: number
}

export const useTransfersStore = defineStore('transfers', {
  state: () => ({
    items: [] as StoredTransfer[],
    logs: [] as { time: string; text: string; kind: 'info' | 'error' | 'success' }[],
    expanded: true,
    activeTab: 'transfer' as 'transfer' | 'log',
  }),
  getters: {
    runningCount: (st) => st.items.filter((i) => i.status === 'running' || i.status === 'queued').length,
    totalCount: (st) => st.items.length,
    hasActive(): boolean {
      return this.runningCount > 0
    },
  },
  actions: {
    log(text: string, kind: 'info' | 'error' | 'success' = 'info') {
      const time = new Date().toLocaleTimeString('zh-CN', { hour12: false })
      this.logs.unshift({ time, text, kind })
      if (this.logs.length > 500) this.logs.pop()
    },
    /** tauri 真实传输：调后端入队，展开结果建条目，进度由事件驱动 */
    async enqueueReal(sid: number, item: EnqueueItem, name: string) {
      try {
        const results = await transferEnqueue(item)
        for (const r of results) {
          const it: StoredTransfer = {
            id: `xfer-${r.tid}`,
            name,
            direction: item.direction,
            localPath: r.localPath,
            remotePath: r.remotePath,
            total: Math.max(r.size, 1),
            transferred: 0,
            speed: 0,
            startedAt: Date.now(),
            elapsedMs: 0,
            status: 'running',
            sid,
          }
          this.items.unshift(it)
        }
        this.log(
          item.direction === 'upload'
            ? t('transfer.log.enqueuedUp', { name, size: formatBytes(item.size) })
            : t('transfer.log.enqueuedDown', { name, size: formatBytes(item.size) }),
        )
        this.expanded = true
      } catch (e) {
        this.log(`${name}: ${String(e)}`, 'error')
      }
    },

    /** 后端进度事件 */
    applyProgress(frontendId: string, transferred: number, total: number) {
      const it = this.items.find((i) => i.id === frontendId)
      if (!it) return
      const now = Date.now()
      const dt = (now - it.startedAt - it.elapsedMs) / 1000
      if (dt > 0.05) it.speed = Math.max(0, (transferred - it.transferred) / dt)
      it.transferred = transferred
      it.total = Math.max(total, it.transferred)
      it.elapsedMs = now - it.startedAt
      it.status = 'running'
    },

    /** 后端完成事件 */
    applyDone(frontendId: string, ok: boolean, error?: string) {
      const it = this.items.find((i) => i.id === frontendId)
      if (!it) return
      it.elapsedMs = Date.now() - it.startedAt
      if (ok) {
        it.transferred = it.total
        it.speed = 0
        it.status = 'done'
        this.log(t('transfer.log.done', { name: it.name, duration: formatDuration(it.elapsedMs) }), 'success')
      } else if (error && error.includes('取消')) {
        it.status = 'cancelled'
        this.log(t('transfer.log.cancelled', { name: it.name }), 'error')
      } else {
        it.status = 'failed'
        this.log(`${it.name}: ${error ?? 'unknown'}`, 'error')
      }
    },

    enqueue(opts: {
      name: string
      direction: TransferDirection
      localPath: string
      remotePath: string
      total: number
      startPaused?: boolean
      sid?: number
    }) {
      const item: StoredTransfer = {
        id: `xfer-${nextId++}`,
        name: opts.name,
        direction: opts.direction,
        localPath: opts.localPath,
        remotePath: opts.remotePath,
        total: Math.max(opts.total, 1024),
        transferred: 0,
        speed: 0,
        startedAt: Date.now(),
        elapsedMs: 0,
        status: opts.startPaused ? 'paused' : 'queued',
      }
      this.items.unshift(item)
      this.log(
        opts.direction === 'upload'
          ? t('transfer.log.enqueuedUp', { name: opts.name, size: formatBytes(opts.total) })
          : t('transfer.log.enqueuedDown', { name: opts.name, size: formatBytes(opts.total) }),
      )
      this.expanded = true
      if (!opts.startPaused) this.start(item.id)
      return item.id
    },

    /** 统一入口：tauri → 后端；浏览器 → 模拟。返回首条任务 id（便于跟踪完成状态） */
    async submit(opts: {
      sid?: number
      name: string
      direction: TransferDirection
      localPath: string
      remotePath: string
      total: number
      isDir: boolean
    }): Promise<string | undefined> {
      if (isTauri && opts.sid !== undefined) {
        await this.enqueueReal(opts.sid, {
          sid: opts.sid,
          direction: opts.direction,
          isDir: opts.isDir,
          size: opts.total,
          localPath: opts.localPath,
          remotePath: opts.remotePath,
        }, opts.name)
        const first = this.items.find((i) => i.name === opts.name && i.localPath === opts.localPath)
        return first?.id
      }
      return this.enqueue(opts)
    },
    start(id: string) {
      // 从响应式数组中取代理对象，直接改原始对象不会触发视图更新
      const it = this.items.find((i) => i.id === id)
      if (!it || it.timer) return
      it.status = 'running'
      // 模拟传输速率：8~14 MB/s
      const base = 8_000_000 + Math.random() * 6_000_000
      it.timer = window.setInterval(() => {
        if (it.status !== 'running') {
          window.clearInterval(it.timer)
          it.timer = undefined
          return
        }
        const remaining = it.total - it.transferred
        const step = Math.min(remaining, base / 10 + (Math.random() * base) / 20)
        it.transferred += step
        it.speed = step * 10
        it.elapsedMs = Date.now() - it.startedAt
        if (it.transferred >= it.total) {
          this.finish(it.id, 'done')
        }
      }, 100)
    },
    pause(id: string) {
      const it = this.items.find((i) => i.id === id)
      if (it && it.status === 'running') {
        it.status = 'paused'
        this.log(t('transfer.log.paused', { name: it.name }), 'error')
        if (isTauri) transferPause(transferTid(id)).catch(() => {})
      }
    },
    resume(id: string) {
      const it = this.items.find((i) => i.id === id)
      if (it && it.status === 'paused') {
        it.startedAt = Date.now() - it.elapsedMs
        if (isTauri) {
          transferResume(transferTid(id)).catch(() => {})
          it.status = 'running'
        } else {
          this.start(id)
        }
      }
    },
    cancel(id: string) {
      const it = this.items.find((i) => i.id === id)
      if (it && (it.status === 'running' || it.status === 'queued' || it.status === 'paused')) {
        if (isTauri) {
          transferCancel(transferTid(id)).catch(() => {})
          this.finish(id, 'cancelled')
        } else {
          this.finish(id, 'cancelled')
        }
      }
    },
    /** 断点续传重试：失败/取消的任务以 resume 重新入队（目标端已有部分文件时从偏移继续） */
    async retry(id: string) {
      const old = this.items.find((i) => i.id === id)
      if (!old || !isTauri || old.sid === undefined) return
      if (old.status !== 'failed' && old.status !== 'cancelled') return
      // 原任务已结束后端清理；保留条目作占位，拿新 tid 继续驱动进度
      try {
        const results = await transferEnqueue({
          sid: old.sid,
          direction: old.direction,
          isDir: false,
          size: old.total,
          localPath: old.localPath,
          remotePath: old.remotePath,
          resume: true,
        })
        const r = results[0]
        if (!r) return
        old.id = `xfer-${r.tid}`
        old.status = 'running'
        old.transferred = 0
        old.speed = 0
        old.startedAt = Date.now()
        old.elapsedMs = 0
        this.log(t('transfer.log.resuming', { name: old.name }), 'info')
      } catch (e) {
        this.log(`${old.name}: ${String(e)}`, 'error')
      }
    },
    finish(id: string, status: TransferStatus) {
      const it = this.items.find((i) => i.id === id)
      if (!it) return
      if (it.timer) {
        window.clearInterval(it.timer)
        it.timer = undefined
      }
      it.status = status
      if (status === 'done') {
        it.speed = 0
        this.log(t('transfer.log.done', { name: it.name, duration: formatDuration(it.elapsedMs) }), 'success')
      } else if (status === 'cancelled') {
        this.log(t('transfer.log.cancelled', { name: it.name }), 'error')
      }
    },
    clearFinished() {
      this.items = this.items.filter((i) => i.status !== 'done' && i.status !== 'cancelled' && i.status !== 'failed')
    },
  },
})
