<script setup lang="ts">
/**
 * 会话管理器：列出全部连接配置，支持 连接 / 编辑 / 删除 / 新建。
 * 删除通过确认对话框二次确认，避免误删。
 */
import { computed, onUnmounted, ref } from 'vue'
import { FButton, FConfirmDialog, FDialog, toast } from '../ui'
import { useProfilesStore } from '../stores/profiles'
import { profileCopy } from '../api/backend'
import { useSessionsStore } from '../stores/sessions'
import type { SshProfile } from '../api/types'
import { t } from '../i18n'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const profiles = useProfilesStore()
const sessions = useSessionsStore()
const deleteTarget = ref<SshProfile | null>(null)

/* ---------- 行拖拽排序（行首把手，鼠标模拟；顺序经 profile_reorder 持久化） ---------- */
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
  const rowEls = [...listEl.value.querySelectorAll('.sm-row')]
  const hover = rowEls.findIndex((r) => {
    const b = r.getBoundingClientRect()
    return e.clientY >= b.top && e.clientY <= b.bottom
  })
  if (hover < 0 || hover === gripIndex.value) return
  profiles.moveRow(gripIndex.value, hover)
  gripIndex.value = hover
  gripMoved = true
}

async function onGripUp() {
  document.removeEventListener('mousemove', onGripMove)
  document.removeEventListener('mouseup', onGripUp)
  document.body.style.userSelect = ''
  gripDragging.value = false
  if (gripMoved) await profiles.persistOrder()
  gripIndex.value = -1
  gripMoved = false
}

onUnmounted(() => {
  document.removeEventListener('mousemove', onGripMove)
  document.removeEventListener('mouseup', onGripUp)
  document.body.style.userSelect = ''
})


const rows = computed(() =>
  profiles.profiles.map((p) => ({
    profile: p,
    addr: `${p.username}@${p.host}:${p.port}`,
    proxy: proxyLabel(p),
  })),
)

function proxyLabel(p: SshProfile): string {
  switch (p.proxy.kind) {
    case 'socks5':
      return `SOCKS5 ${p.proxy.host}:${p.proxy.port}`
    case 'http':
      return `HTTP ${p.proxy.host}:${p.proxy.port}`
    case 'jump':
      return t('sessionToolbar.jump', { name: profiles.byId(p.proxy.profileId)?.name ?? '?' })
    case 'proxyCommand':
      return t('sessionToolbar.proxyCommand')
    default:
      return ''
  }
}

function connect(p: SshProfile) {
  sessions.createSession(JSON.parse(JSON.stringify(p)))
  emit('update:open', false)
}

function edit(p: SshProfile) {
  // 保持会话管理器打开：连接对话框叠加在上层，保存/取消后回到列表并自动刷新
  sessions.openConnectDialog(p.id)
}

function remove(p: SshProfile) {
  deleteTarget.value = p
}

/** 复制配置：生成一份带凭据的副本（名称自动去重），不发起连接 */
async function copyConfig(p: SshProfile) {
  try {
    const saved = await profileCopy(p.id, p.name + t('sessions.copySuffix'))
    profiles.pushLoaded(saved)
    toast(t('sessions.copiedToast', { name: saved.name }), 'success')
  } catch (e) {
    toast(String(e), 'error')
  }
}

async function confirmRemove() {
  const p = deleteTarget.value
  if (!p) return
  await profiles.remove(p.id)
  toast(t('sessions.deletedToast', { name: p.name }), 'success')
  deleteTarget.value = null
}
</script>

<template>
  <FDialog :open="open" :title="t('sessions.title')" :width="620" @update:open="emit('update:open', $event)">
    <div v-if="!rows.length" class="sm-empty">{{ t('sessions.empty') }}</div>
    <div v-else ref="listEl" class="sm-list" :class="{ 'grip-dragging': gripDragging }">
      <div
        v-for="(row, idx) in rows"
        :key="row.profile.id"
        class="sm-row"
        :class="{ 'dragging-row': gripDragging && gripIndex === idx }"
      >
        <span class="sm-grip" :title="t('sessions.dragSort')" @mousedown="onGripMouseDown($event, idx)">
          <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor">
            <circle cx="2.6" cy="3" r="1.4" /><circle cx="7.4" cy="3" r="1.4" />
            <circle cx="2.6" cy="8" r="1.4" /><circle cx="7.4" cy="8" r="1.4" />
            <circle cx="2.6" cy="13" r="1.4" /><circle cx="7.4" cy="13" r="1.4" />
          </svg>
        </span>
        <div class="sm-main">
          <div class="sm-name">
            {{ row.profile.name }}
            <span v-if="row.proxy" class="sm-proxy">{{ row.proxy }}</span>
          </div>
          <div class="sm-addr">{{ row.addr }} · {{ t(`connect.auth.${row.profile.auth === 'keyboardInteractive' ? 'ki' : row.profile.auth === 'priFileKey' ? 'privateKey' : row.profile.auth === 'agent' ? 'agent' : 'password'}`) }}</div>
        </div>
        <div class="sm-ops">
          <button class="sm-ic-btn connect" :title="t('common.connect')" @click="connect(row.profile)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M7.5 3v3.6M12.5 3v3.6M5.4 6.6h9.2v3.2a4.6 4.6 0 0 1-9.2 0V6.6zM10 14.4v2.8" />
            </svg>
          </button>
          <button class="sm-ic-btn edit" :title="t('common.edit')" @click="edit(row.profile)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M13.1 3.6l3.3 3.3L7 16.3l-4 1 1-4 9.1-9.7zM11.7 5l3.3 3.3" />
            </svg>
          </button>
          <button class="sm-ic-btn copy" :title="t('sessions.copy')" @click="copyConfig(row.profile)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <rect x="7" y="7" width="9" height="9" rx="1.6" />
              <path d="M12.6 4.5H5.6A1.6 1.6 0 0 0 4 6.1v7" />
            </svg>
          </button>
          <button class="sm-ic-btn del" :title="t('common.delete')" @click="remove(row.profile)">
            <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M4.5 5.5h11M8 5.5V4.3c0-.44.36-.8.8-.8h2.4c.44 0 .8.36.8.8v1.2M6 5.5l.7 9.8c.04.66.6 1.2 1.27 1.2h4.06c.66 0 1.23-.54 1.27-1.2l.7-9.8M8.4 8.5v5M11.6 8.5v5" />
            </svg>
          </button>
        </div>
      </div>
    </div>
    <template #footer>
      <FButton @click="sessions.openConnectDialog()">
        {{ t('common.newConnection') }}
      </FButton>
      <div class="sm-spacer" />
      <FButton variant="subtle" @click="emit('update:open', false)">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>

  <!-- 删除连接确认对话框 -->
  <FConfirmDialog
    :open="!!deleteTarget"
    :title="t('common.delete')"
    :message="t('sessions.confirmDelete', { name: deleteTarget?.name ?? '' })"
    @update:open="(v: boolean) => !v && (deleteTarget = null)"
    @confirm="confirmRemove"
  />
</template>

<style scoped>
.sm-empty {
  padding: 30px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
}
.sm-list {
  max-height: 380px;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.sm-list.grip-dragging {
  cursor: grabbing;
}
.sm-row.dragging-row {
  background: var(--bg-selected);
}
/* 行首拖拽把手（六点 grip） */
.sm-grip {
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
.sm-grip:hover {
  color: var(--text-secondary);
  background: var(--bg-alt-hover);
}
.sm-grip:active {
  cursor: grabbing;
}
.sm-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 10px;
  border-radius: var(--radius-md);
}
.sm-row:hover {
  background: var(--bg-alt-hover);
}
.sm-main {
  flex: 1;
  min-width: 0;
}
.sm-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 8px;
}
.sm-proxy {
  font-size: 10.5px;
  font-weight: 500;
  color: var(--accent);
  background: var(--accent-light);
  border-radius: 9px;
  padding: 1px 8px;
}
.sm-addr {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  margin-top: 3px;
}
.sm-ops {
  display: flex;
  gap: 6px;
  flex: none;
}
.sm-ic-btn {
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
.sm-ic-btn:hover {
  background: var(--bg-alt-hover);
}
.sm-ic-btn.connect {
  background: var(--accent);
  color: #fff;
}
.sm-ic-btn.connect:hover {
  background: var(--accent);
  filter: brightness(1.12);
}
.sm-ic-btn.edit:hover {
  color: var(--warning);
}
.sm-ic-btn.copy {
  color: var(--success);
}
.sm-ic-btn.del {
  color: var(--danger);
}
.sm-ic-btn.del:hover {
  background: rgba(209, 52, 56, 0.12);
}
.sm-spacer {
  flex: 1;
}
</style>
