<script setup lang="ts">
/**
 * 远程文件在线编辑对话框（Win11 Fluent 风格）：
 * - 毛玻璃面板（backdrop-filter + 半透明层），配色随深/浅主题切换（--cm-* 变量）
 * - 标题栏拖动移动、8 向边缘/角落拖拽缩放
 * - CodeMirror 编辑区：行号、语法高亮（json/xml/yaml/php/python/shell 等）
 * - 一键格式化（JSON / XML / YAML）、自动换行开关
 * - 编码由后端自动识别（GBK/UTF-8/UTF-16 等），保存时按原编码/换行/BOM 写回
 * - 底部状态栏 + 保存/取消按钮；Ctrl+S 保存后保持窗口继续编辑，
 *   「保存」按钮成功后关闭窗口，Esc 取消（有改动先确认）
 */
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { Compartment, EditorState, Prec } from '@codemirror/state'
import { EditorView, keymap, type ViewUpdate } from '@codemirror/view'
import { basicSetup } from 'codemirror'
import { indentUnit } from '@codemirror/language'
import { indentWithTab } from '@codemirror/commands'
import { FButton, FConfirmDialog, toast } from '../../ui'
import { readRemoteFile, writeRemoteFile } from '../../api/backend'
import { formatBytes } from '../../utils/format'
import { t } from '../../i18n'
import type { SessionTab } from '../../api/types'
import { detectLang, plainLabel } from './editor/langs'
import { fluentEditorTheme, fluentHighlight } from './editor/theme'
import { formatDocument, formatLangOf } from './editor/format'

const props = defineProps<{
  open: boolean
  session?: SessionTab
  file: { name: string; path: string; size: number } | null
}>()

const emit = defineEmits<{
  (e: 'update:open', v: boolean): void
  (e: 'saved'): void
}>()

/* ---------- 状态 ---------- */

const loading = ref(false)
const saving = ref(false)
const dirty = ref(false)
const wrap = ref(false)
const confirmDiscard = ref(false)

const langId = ref<string | null>(null)
const langLabelText = ref('')
const encoding = ref('UTF-8')
const bom = ref(false)
const crlf = ref(false)
const lossy = ref(false)
const fileSize = ref(0)

const cursorLine = ref(1)
const cursorCol = ref(1)
const lineCount = ref(0)

const editorHost = ref<HTMLElement>()
const panelEl = ref<HTMLElement>()
let view: EditorView | null = null
let originalText = ''

const langComp = new Compartment()
const indentComp = new Compartment()
const wrapComp = new Compartment()

/* ---------- 尺寸 / 位置（拖拽移动 + 边缘缩放） ---------- */

const RESIZE_DIRS = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as const
type ResizeDir = (typeof RESIZE_DIRS)[number]
const MIN_W = 480
const MIN_H = 320

const size = ref<{ w: number; h: number } | null>(null)
const pos = ref<{ left: number; top: number } | null>(null)

const panelStyle = computed(() => {
  const s = size.value
  const p = pos.value
  if (!s || !p) return {}
  return { width: `${s.w}px`, height: `${s.h}px`, left: `${p.left}px`, top: `${p.top}px` }
})

function ensureGeometry() {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const w = Math.max(MIN_W, Math.min(size.value?.w ?? Math.min(960, vw - 96), vw - 24))
  const h = Math.max(MIN_H, Math.min(size.value?.h ?? Math.min(680, vh - 96), vh - 24))
  size.value = { w, h }
  const p = pos.value ?? {
    left: Math.round((vw - w) / 2),
    top: Math.round((vh - h) / 2) - 14,
  }
  pos.value = {
    left: Math.min(Math.max(8, p.left), Math.max(8, vw - w - 8)),
    top: Math.min(Math.max(8, p.top), Math.max(8, vh - h - 8)),
  }
}

function onHeadDown(e: MouseEvent) {
  if (e.button !== 0 || (e.target as HTMLElement).closest('button')) return
  const start = { left: pos.value!.left, top: pos.value!.top, w: size.value!.w, h: size.value!.h }
  const ox = e.clientX - start.left
  const oy = e.clientY - start.top
  const move = (ev: MouseEvent) => {
    pos.value = {
      left: Math.min(Math.max(-start.w + 120, ev.clientX - ox), window.innerWidth - 96),
      top: Math.min(Math.max(0, ev.clientY - oy), window.innerHeight - 52),
    }
  }
  const up = () => {
    document.body.style.userSelect = ''
    window.removeEventListener('mousemove', move)
    window.removeEventListener('mouseup', up)
  }
  document.body.style.userSelect = 'none'
  window.addEventListener('mousemove', move)
  window.addEventListener('mouseup', up)
}

function onResizeDown(dir: ResizeDir, e: MouseEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  const start = { left: pos.value!.left, top: pos.value!.top, w: size.value!.w, h: size.value!.h }
  const sx = e.clientX
  const sy = e.clientY
  const move = (ev: MouseEvent) => {
    const dx = ev.clientX - sx
    const dy = ev.clientY - sy
    let { left, top, w, h } = start
    if (dir.includes('e')) w = Math.min(Math.max(MIN_W, start.w + dx), window.innerWidth - left - 8)
    if (dir.includes('s')) h = Math.min(Math.max(MIN_H, start.h + dy), window.innerHeight - top - 8)
    if (dir.includes('w')) {
      w = Math.max(MIN_W, start.w - dx)
      left = start.left + start.w - w
    }
    if (dir.includes('n')) {
      h = Math.max(MIN_H, start.h - dy)
      top = start.top + start.h - h
    }
    size.value = { w, h }
    pos.value = { left, top }
  }
  const up = () => {
    document.body.style.userSelect = ''
    window.removeEventListener('mousemove', move)
    window.removeEventListener('mouseup', up)
  }
  document.body.style.userSelect = 'none'
  window.addEventListener('mousemove', move)
  window.addEventListener('mouseup', up)
}

/* ---------- 展示派生 ---------- */

const formatLang = computed(() => formatLangOf(langId.value))
const langLabel = computed(() => langLabelText.value || (props.file ? plainLabel(props.file.name) : ''))
const encodingLabel = computed(() => encoding.value + (bom.value ? ' BOM' : ''))
const statusText = computed(() =>
  [
    t('sftp.editor.cursor', { ln: cursorLine.value, col: cursorCol.value }),
    t('sftp.editor.lines', { n: lineCount.value }),
    formatBytes(fileSize.value),
  ].join(' · '),
)

/* ---------- 编辑器 ---------- */

function onEditorUpdate(u: ViewUpdate) {
  if (u.docChanged) dirty.value = true
  const head = u.state.selection.main.head
  const line = u.state.doc.lineAt(head)
  cursorLine.value = line.number
  cursorCol.value = head - line.from + 1
  lineCount.value = u.state.doc.lines
}

function createEditor(text: string) {
  const host = editorHost.value
  const file = props.file
  if (!host || !file) return
  const lang = detectLang(file.name)
  view = new EditorView({
    parent: host,
    state: EditorState.create({
      doc: text,
      extensions: [
        basicSetup,
        fluentEditorTheme,
        fluentHighlight,
        langComp.of(lang?.support() ?? []),
        indentComp.of(indentUnit.of(lang?.indent ?? '    ')),
        wrapComp.of(wrap.value ? EditorView.lineWrapping : []),
        // Ctrl+S 保存（在编辑器内按键时优先于默认行为）
        Prec.highest(
          keymap.of([
            {
              key: 'Mod-s',
              run: () => {
                void save()
                return true
              },
            },
          ]),
        ),
        keymap.of([indentWithTab]),
        EditorView.updateListener.of(onEditorUpdate),
      ],
    }),
  })
  lineCount.value = view.state.doc.lines
  view.focus()
}

function destroyEditor() {
  view?.destroy()
  view = null
}

async function startOpen() {
  const file = props.file
  if (!file) return
  loading.value = true
  saving.value = false
  dirty.value = false
  lossy.value = false
  cursorLine.value = 1
  cursorCol.value = 1
  lineCount.value = 0
  encoding.value = 'UTF-8'
  bom.value = false
  crlf.value = false
  ensureGeometry()
  await nextTick()
  try {
    const res = await readRemoteFile(props.session?.sid, file.path)
    if (!props.open) return // 读取期间已被关闭
    originalText = res.text
    encoding.value = res.encoding
    bom.value = res.bom
    crlf.value = res.crlf
    lossy.value = res.lossy
    fileSize.value = res.size
    langId.value = detectLang(file.name)?.id ?? null
    langLabelText.value = detectLang(file.name)?.label ?? ''
    createEditor(res.text)
  } catch (e) {
    toast(String(e), 'error')
    close()
  } finally {
    loading.value = false
  }
}

/* ---------- 操作 ---------- */

async function save(closeAfter = false) {
  if (!view || saving.value || loading.value) return
  saving.value = true
  try {
    const text = view.state.doc.toString()
    const n = await writeRemoteFile(props.session?.sid, props.file!.path, text, encoding.value, bom.value, crlf.value)
    originalText = text
    fileSize.value = n
    dirty.value = false
    toast(t('sftp.editor.savedToast', { name: props.file!.name, n }), 'success')
    emit('saved')
    // 底部「保存」按钮：成功后关闭窗口；Ctrl+S：保持打开继续编辑。失败时两种方式都留在窗口
    if (closeAfter) close()
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    saving.value = false
  }
}

function applyFormat() {
  if (!view || !formatLang.value || saving.value) return
  const state = view.state
  try {
    const text = state.doc.toString()
    const formatted = formatDocument(text, formatLang.value)
    if (formatted === text) return
    const anchor = Math.min(state.selection.main.head, formatted.length)
    view.dispatch({ changes: { from: 0, to: state.doc.length, insert: formatted }, selection: { anchor } })
    dirty.value = true
  } catch (e) {
    toast(`${t('sftp.editor.formatFailed')}: ${e instanceof Error ? e.message : String(e)}`, 'error')
  }
}

function toggleWrap() {
  wrap.value = !wrap.value
  view?.dispatch({ effects: wrapComp.reconfigure(wrap.value ? EditorView.lineWrapping : []) })
}

function close() {
  emit('update:open', false)
}

function requestClose() {
  if (dirty.value) confirmDiscard.value = true
  else close()
}

function forceClose() {
  dirty.value = false
  close()
}

function onDocKeydown(e: KeyboardEvent) {
  if (!props.open) return
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    e.preventDefault()
    void save()
    return
  }
  if (e.key === 'Escape') {
    // 上层的放弃确认框自行处理 Esc；CodeMirror 面板（搜索等）打开时先让 CM 处理
    if (confirmDiscard.value || view?.dom.querySelector('.cm-panel')) return
    e.preventDefault()
    requestClose()
  }
}

/* ---------- 生命周期 ---------- */

watch(
  () => props.open,
  (v) => {
    if (v) {
      document.addEventListener('keydown', onDocKeydown)
      void startOpen()
    } else {
      document.removeEventListener('keydown', onDocKeydown)
      destroyEditor()
    }
  },
)

onUnmounted(() => {
  document.removeEventListener('keydown', onDocKeydown)
  destroyEditor()
})
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fed-overlay">
      <div
        ref="panelEl"
        class="fed-panel"
        :style="panelStyle"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <div
          v-for="d in RESIZE_DIRS"
          :key="d"
          class="fed-rz"
          :class="`rz-${d}`"
          @mousedown="onResizeDown(d, $event)"
        />
        <div class="fed-head" @mousedown="onHeadDown">
          <div class="fed-titles">
            <span class="fed-name">
              <i v-if="dirty" class="fed-dirty" :title="t('sftp.editor.unsaved')" />
              {{ file?.name }}
            </span>
            <span class="fed-path mono" :title="file?.path">{{ file?.path }}</span>
          </div>
          <button class="fed-close" :title="t('common.close')" @click="requestClose">
            <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M5.5 5.5l9 9M14.5 5.5l-9 9" /></svg>
          </button>
        </div>

        <div class="fed-toolbar">
          <span class="fed-badge" :title="t('sftp.editor.langTip')">
            <svg width="12" height="12" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M7 6.5L3.5 10 7 13.5M13 6.5l3.5 3.5L13 13.5" /></svg>
            {{ langLabel }}
          </span>
          <span class="fed-badge mono" :title="t('sftp.editor.encTip')">{{ encodingLabel }}</span>
          <span v-if="lossy" class="fed-badge fed-badge-warn" :title="t('sftp.editor.lossy')">⚠</span>
          <span class="fed-toolbar-spacer" />
          <button
            class="fed-tool"
            :class="{ active: wrap }"
            :title="t('sftp.editor.wrap')"
            @click="toggleWrap"
          >
            <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M3.5 5h13M3.5 10h9.5a2.75 2.75 0 0 1 0 5.5H10M11.8 13.2L9.5 15.5l2.3 2.3M3.5 15h3" /></svg>
          </button>
          <FButton v-if="formatLang" variant="subtle" icon="wand" :disabled="saving || loading" @click="applyFormat">
            {{ t('sftp.editor.format') }}
          </FButton>
        </div>

        <div class="fed-editor-surface">
          <div ref="editorHost" class="fed-editor" :class="{ hidden: loading }" />
          <div v-if="loading" class="fed-loading">{{ t('sftp.loading') }}</div>
        </div>

        <div class="fed-foot">
          <span class="fed-status mono">{{ statusText }}</span>
          <span class="fed-foot-spacer" />
          <FButton variant="subtle" :disabled="saving" @click="requestClose">{{ t('common.cancel') }}</FButton>
          <FButton variant="accent" :disabled="saving || loading" @click="save(true)">
            {{ saving ? t('sftp.editor.saving') : t('common.save') }}
          </FButton>
        </div>
      </div>
    </div>

    <FConfirmDialog
      v-model:open="confirmDiscard"
      :title="t('sftp.editor.discardTitle')"
      :message="t('sftp.editor.discardBody', { name: file?.name ?? '' })"
      :danger="false"
      :confirm-text="t('sftp.editor.discard')"
      @confirm="forceClose"
    />
  </Teleport>
</template>

<style scoped>
.fed-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
  background: var(--bg-smoke);
  animation: fed-fade 0.14s ease;
}
@keyframes fed-fade {
  from {
    opacity: 0;
  }
}

/* ---------- Fluent 代码编辑配色（随主题切换） ---------- */
.fed-panel {
  /* 浅色（VS Light+ 色系） */
  --cm-bg: rgba(253, 253, 253, 0.78);
  --cm-fg: #1a1a1a;
  --cm-gutter: rgba(0, 0, 0, 0.34);
  --cm-active: rgba(0, 0, 0, 0.045);
  --cm-selection: rgba(0, 95, 184, 0.2);
  --cm-match: rgba(0, 95, 184, 0.3);
  --cm-search: rgba(255, 214, 0, 0.45);
  --cm-searchSel: rgba(255, 145, 0, 0.5);
  --cm-searchOutline: rgba(0, 95, 184, 0.5);
  --cm-keyword: #af00db;
  --cm-string: #a31515;
  --cm-number: #098658;
  --cm-comment: #487e52;
  --cm-function: #795e26;
  --cm-variable: #001080;
  --cm-property: #0451a5;
  --cm-type: #267f99;
  --cm-tag: #800000;
  --cm-attr: #c94343;
  --cm-operator: #3b3b3b;
  --cm-meta: #7a3e9d;
}
html[data-theme='dark'] .fed-panel {
  /* 深色（VS Dark+ 色系） */
  --cm-bg: rgba(28, 28, 28, 0.76);
  --cm-fg: #d4d4d4;
  --cm-gutter: rgba(255, 255, 255, 0.28);
  --cm-active: rgba(255, 255, 255, 0.055);
  --cm-selection: rgba(76, 194, 255, 0.22);
  --cm-match: rgba(76, 194, 255, 0.25);
  --cm-search: rgba(255, 196, 0, 0.26);
  --cm-searchSel: rgba(255, 145, 0, 0.38);
  --cm-searchOutline: rgba(76, 194, 255, 0.55);
  --cm-keyword: #569cd6;
  --cm-string: #ce9178;
  --cm-number: #b5cea8;
  --cm-comment: #6a9955;
  --cm-function: #dcdcaa;
  --cm-variable: #9cdcfe;
  --cm-property: #9cdcfe;
  --cm-type: #4ec9b0;
  --cm-tag: #569cd6;
  --cm-attr: #c8a1ff;
  --cm-operator: #d4d4d4;
  --cm-meta: #c586c0;
}

/* ---------- 面板（毛玻璃） ---------- */
.fed-panel {
  position: absolute;
  display: flex;
  flex-direction: column;
  min-width: 480px;
  min-height: 320px;
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--bg-layer) 78%, transparent);
  backdrop-filter: blur(28px) saturate(1.4);
  -webkit-backdrop-filter: blur(28px) saturate(1.4);
  box-shadow: var(--shadow-dialog);
  overflow: hidden;
  animation: fed-pop 0.16s cubic-bezier(0.16, 1, 0.3, 1);
}
@keyframes fed-pop {
  from {
    opacity: 0.4;
    transform: scale(0.97);
  }
}
.fed-panel:focus,
.fed-panel:focus-visible {
  outline: none;
}

/* ---------- 边缘缩放手柄 ---------- */
.fed-rz {
  position: absolute;
  z-index: 6;
}
.rz-n {
  top: -2px;
  left: 12px;
  right: 12px;
  height: 6px;
  cursor: ns-resize;
}
.rz-s {
  bottom: -2px;
  left: 12px;
  right: 12px;
  height: 6px;
  cursor: ns-resize;
}
.rz-e {
  right: -2px;
  top: 12px;
  bottom: 12px;
  width: 6px;
  cursor: ew-resize;
}
.rz-w {
  left: -2px;
  top: 12px;
  bottom: 12px;
  width: 6px;
  cursor: ew-resize;
}
.rz-ne {
  top: -2px;
  right: -2px;
  width: 13px;
  height: 13px;
  cursor: nesw-resize;
}
.rz-nw {
  top: -2px;
  left: -2px;
  width: 13px;
  height: 13px;
  cursor: nwse-resize;
}
.rz-se {
  bottom: -2px;
  right: -2px;
  width: 13px;
  height: 13px;
  cursor: nwse-resize;
}
.rz-sw {
  bottom: -2px;
  left: -2px;
  width: 13px;
  height: 13px;
  cursor: nesw-resize;
}

/* ---------- 标题栏 ---------- */
.fed-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 12px 8px 20px;
  cursor: move;
  flex: none;
  user-select: none;
}
.fed-titles {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.fed-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fed-dirty {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--accent);
  flex: none;
}
.fed-path {
  font-size: 11.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fed-close {
  width: 28px;
  height: 28px;
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
.fed-close:hover {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}

/* ---------- 工具栏 ---------- */
.fed-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 14px 8px 20px;
  flex: none;
}
.fed-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 9px;
  border-radius: 999px;
  border: 1px solid var(--stroke-card);
  background: var(--bg-alt-hover);
  color: var(--text-secondary);
  font-size: 11.5px;
  white-space: nowrap;
}
.fed-badge svg {
  color: var(--accent);
}
.fed-badge-warn {
  color: var(--warning);
  border-color: color-mix(in srgb, var(--warning) 45%, transparent);
  cursor: help;
}
.fed-toolbar-spacer {
  flex: 1;
}
.fed-tool {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.fed-tool:hover {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.fed-tool.active {
  color: var(--accent);
  background: var(--accent-light);
}

/* ---------- 编辑区 ---------- */
.fed-editor-surface {
  position: relative;
  flex: 1;
  min-height: 0;
  margin: 0 14px;
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  background: var(--cm-bg);
  overflow: hidden;
  display: flex;
}
.fed-editor {
  flex: 1;
  min-width: 0;
}
.fed-editor.hidden {
  visibility: hidden;
}
.fed-loading {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  font-size: 13px;
}
.fed-editor :deep(.cm-editor) {
  height: 100%;
}

/* ---------- 底栏 ---------- */
.fed-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px 12px 20px;
  flex: none;
}
.fed-status {
  font-size: 11.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fed-foot-spacer {
  flex: 1;
}

.mono {
  font-family: 'Cascadia Mono', Consolas, monospace;
}
</style>
