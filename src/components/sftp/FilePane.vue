<script setup lang="ts">
/**
 * 文件窗格：本地 / 远程通用。
 * - 本地：真实列目录（Tauri）/ mock 文件树（浏览器）
 * - 远程：mock 数据（M1 由 sftp_list 事件替换）
 * - 拖拽：行可拖拽 → 落到对侧窗格入传输队列；接受系统文件拖入
 */
import { computed, onMounted, ref, watch } from 'vue'
import type { DirEntry, SessionTab } from '../../api/types'
import { listLocalDir, listRemoteDir, specialDir, mockSpecialDirs } from '../../api/backend'
import { formatBytes, formatTime, fileKindLabel } from '../../utils/format'
import { t } from '../../i18n'
import { showContextMenu, toast } from '../../ui'

const props = defineProps<{
  side: 'local' | 'remote'
  session?: SessionTab
}>()

const emit = defineEmits<{
  (e: 'transfer-request', payload: TransferRequest): void
  (e: 'counts-changed', counts: { files: number; dirs: number }): void
  (e: 'path-changed', path: string): void
}>()

export interface TransferRequest {
  entries: DirEntry[]
  /** 来源窗格 */
  from: 'local' | 'remote'
  fromPath: string
}

const path = ref('')
const entries = ref<DirEntry[]>([])
const loading = ref(false)
const loadError = ref('')
const selected = ref(new Set<string>())
const lastSelected = ref('')
const sortKey = ref<'name' | 'size' | 'type' | 'mtime'>('name')
const sortAsc = ref(true)
const dragOver = ref(false)

const backStack = ref<string[]>([])
const forwardStack = ref<string[]>([])

const paneTitle = computed(() => (props.side === 'local' ? t('sftp.local') : props.session?.title || t('sftp.remote')))
const paneTabLabel = computed(() =>
  props.side === 'local' ? `${paneTitle.value}` : `${paneTitle.value}（${currentDirName.value}）`,
)
const currentDirName = computed(() => {
  if (!path.value) return t('sftp.thisPC')
  const norm = path.value.replace(/[\\/]+$/, '')
  const idx = Math.max(norm.lastIndexOf('\\'), norm.lastIndexOf('/'))
  return idx >= 0 ? norm.slice(idx + 1) : norm
})

const quickLinks = [
  { labelKey: 'sftp.quick.desktop', kind: 'desktop', icon: 'monitor' },
  { labelKey: 'sftp.quick.documents', kind: 'documents', icon: 'file-text' },
  { labelKey: 'sftp.quick.downloads', kind: 'downloads', icon: 'download' },
  { labelKey: 'sftp.quick.pictures', kind: 'pictures', icon: 'file' },
  { labelKey: 'sftp.quick.home', kind: 'home', icon: 'server' },
] as const

/* ---------- 加载 ---------- */

async function load(p: string, pushBack = true) {
  loading.value = true
  selected.value = new Set()
  try {
    const list =
      props.side === 'local'
        ? await listLocalDir(p)
        : await listRemoteDir(props.session?.sid, p, props.session?.status === 'connected')
    entries.value = list
    loadError.value = ''
    if (pushBack && p !== path.value) {
      backStack.value.push(path.value)
      forwardStack.value = []
    }
    path.value = p
    emit('path-changed', p)
    emit('counts-changed', {
      files: list.filter((x) => x.kind !== 'dir' && x.name !== '..').length,
      dirs: list.filter((x) => x.kind === 'dir' && x.name !== '..').length,
    })
  } catch (e) {
    entries.value = []
    loadError.value = String(e)
  } finally {
    loading.value = false
  }
}

function reload() {
  load(path.value, false)
}

onMounted(async () => {
  if (props.side === 'local') {
    const desktop = await specialDir('desktop').catch(() => '')
    load(desktop || mockSpecialDirs.desktop || '')
  } else {
    load('/root', false)
  }
})

// 远程窗格：会话变为已连接时自动（重新）加载真实目录
watch(
  () => props.session?.status,
  (st) => {
    if (props.side === 'remote' && st === 'connected') reload()
  },
)

watch(
  () => props.session?.id,
  () => {
    if (props.side === 'remote') load('/root', false)
  },
)

/* ---------- 导航 ---------- */

function goUp() {
  const p = path.value
  if (props.side === 'remote') {
    if (p === '' || p === '/') return
    const parent = p.slice(0, p.replace(/\/$/, '').lastIndexOf('/')) || '/'
    load(parent === '/' ? '/' : parent)
  } else {
    const norm = p.replace(/[\\/]+$/, '')
    if (!norm) return
    const idx = Math.max(norm.lastIndexOf('\\'), norm.lastIndexOf('/'))
    if (idx < 0) load('')
    else load(norm.slice(0, idx))
  }
}
function goBack() {
  if (!backStack.value.length) return
  const prev = backStack.value.pop()!
  forwardStack.value.push(path.value)
  load(prev, false)
}
function goForward() {
  if (!forwardStack.value.length) return
  const next = forwardStack.value.pop()!
  backStack.value.push(path.value)
  load(next, false)
}
function goto(p: string) {
  load(p)
}

/** 快速链接：解析系统特殊目录后跳转 */
async function gotoSpecial(kind: string) {
  const target = await specialDir(kind).catch(() => '')
  if (target) load(target)
}

/* ---------- 选择 / 排序 ---------- */

function onRowClick(e: MouseEvent, entry: DirEntry, idx: number) {
  if (e.shiftKey && lastSelected.value) {
    const start = entries.value.findIndex((x) => x.name === lastSelected.value)
    const range = entries.value.slice(Math.min(start, idx), Math.max(start, idx) + 1)
    if (!e.ctrlKey) selected.value = new Set()
    range.forEach((x) => selected.value.add(x.name))
  } else if (e.ctrlKey) {
    const next = new Set(selected.value)
    if (next.has(entry.name)) next.delete(entry.name)
    else next.add(entry.name)
    selected.value = next
  } else {
    selected.value = new Set([entry.name])
  }
  lastSelected.value = entry.name
}

function setSort(key: typeof sortKey.value) {
  if (sortKey.value === key) sortAsc.value = !sortAsc.value
  else {
    sortKey.value = key
    sortAsc.value = true
  }
}

const sortedEntries = computed(() => {
  const dirFirst = (a: DirEntry, b: DirEntry) => (a.name === '..' ? -1 : b.name === '..' ? 1 : a.kind === b.kind ? 0 : a.kind === 'dir' ? -1 : 1)
  const list = [...entries.value]
  const dir = sortAsc.value ? 1 : -1
  list.sort((a, b) => {
    const d = dirFirst(a, b)
    if (d !== 0) return d
    switch (sortKey.value) {
      case 'size':
        return (a.size - b.size) * dir
      case 'mtime':
        return (a.mtime - b.mtime) * dir
      case 'type':
        return a.kind.localeCompare(b.kind) * dir
      default:
        return a.name.localeCompare(b.name, 'zh-CN') * dir
    }
  })
  return list
})

const counts = computed(() => ({
  files: entries.value.filter((e) => e.kind !== 'dir' && e.name !== '..').length,
  dirs: entries.value.filter((e) => e.kind === 'dir' && e.name !== '..').length,
}))

/* ---------- 拖拽 ---------- */

const MIME = 'application/x-hishell-transfer'

function onDragStart(e: DragEvent, entry: DirEntry) {
  const sel = selected.value.has(entry.name) ? entries.value.filter((x) => selected.value.has(x.name)) : [entry]
  e.dataTransfer?.setData(MIME, JSON.stringify({ from: props.side, fromPath: path.value, entries: sel }))
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy'
}

function onDrop(e: DragEvent) {
  dragOver.value = false
  e.preventDefault()
  const raw = e.dataTransfer?.getData(MIME)
  if (raw) {
    const payload = JSON.parse(raw) as TransferRequest
    if (payload.from !== props.side) emit('transfer-request', payload)
    return
  }
  // 系统外部文件拖入
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (!files.length) return
  const entriesPayload: DirEntry[] = files.map((f) => ({
    name: f.name,
    kind: 'file',
    size: f.size,
    mtime: f.lastModified,
  }))
  emit('transfer-request', { entries: entriesPayload, from: props.side, fromPath: path.value })
}

/* ---------- 右键菜单 ---------- */

function rowContextMenu(e: MouseEvent, entry: DirEntry) {
  onRowClick(e, entry, entries.value.findIndex((x) => x.name === entry.name))
  const targets = entries.value.filter((x) => selected.value.has(x.name))
  showContextMenu(e, [
    {
      label: props.side === 'remote' ? t('sftp.download') : t('sftp.upload'),
      icon: props.side === 'remote' ? 'download' : 'upload',
      disabled: targets.every((t) => t.name === '..'),
      onClick: () => emit('transfer-request', { entries: targets, from: props.side, fromPath: path.value }),
    },
    { separator: true },
    {
      label: t('sftp.rename'),
      icon: 'edit',
      onClick: () => toast(t('sftp.renameToast'), 'info'),
    },
    {
      label: t('sftp.delete'),
      icon: 'trash',
      danger: true,
      onClick: () => toast(t('sftp.deleteToast'), 'info'),
    },
    { separator: true },
    {
      label: t('sftp.props'),
      icon: 'info',
      onClick: () =>
        toast(
          `${entry.name} — ${entry.kind === 'dir' ? t('sftp.folder') : formatBytes(entry.size)}${entry.attrs ? ' ' + entry.attrs : ''}`,
        ),
    },
  ])
}

function paneContextMenu(e: MouseEvent) {
  showContextMenu(e, [
    { label: t('common.refresh'), icon: 'refresh', onClick: reload },
    { label: t('sftp.newFolder'), icon: 'folder', onClick: () => toast(t('sftp.newFolderToast')) },
    { separator: true },
    { label: t('sftp.paste'), icon: 'copy', disabled: true },
  ])
}

async function openEntry(entry: DirEntry) {
  if (entry.name === '..') {
    goUp()
    return
  }
  if (entry.kind === 'dir') {
    const child = joinPath(path.value, entry.name)
    load(child)
  } else {
    toast(`打开文件「${entry.name}」将在后续版本提供（可配置默认编辑器）`)
  }
}

function joinPath(base: string, name: string): string {
  if (!base) return name
  return props.side === 'remote'
    ? (base === '/' ? '' : base) + '/' + name
    : base.replace(/[\\/]+$/, '') + '\\' + name
}

defineExpose({ counts })
</script>

<template>
  <div
    class="file-pane"
    :class="{ 'drag-over': dragOver, loading, 'is-remote': side === 'remote' }"
    @dragover.prevent="dragOver = true"
    @dragleave="dragOver = false"
    @drop="onDrop"
    @contextmenu="paneContextMenu"
  >
    <div class="pane-head">
      <span class="pane-tab" :class="side">
        <span class="pane-dot" />
        {{ paneTitle }}<template v-if="side === 'remote'"> · {{ currentDirName }}</template>
      </span>
    </div>

    <div class="pathbar">
      <button class="pb-btn" :title="t('sftp.back')" :disabled="!backStack.length" @click="goBack">
        <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M12.5 5L7.5 10l5 5" /></svg>
      </button>
      <button class="pb-btn" :title="t('sftp.forward')" :disabled="!forwardStack.length" @click="goForward">
        <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M7.5 5l5 5-5 5" /></svg>
      </button>
      <button class="pb-btn" :title="t('sftp.levelUp')" @click="goUp">
        <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M5 12.5L10 7.5l5 5" /></svg>
      </button>
      <div class="pb-path">
        <svg v-if="side === 'local'" width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="3" y="4.5" width="14" height="9.5" rx="1.2" /><path d="M7 17h6M10 14v3" /></svg>
        <svg v-else width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="3.5" y="3.5" width="13" height="5.5" rx="1" /><rect x="3.5" y="11" width="13" height="5.5" rx="1" /><path d="M6 6.25h.01M6 13.75h.01" /></svg>
        <input
          class="pb-input"
          :value="path || (side === 'local' ? t('sftp.thisPC') : '/')"
          spellcheck="false"
          @keydown.enter="goto(($event.target as HTMLInputElement).value.trim())"
        />
        <button class="pb-btn" :title="t('common.refresh')" @click="reload">
          <svg width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M16 10a6 6 0 1 1-1.76-4.24M16.5 3.5v3h-3" /></svg>
        </button>
      </div>
    </div>

    <div class="pane-body">
      <div v-if="side === 'local'" class="quick-links">
        <div
          v-for="ql in quickLinks"
          :key="ql.kind"
          class="quick-link"
          :title="t(ql.labelKey)"
          @click="gotoSpecial(ql.kind)"
        >
          <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
            <template v-if="ql.icon === 'monitor'"><rect x="3" y="4.5" width="14" height="9.5" rx="1.2" /><path d="M7 17h6M10 14v3" /></template>
            <template v-else-if="ql.icon === 'file-text'"><path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6M7.5 11.5h5M7.5 14h5" /></template>
            <template v-else-if="ql.icon === 'download'"><path d="M10 3.5v9M6.2 9.3l3.8 3.7 3.8-3.7M4.5 16.5h11" /></template>
            <template v-else-if="ql.icon === 'file'"><path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6" /></template>
            <template v-else><rect x="3.5" y="3.5" width="13" height="5.5" rx="1" /><rect x="3.5" y="11" width="13" height="5.5" rx="1" /><path d="M6 6.25h.01M6 13.75h.01" /></template>
          </svg>
          <span>{{ t(ql.labelKey) }}</span>
        </div>
      </div>

      <div class="file-table-wrap">
        <div class="file-table">
          <div class="ft-head">
            <span class="col-name" @click="setSort('name')">
              {{ t('sftp.colName') }}
              <span v-if="sortKey === 'name'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span>
            </span>
            <span class="col-size" @click="setSort('size')">{{ t('sftp.colSize') }} <span v-if="sortKey === 'size'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span></span>
            <span class="col-type" @click="setSort('type')">{{ t('sftp.colType') }} <span v-if="sortKey === 'type'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span></span>
            <span class="col-mtime" @click="setSort('mtime')">{{ t('sftp.colMtime') }} <span v-if="sortKey === 'mtime'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span></span>
            <span v-if="side === 'remote'" class="col-attrs">{{ t('sftp.colAttrs') }}</span>
          </div>
          <div class="ft-body">
            <div
              v-for="(entry, idx) in sortedEntries"
              :key="entry.name"
              class="ft-row"
              :class="{ selected: selected.has(entry.name), dir: entry.kind === 'dir' }"
              draggable="true"
              @click="onRowClick($event, entry, idx)"
              @dblclick="openEntry(entry)"
              @dragstart="onDragStart($event, entry)"
              @contextmenu.prevent.stop="rowContextMenu($event, entry)"
            >
              <span class="col-name">
                <svg v-if="entry.kind === 'dir'" width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" class="ic dir-ic">
                  <path d="M3.5 6.3c0-.7.6-1.3 1.3-1.3h3l1.8 1.8h5.6c.7 0 1.3.6 1.3 1.3v7.6c0 .7-.6 1.3-1.3 1.3H4.8c-.7 0-1.3-.6-1.3-1.3V6.3z" />
                </svg>
                <svg v-else width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" class="ic">
                  <path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6" />
                </svg>
                {{ entry.name }}
              </span>
              <span class="col-size">{{ entry.kind === 'dir' ? '' : formatBytes(entry.size) }}</span>
              <span class="col-type">{{ fileKindLabel(entry.kind, entry.name) }}</span>
              <span class="col-mtime">{{ formatTime(entry.mtime) }}</span>
              <span v-if="side === 'remote'" class="col-attrs mono">{{ entry.attrs || '' }}</span>
            </div>
            <div v-if="!sortedEntries.length && !loading" class="ft-empty">{{ loadError || t('sftp.emptyFolder') }}</div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="loading" class="pane-loading">{{ t('sftp.loading') }}</div>
  </div>
</template>

<style scoped>
.file-pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  background: var(--bg-card);
  overflow: hidden;
  position: relative;
}
.file-pane.drag-over {
  border-color: var(--accent);
  box-shadow: inset 0 0 0 1px var(--accent);
}
.pane-head {
  display: flex;
  align-items: center;
  padding: 6px 8px 2px;
  flex: none;
}
.pane-tab {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  background: var(--bg-alt-hover);
  padding: 4px 12px;
  border-radius: 6px 6px 0 0;
}
.pane-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #2ecc71;
}
.pane-dot + * {
  margin-left: 2px;
}
.pathbar {
  display: flex;
  align-items: center;
  gap: 3px;
  padding: 4px 8px 8px;
  flex: none;
}
.pb-btn {
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex: none;
}
.pb-btn:hover:not(:disabled) {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.pb-btn:disabled {
  color: var(--text-disabled);
  cursor: default;
}
.pb-path {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 4px 0 10px;
  background: var(--bg-card);
  border: 1px solid var(--stroke-control);
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
}
.pb-path:focus-within {
  border-color: var(--accent);
}
.pb-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 12.5px;
  color: var(--text-primary);
}
.pane-body {
  flex: 1;
  display: flex;
  min-height: 0;
}
.quick-links {
  width: 108px;
  flex: none;
  border-right: 1px solid var(--stroke-divider);
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow: auto;
}
.quick-link {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 12.5px;
  color: var(--text-primary);
  white-space: nowrap;
}
.quick-link:hover {
  background: var(--bg-alt-hover);
}
.quick-link svg {
  color: var(--accent);
  flex: none;
}
.file-table-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.file-table {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.ft-head,
.ft-row {
  display: grid;
  grid-template-columns: minmax(140px, 2fr) 76px 90px 150px;
  gap: 8px;
  align-items: center;
  padding: 0 10px;
}
.is-remote .ft-head,
.is-remote .ft-row {
  grid-template-columns: minmax(140px, 2fr) 76px 90px 150px 110px;
}
.ft-head {
  height: 28px;
  font-size: 12px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--stroke-divider);
  user-select: none;
  flex: none;
}
.ft-head span {
  cursor: pointer;
  white-space: nowrap;
}
.ft-head span:hover {
  color: var(--text-primary);
}
.sort-arrow {
  font-size: 9px;
}
.ft-body {
  flex: 1;
  overflow: auto;
  padding-bottom: 4px;
}
.ft-row {
  height: 28px;
  font-size: 12.5px;
  cursor: default;
  border-radius: var(--radius-sm);
  margin: 0 4px;
  width: calc(100% - 8px);
}
.ft-row:hover {
  background: var(--bg-alt-hover);
}
.ft-row.selected {
  background: var(--bg-selected);
}
.ft-row.selected .col-name {
  outline: 1px solid var(--stroke-card);
  border-radius: 3px;
}
.col-name {
  display: flex;
  align-items: center;
  gap: 7px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.col-size,
.col-type,
.col-mtime,
.col-attrs {
  color: var(--text-secondary);
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ic {
  color: #f8b64c;
  flex: none;
}
.ic:not(.dir-ic) {
  color: var(--text-tertiary);
}
.ft-empty {
  padding: 24px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12.5px;
}
.pane-loading {
  position: absolute;
  right: 10px;
  bottom: 6px;
  font-size: 11.5px;
  color: var(--text-tertiary);
  background: var(--bg-card);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-sm);
  padding: 2px 8px;
}
</style>
