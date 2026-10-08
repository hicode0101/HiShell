<script setup lang="ts">
/**
 * 本地终端会话管理器：会话名 + 默认工作路径。
 * 点击会话名/终端图标即按配置路径开启本地终端；
 * 支持 拖动排序 / 编辑 / 复制 / 删除（与 SSH 会话管理器交互一致）。
 * 配置整表存配置目录 local_terminals.json，改动即保存。
 */
import { computed, ref, watch } from 'vue'
import { FButton, FConfirmDialog, FDialog, toast } from '../ui'
import { isTauri } from '../api/adapter'
import { localSessionList, localSessionSave } from '../api/backend'
import type { LocalTermSession } from '../api/types'
import { useSessionsStore } from '../stores/sessions'
import { t } from '../i18n'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const sessions = useSessionsStore()

const list = ref<LocalTermSession[]>([])
const loadedOnce = ref(false)

watch(
  () => props.open,
  async (open) => {
    if (!open) return
    if (loadedOnce.value) return
    loadedOnce.value = true
    if (isTauri) {
      try {
        list.value = await localSessionList()
      } catch (e) {
        toast(String(e), 'error')
      }
    }
  },
)

async function persist() {
  if (!isTauri) return
  try {
    await localSessionSave(JSON.parse(JSON.stringify(list.value)))
  } catch (e) {
    toast(String(e), 'error')
  }
}

function newId(): string {
  return `lts-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/* ---------- 打开终端 ---------- */

function openTerminal(s: LocalTermSession) {
  sessions.createLocalTab({ name: s.name, path: s.path || undefined })
  emit('update:open', false)
}

/* ---------- 行拖拽排序（行首把手，鼠标模拟；与 SSH 会话管理器一致） ---------- */

const listEl = ref<HTMLElement>()
const gripDragging = ref(false)
const gripIndex = ref(-1)
let gripMoved = false

function onGripMouseDown(e: MouseEvent, index: number) {
  if (e.button !== 0) return
  e.preventDefault()
  gripIndex.value = index
  gripDragging.value = true
  document.body.style.userSelect = 'none'
  document.addEventListener('mousemove', onGripMove)
  document.addEventListener('mouseup', onGripUp)
}

function onGripMove(e: MouseEvent) {
  if (gripIndex.value < 0 || !listEl.value) return
  const rowEls = [...listEl.value.querySelectorAll('.lts-row')]
  const hover = rowEls.findIndex((r) => {
    const b = r.getBoundingClientRect()
    return e.clientY >= b.top && e.clientY <= b.bottom
  })
  if (hover < 0 || hover === gripIndex.value) return
  const [row] = list.value.splice(gripIndex.value, 1)
  list.value.splice(hover, 0, row)
  gripIndex.value = hover
  gripMoved = true
}

async function onGripUp() {
  document.removeEventListener('mousemove', onGripMove)
  document.removeEventListener('mouseup', onGripUp)
  document.body.style.userSelect = ''
  gripDragging.value = false
  if (gripMoved) await persist()
  gripIndex.value = -1
  gripMoved = false
}

/* ---------- 新建 / 编辑（复用一个表单对话框） ---------- */

const formOpen = ref(false)
const form = ref<{ id: string | null; name: string; path: string }>({ id: null, name: '', path: '' })
const formTitle = computed(() => (form.value.id ? t('localSessions.editTitle') : t('localSessions.newTitle')))

function openNew() {
  form.value = { id: null, name: '', path: '' }
  formOpen.value = true
}

function openEdit(s: LocalTermSession) {
  form.value = { id: s.id, name: s.name, path: s.path }
  formOpen.value = true
}

async function saveForm() {
  const name = form.value.name.trim()
  if (!name) {
    toast(t('localSessions.nameRequired'), 'error')
    return
  }
  const path = form.value.path.trim()
  if (form.value.id) {
    const row = list.value.find((s) => s.id === form.value.id)
    if (row) {
      row.name = name
      row.path = path
    }
  } else {
    list.value.push({ id: newId(), name, path })
  }
  await persist()
  formOpen.value = false
  toast(t('localSessions.savedToast', { name }), 'success')
}

/** 目录浏览器（tauri 对话框插件）；失败静默，保留手输 */
async function browsePath() {
  if (!isTauri) return
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const dir = await open({ directory: true, title: t('localSessions.path') })
    if (typeof dir === 'string' && dir) form.value.path = dir
  } catch {
    /* 浏览器预览环境无该插件 */
  }
}

/* ---------- 复制 / 删除 ---------- */

async function copyConfig(s: LocalTermSession) {
  const copy: LocalTermSession = { id: newId(), name: s.name + t('sessions.copySuffix'), path: s.path }
  list.value.push(copy)
  await persist()
  toast(t('localSessions.copiedToast', { name: copy.name }), 'success')
}

const deleteTarget = ref<LocalTermSession | null>(null)

function remove(s: LocalTermSession) {
  deleteTarget.value = s
}

async function confirmRemove() {
  const s = deleteTarget.value
  if (!s) return
  list.value = list.value.filter((x) => x.id !== s.id)
  await persist()
  toast(t('localSessions.deletedToast', { name: s.name }), 'success')
  deleteTarget.value = null
}
</script>

<template>
  <FDialog :open="open" :title="t('localSessions.title')" :width="620" @update:open="emit('update:open', $event)">
    <template #title-extra>
      <div class="lts-count">{{ t('sessions.totalPrefix') }}<span class="lts-count-num">{{ list.length }}</span>{{ t('sessions.totalSuffix') }}</div>
    </template>
    <div v-if="!list.length" class="lts-empty">{{ t('localSessions.empty') }}</div>
    <div v-else ref="listEl" class="lts-list" :class="{ 'grip-dragging': gripDragging }">
      <div
        v-for="(row, idx) in list"
        :key="row.id"
        class="lts-row"
        :class="{ 'dragging-row': gripDragging && gripIndex === idx }"
      >
        <span class="lts-grip" :title="t('localSessions.dragSort')" @mousedown="onGripMouseDown($event, idx)">
          <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor">
            <circle cx="2.6" cy="3" r="1.4" /><circle cx="7.4" cy="3" r="1.4" />
            <circle cx="2.6" cy="8" r="1.4" /><circle cx="7.4" cy="8" r="1.4" />
            <circle cx="2.6" cy="13" r="1.4" /><circle cx="7.4" cy="13" r="1.4" />
          </svg>
        </span>
        <div class="lts-main" :title="t('localSessions.openTerminal')" @click="openTerminal(row)">
          <div class="lts-name">{{ row.name }}</div>
          <div class="lts-path">{{ row.path || '—' }}</div>
        </div>
        <div class="lts-ops">
          <button class="lts-ic-btn connect" :title="t('localSessions.openTerminal')" @click="openTerminal(row)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
              <rect x="3" y="4.5" width="14" height="11" rx="1.5" /><path d="M6.5 8l2.8 2.3-2.8 2.3M11.2 13h3" />
            </svg>
          </button>
          <button class="lts-ic-btn edit" :title="t('common.edit')" @click="openEdit(row)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M13.1 3.6l3.3 3.3L7 16.3l-4 1 1-4 9.1-9.7zM11.7 5l3.3 3.3" />
            </svg>
          </button>
          <button class="lts-ic-btn copy" :title="t('sessions.copy')" @click="copyConfig(row)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <rect x="7" y="7" width="9" height="9" rx="1.6" />
              <path d="M12.6 4.5H5.6A1.6 1.6 0 0 0 4 6.1v7" />
            </svg>
          </button>
          <button class="lts-ic-btn del" :title="t('common.delete')" @click="remove(row)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M4.5 5.5h11M8 5.5V4.3c0-.44.36-.8.8-.8h2.4c.44 0 .8.36.8.8v1.2M6 5.5l.7 9.8c.04.66.6 1.2 1.27 1.2h4.06c.66 0 1.23-.54 1.27-1.2l.7-9.8M8.4 8.5v5M11.6 8.5v5" />
            </svg>
          </button>
        </div>
      </div>
    </div>
    <template #footer>
      <FButton icon="plus" @click="openNew">{{ t('localSessions.newBtn') }}</FButton>
      <div class="lts-spacer" />
      <FButton variant="subtle" @click="emit('update:open', false)">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>

  <!-- 新建 / 编辑表单 -->
  <FDialog :open="formOpen" :title="formTitle" :width="480" @update:open="formOpen = $event">
    <div class="lts-form">
      <label class="lts-field">
        <span class="lts-label">{{ t('localSessions.name') }}</span>
        <input v-model="form.name" :placeholder="t('localSessions.namePlaceholder')" spellcheck="false" @keydown.enter="saveForm" />
      </label>
      <label class="lts-field">
        <span class="lts-label">{{ t('localSessions.path') }}</span>
        <div class="lts-path-input">
          <input v-model="form.path" :placeholder="t('localSessions.pathPlaceholder')" spellcheck="false" @keydown.enter="saveForm" />
          <FButton icon="folder" variant="subtle" @click="browsePath">{{ t('localSessions.browse') }}</FButton>
        </div>
      </label>
    </div>
    <template #footer>
      <div class="lts-spacer" />
      <FButton variant="subtle" @click="formOpen = false">{{ t('common.cancel') }}</FButton>
      <FButton variant="accent" icon="check" @click="saveForm">{{ t('common.save') }}</FButton>
    </template>
  </FDialog>

  <!-- 删除确认 -->
  <FConfirmDialog
    :open="!!deleteTarget"
    :title="t('common.delete')"
    :message="t('localSessions.confirmDelete', { name: deleteTarget?.name ?? '' })"
    @update:open="(v: boolean) => !v && (deleteTarget = null)"
    @confirm="confirmRemove"
  />
</template>

<style scoped>
.lts-empty {
  padding: 30px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
}
.lts-list {
  max-height: 380px;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.lts-list.grip-dragging {
  cursor: grabbing;
}
.lts-row.dragging-row {
  background: var(--bg-selected);
}
.lts-grip {
  flex: none;
  width: 16px;
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  cursor: grab;
  border-radius: 4px;
}
.lts-grip:hover {
  color: var(--text-secondary);
  background: var(--bg-alt-hover);
}
.lts-grip:active {
  cursor: grabbing;
}
.lts-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 10px;
  border-radius: var(--radius-md);
}
.lts-row:hover {
  background: var(--bg-alt-hover);
}
.lts-main {
  flex: 1;
  min-width: 0;
  cursor: pointer;
}
.lts-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.lts-row:hover .lts-name {
  color: var(--accent);
}
.lts-path {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  margin-top: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.lts-ops {
  display: flex;
  gap: 6px;
  flex: none;
}
.lts-ic-btn {
  width: 30px;
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 0;
}
.lts-ic-btn:hover {
  background: var(--bg-alt-hover);
}
.lts-ic-btn.connect {
  background: var(--accent);
  color: #fff;
}
.lts-ic-btn.connect:hover {
  background: var(--accent);
  filter: brightness(1.12);
}
.lts-ic-btn.edit:hover {
  color: var(--warning);
}
.lts-ic-btn.copy {
  color: var(--success);
}
.lts-ic-btn.del {
  color: var(--danger);
}
.lts-ic-btn.del:hover {
  background: rgba(209, 52, 56, 0.12);
}
.lts-spacer {
  flex: 1;
}
.lts-count {
  color: var(--text-tertiary);
  font-size: 13px;
  font-weight: 400;
  white-space: nowrap;
}
.lts-count-num {
  color: var(--accent);
  font-size: 15px;
  font-weight: 700;
  margin: 0 3px;
  font-family: 'Cascadia Mono', Consolas, monospace;
}
/* 表单 */
.lts-form {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 4px 2px;
}
.lts-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.lts-label {
  font-size: 12.5px;
  color: var(--text-secondary);
}
.lts-field input {
  height: 32px;
  border: 1px solid var(--stroke-control);
  background: var(--bg-card);
  border-radius: var(--radius-sm);
  padding: 0 10px;
  font: inherit;
  font-size: 12.5px;
  color: var(--text-primary);
  outline: none;
}
.lts-field input:focus {
  border-color: var(--accent);
}
.lts-path-input {
  display: flex;
  align-items: center;
  gap: 6px;
}
.lts-path-input input {
  flex: 1;
  min-width: 0;
  font-family: 'Cascadia Mono', Consolas, monospace;
}
</style>
