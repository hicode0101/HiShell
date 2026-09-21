<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import type { SessionTab } from '../../api/types'
import { useSettingsStore } from '../../stores/settings'
import { useSessionsStore } from '../../stores/sessions'
import { t } from '../../i18n'
import { isTauri } from '../../api/adapter'
import { onSshDataFor, onLocalDataFor } from '../../stores/events'
import { localResize, localWrite, sshResize, sshWrite } from '../../api/backend'
import { applySettingsToTerminals, attachTerminal, detachTerminal, terminalEntry } from './terminalRegistry'

const props = defineProps<{ tab: SessionTab }>()

const containerRef = ref<HTMLElement>()
const searchOpen = ref(false)
const searchText = ref('')
const settings = useSettingsStore()

let container: HTMLElement | null = null
let unlistenData: (() => void) | null = null
let unlistenLocal: (() => void) | null = null

/** 选项卡功能区事件（清屏/搜索/复制/粘贴/全选），按 tabId 过滤 */
function onTabCommand(e: Event) {
  const { tabId, cmd } = (e as CustomEvent<{ tabId: string; cmd: string }>).detail
  if (tabId !== props.tab.id) return
  const entry = terminalEntry(props.tab.id)
  switch (cmd) {
    case 'clear':
      entry?.term.clear()
      break
    case 'search':
      toggleSearch()
      break
    case 'copy':
      if (entry && entry.term.hasSelection()) {
        navigator.clipboard.writeText(entry.term.getSelection()).catch(() => {})
      }
      break
    case 'paste':
      if (entry) {
        navigator.clipboard
          .readText()
          .then((text) => {
            if (text) entry.term.input(text)
          })
          .catch(() => {})
      }
      break
    case 'select-all':
      entry?.term.selectAll()
      break
  }
}

onMounted(() => {
  window.addEventListener('hishell-tab-command', onTabCommand)
  if (!containerRef.value) return
  container = containerRef.value
  attachTerminal(props.tab, container)
  // 挂载时已在连接中：给出可见反馈（避免黑屏静默）
  if (isTauri && props.tab.status === 'connecting') {
    terminalEntry(props.tab.id)?.term.write(
      `[90m[HiShell] ${t('terminal.connecting', { addr: props.tab.address })}...[0m

`,
    )
  }
})
// 尺寸变化同步到后端 PTY（SSH 会话或本地终端）
watch(
  () => [props.tab.cols, props.tab.rows],
  ([cols, rows]) => {
    if (!isTauri || !cols || !rows) return
    if (props.tab.sid !== undefined) sshResize(props.tab.sid, cols, rows).catch(() => {})
    if (props.tab.lid !== undefined) localResize(props.tab.lid, cols, rows).catch(() => {})
  },
)

// 连接状态行（tauri 模式：终端内可见连接过程与失败原因）
watch(
  () => [props.tab.status, props.tab.statusMessage] as const,
  ([status, message], [prevStatus]) => {
    if (!isTauri || status === prevStatus) return
    const entry = terminalEntry(props.tab.id)
    if (!entry) return
    if (status === 'connected') {
      if (props.tab.kind === 'local') return // 本地终端开箱即连，无需横幅
      entry.term.write(`

[92m[HiShell] ${t('terminal.connected')}[0m

`)
    } else if (status === 'disconnected') {
      const text = props.tab.kind === 'local'
        ? message || t('terminal.localExited')
        : `${t('terminal.disconnected')}${message ? ' — ' + message : ''}`
      entry.term.write(
        `

[91m[HiShell] ${text}[0m

`,
      )
    }
  },
)

// 按 sid 动态解析目标终端（跨标签切换时不串线）
function resolveEntryBySid(sid: number) {
  const tab = useSessionsStore().tabs.find((t) => t.sid === sid)
  return tab ? terminalEntry(tab.id) : undefined
}

function resolveEntryByLid(lid: number) {
  const tab = useSessionsStore().tabs.find((t) => t.lid === lid)
  return tab ? terminalEntry(tab.id) : undefined
}

watch(
  () => props.tab.sid,
  (sid) => {
    if (!isTauri || sid === undefined) return
    // 重新注册（旧订阅先解除，避免累积）
    if (unlistenData) unlistenData()
    unlistenData = onSshDataFor(sid, (bytes) => {
      resolveEntryBySid(sid)?.term.write(bytes)
    })
    const entry = resolveEntryBySid(sid)
    if (entry && !entry.wired) {
      entry.wired = true
      const encoder = new TextEncoder()
      entry.term.onData((d) => {
        const tab = useSessionsStore().tabs.find((t) => t.sid === sid)
        if (tab?.sid !== undefined) {
          sshWrite(tab.sid, encoder.encode(d)).catch(() => {})
        }
      })
    }
    // 首次尺寸同步
    if (entry) sshResize(sid, entry.term.cols, entry.term.rows).catch(() => {})
  },
  { immediate: true },
)

// 本地终端（lid）：订阅 local-data，输入/尺寸走 local_* 命令
watch(
  () => props.tab.lid,
  (lid) => {
    if (!isTauri || lid === undefined) return
    if (unlistenLocal) unlistenLocal()
    unlistenLocal = onLocalDataFor(lid, (bytes) => {
      resolveEntryByLid(lid)?.term.write(bytes)
    })
    const entry = resolveEntryByLid(lid)
    if (entry && !entry.localWired) {
      entry.localWired = true
      const encoder = new TextEncoder()
      entry.term.onData((d) => {
        const tab = useSessionsStore().tabs.find((x) => x.lid === lid)
        if (tab?.lid !== undefined) {
          localWrite(tab.lid, encoder.encode(d)).catch(() => {})
        }
      })
    }
    if (entry) localResize(lid, entry.term.cols, entry.term.rows).catch(() => {})
  },
  { immediate: true },
)

onUnmounted(() => {
  window.removeEventListener('hishell-tab-command', onTabCommand)
  if (unlistenData) unlistenData()
  unlistenData = null
  if (unlistenLocal) unlistenLocal()
  unlistenLocal = null
  detachTerminal(props.tab.id)
})

watch(
  () => props.tab.id,
  () => {
    if (container) attachTerminal(props.tab, container)
  },
)

watch(
  () => [settings.s.theme, settings.s.terminalColorScheme, settings.s.terminalFontFamily, settings.s.terminalFontSize],
  () => applySettingsToTerminals(),
)

function toggleSearch() {
  searchOpen.value = !searchOpen.value
  if (searchOpen.value) {
    requestAnimationFrame(() => document.getElementById('term-search-input')?.focus())
  } else {
    terminalEntry(props.tab.id)?.search.clearDecorations()
  }
}

function findNext() {
  const e = terminalEntry(props.tab.id)
  if (!e) return
  if (!e.search.findNext(searchText.value)) e.search.findNext(searchText.value)
}
function findPrev() {
  terminalEntry(props.tab.id)?.search.findPrevious(searchText.value)
}

defineExpose({ toggleSearch })
</script>

<template>
  <div class="term-pane">
    <div v-if="searchOpen" class="term-search">
      <input
        id="term-search-input"
        v-model="searchText"
        :placeholder="t('terminal.searchPlaceholder')"
        @keydown.enter="findNext"
        @keydown.esc="toggleSearch"
        @input="findNext"
      />
      <button :title="t('terminal.prev')" @click="findPrev">▲</button>
      <button :title="t('terminal.next')" @click="findNext">▼</button>
      <button :title="t('common.close')" @click="toggleSearch">✕</button>
    </div>
    <div ref="containerRef" class="term-container" />
  </div>
</template>

<style scoped>
.term-pane {
  position: relative;
  height: 100%;
  background: var(--term-bg);
}
.term-container {
  position: absolute;
  inset: 4px 2px 2px 2px;
}
.term-search {
  position: absolute;
  top: 6px;
  right: 14px;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 2px;
  background: var(--bg-layer);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-flyout);
  padding: 4px;
}
.term-search input {
  width: 170px;
  height: 26px;
  border: none;
  outline: none;
  background: var(--bg-card);
  border-radius: var(--radius-sm);
  padding: 0 8px;
  font: inherit;
  font-size: 12px;
  color: var(--text-primary);
}
.term-search button {
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 11px;
}
.term-search button:hover {
  background: var(--bg-alt-hover);
}
</style>
