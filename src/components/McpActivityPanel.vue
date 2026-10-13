<script setup lang="ts">
/**
 * AI 操作动态面板：实时展示智能体通过 MCP 执行的每一步操作。
 * 仅当 MCP 服务运行中且设置「实时显示 AI 操作」开启时渲染；服务一停面板即时隐藏。
 * 悬浮于主窗口右下角、状态栏之上。
 */
import { computed, ref } from 'vue'
import { useSettingsStore } from '../stores/settings'
import { useMcpActivityStore, type McpActivityEntry } from '../stores/mcpActivity'
import { useMcpServerStore } from '../stores/mcpServer'
import { useProfilesStore } from '../stores/profiles'
import { isTauri } from '../api/adapter'
import { t } from '../i18n'
import { FConfirmDialog, FIcon, toast } from '../ui'

const settings = useSettingsStore()
const activity = useMcpActivityStore()
const mcpServer = useMcpServerStore()
const profiles = useProfilesStore()

const visible = computed(() => isTauri && mcpServer.running && settings.s.mcpShowActivity)

const clearOpen = ref(false)

function timeStr(ts: number): string {
  const d = new Date(ts)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

/** profileId → 配置名（找不到则原样显示） */
function profName(id: unknown): string {
  const s = typeof id === 'string' ? id : ''
  return profiles.byId(s)?.name ?? s
}

/** 把工具调用参数转成人话 */
function summarize(e: { tool: string; args?: Record<string, unknown> }): string {
  const a = e.args ?? {}
  const s = (v: unknown) => String(v ?? '')
  switch (e.tool) {
    case 'list_sessions':
      return t('activity.sumListSessions')
    case 'connect_session':
      return t('activity.sumConnect', { name: profName(a.profileId) })
    case 'disconnect_session':
      return t('activity.sumDisconnect', { name: profName(a.profileId) })
    case 'exec_command':
      return s(a.command)
    case 'upload_file':
      return t('activity.sumUpload', { local: s(a.localPath), remote: s(a.remotePath), name: profName(a.profileId) })
    case 'download_file':
      return t('activity.sumDownload', { local: s(a.localPath), remote: s(a.remotePath), name: profName(a.profileId) })
    case 'add_forward':
      return t('activity.sumAddForward', {
        type: s(a.type),
        bind: `${s(a.bindHost)}:${s(a.bindPort)}`,
        target: a.type === 'dynamic' ? 'SOCKS5' : `${s(a.targetHost)}:${s(a.targetPort)}`,
      })
    case 'remove_forward':
      return t('activity.sumRemoveForward', { id: s(a.forwardId) })
    case 'list_forwards':
      return t('activity.sumListForwards', { name: profName(a.profileId) })
    default:
      return JSON.stringify(a).slice(0, 120)
  }
}

function msStr(ms?: number): string {
  if (ms === undefined) return ''
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`
}

/** 单条记录转一行文本（复制用）：[时间] 状态 摘要 (耗时)，出错另起一行 */
function entryText(e: McpActivityEntry): string {
  const state = e.ok === undefined ? '▶' : e.ok ? '✓' : '✗'
  const ms = e.ms !== undefined ? ` (${msStr(e.ms)})` : ''
  const line = `[${timeStr(e.ts)}] ${state} ${summarize(e)}${ms}`
  return e.error ? `${line}\n  ${e.error}` : line
}

/** 复制全部记录到剪贴板（writeText 失败时回退 execCommand） */
async function copyAll() {
  const text = activity.entries.map(entryText).join('\n')
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    ta.remove()
  }
  toast(t('activity.copiedToast', { count: activity.entries.length }), 'success')
}
</script>

<template>
  <div v-if="visible" class="mcp-panel" :class="{ collapsed: activity.collapsed }">
    <div class="mcp-panel-head" @click="activity.collapsed = !activity.collapsed">
      <span class="mcp-panel-dot" />
      <span class="mcp-panel-title">{{ t('activity.title') }}</span>
      <span class="mcp-panel-count">{{ activity.entries.length }}</span>
      <span class="mcp-panel-spacer" />
      <template v-if="!activity.collapsed && activity.entries.length">
        <button class="mcp-panel-icon-btn" :title="t('activity.copyAll')" @click.stop="copyAll">
          <FIcon name="copy" :size="14" />
        </button>
        <button class="mcp-panel-icon-btn" :title="t('activity.clear')" @click.stop="clearOpen = true">
          <FIcon name="trash" :size="14" />
        </button>
      </template>
      <span class="mcp-panel-chev">{{ activity.collapsed ? '▲' : '▼' }}</span>
    </div>
    <div v-if="!activity.collapsed" class="mcp-panel-body">
      <div v-if="!activity.entries.length" class="mcp-panel-empty">{{ t('activity.empty') }}</div>
      <div v-for="e in activity.entries" :key="e.callId" class="mcp-item">
        <span class="mcp-item-time">{{ timeStr(e.ts) }}</span>
        <span class="mcp-item-state" :class="e.ok === undefined ? 'run' : e.ok ? 'ok' : 'fail'">
          {{ e.ok === undefined ? '▶' : e.ok ? '✓' : '✗' }}
        </span>
        <div class="mcp-item-main">
          <div class="mcp-item-text" :class="{ mono: e.tool === 'exec_command' }">{{ summarize(e) }}</div>
          <div v-if="e.error" class="mcp-item-err">{{ e.error }}</div>
        </div>
        <span v-if="e.ok !== undefined" class="mcp-item-ms">{{ msStr(e.ms) }}</span>
      </div>
    </div>
    <!-- 清空二次确认，防止误点 -->
    <FConfirmDialog
      v-model:open="clearOpen"
      :title="t('activity.clearConfirmTitle')"
      :message="t('activity.clearConfirmMessage')"
      :confirm-text="t('activity.clear')"
      @confirm="activity.clear()"
    />
  </div>
</template>

<style scoped>
.mcp-panel {
  position: fixed;
  right: 12px;
  bottom: 36px;
  width: 420px;
  max-height: 320px;
  display: flex;
  flex-direction: column;
  background: var(--bg-card);
  border: 1px solid var(--stroke-strong);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.18);
  z-index: 900;
  overflow: hidden;
}
.mcp-panel-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  cursor: pointer;
  border-bottom: 1px solid var(--stroke-divider);
  background: var(--bg-alt-hover);
  user-select: none;
}
.mcp-panel.collapsed .mcp-panel-head {
  border-bottom: none;
}
.mcp-panel-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #f7630c;
  flex: none;
}
.mcp-panel-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
}
.mcp-panel-count {
  font-size: 11px;
  color: var(--text-tertiary);
  background: var(--bg-alt-hover);
  border-radius: 8px;
  padding: 0 7px;
}
.mcp-panel-spacer {
  flex: 1;
}
.mcp-panel-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 0;
  border-radius: var(--radius-sm);
}
.mcp-panel-icon-btn:hover {
  color: var(--text-primary);
  background: var(--bg-alt-pressed);
}
.mcp-panel-chev {
  font-size: 10px;
  color: var(--text-tertiary);
}
.mcp-panel-body {
  overflow: auto;
  padding: 6px 0;
}
.mcp-panel-empty {
  padding: 18px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12.5px;
}
.mcp-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 5px 12px;
}
.mcp-item-time {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  flex: none;
  padding-top: 1px;
}
.mcp-item-state {
  flex: none;
  font-size: 11px;
  padding-top: 1px;
}
.mcp-item-state.run {
  color: #f7630c;
}
.mcp-item-state.ok {
  color: #0f7b0f;
}
.mcp-item-state.fail {
  color: var(--danger);
}
.mcp-item-main {
  flex: 1;
  min-width: 0;
}
.mcp-item-text {
  font-size: 12.5px;
  color: var(--text-primary);
  word-break: break-all;
  line-height: 1.45;
}
.mcp-item-text.mono {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
}
.mcp-item-err {
  font-size: 11.5px;
  color: var(--danger);
  margin-top: 2px;
  word-break: break-all;
}
.mcp-item-ms {
  flex: none;
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  padding-top: 1px;
}
</style>
