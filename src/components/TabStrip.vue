<script setup lang="ts">
import { ref } from 'vue'
import { useSessionsStore } from '../stores/sessions'
import { useProfilesStore } from '../stores/profiles'
import { showContextMenu } from '../ui'
import { t } from '../i18n'
import type { SessionTab } from '../api/types'

const sessions = useSessionsStore()
const profiles = useProfilesStore()

/* ---------- 标签拖拽排序（鼠标模拟）----------
 * dragDropEnabled:true 后 WebView2 拦截了 HTML5 拖拽（含标签页内部拖动），
 * 改用 mousedown + mousemove 自行实现，复用原有的插入位置指示线视觉。 */
const dragId = ref('')
const dropTargetId = ref('')
const dropBefore = ref(false)
let pressId = ''
let pressX = 0
let pressY = 0
let tabDragging = false
let suppressClick = false

function onTabMouseDown(e: MouseEvent, tab: SessionTab) {
  if (e.button !== 0) return
  if ((e.target as HTMLElement).closest('.tab-close')) return
  pressId = tab.id
  pressX = e.clientX
  pressY = e.clientY
  tabDragging = false
  document.addEventListener('mousemove', onTabDragMove)
  document.addEventListener('mouseup', onTabDragUp)
}

function onTabDragMove(e: MouseEvent) {
  if (!pressId) return
  if (!tabDragging) {
    if (Math.abs(e.clientX - pressX) + Math.abs(e.clientY - pressY) < 5) return
    tabDragging = true
    dragId.value = pressId
    document.body.style.userSelect = 'none'
  }
  // 光标落在哪个标签上 → 显示插入位置指示线
  const hit = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest('.tab') as HTMLElement | null
  const targetId = hit?.dataset.tabId ?? ''
  if (!targetId || targetId === dragId.value) {
    dropTargetId.value = ''
    return
  }
  const rect = hit!.getBoundingClientRect()
  dropBefore.value = e.clientX < rect.left + rect.width / 2
  dropTargetId.value = targetId
}

function onTabDragUp(e: MouseEvent) {
  document.removeEventListener('mousemove', onTabDragMove)
  document.removeEventListener('mouseup', onTabDragUp)
  document.body.style.userSelect = ''
  const wasDragging = tabDragging
  tabDragging = false
  if (wasDragging) {
    // 只吞掉 mouseup 后紧随的那次 click，避免残留标志吞掉下一次标签点击
    suppressClick = true
    setTimeout(() => {
      suppressClick = false
    }, 0)
    if (dragId.value && dropTargetId.value && dropTargetId.value !== dragId.value) {
      sessions.moveTab(dragId.value, dropTargetId.value, dropBefore.value)
    }
  }
  pressId = ''
  dragId.value = ''
  dropTargetId.value = ''
  dropBefore.value = false
}

function onTabClick(tab: SessionTab) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  sessions.activate(tab.id)
}

function emit(name: string) {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: name }))
}

function tabContextMenu(e: MouseEvent, tab: SessionTab) {
  showContextMenu(e, [
    { label: t('tabs.rename'), icon: 'edit', onClick: () => emit('rename-tab:' + tab.id) },
    ...(tab.kind === 'session' && tab.profileId
      ? [
          {
            label: t('tabs.duplicate'),
            icon: 'copy',
            onClick: () => duplicateTab(tab),
          },
        ]
      : []),
    ...(tab.kind === 'session' && tab.sid !== undefined && tab.status === 'connected'
      ? [
          {
            label: t('tabs.duplicateChannel'),
            icon: 'network',
            onClick: () => sessions.createAliasSession(tab.id),
          },
        ]
      : []),
    { separator: true },
    { label: t('tabs.close'), shortcut: 'Ctrl+W', icon: 'close', onClick: () => sessions.closeTab(tab.id) },
    { label: t('tabs.closeOthers'), onClick: () => sessions.closeOthers(tab.id) },
    { label: t('tabs.closeRight'), onClick: () => sessions.closeToRight(tab.id) },
  ])
}

/** 复制会话：再开一个同服务器的新连接（复用同一配置与已存凭据，无需重新输入密码） */
function duplicateTab(tab: SessionTab) {
  if (tab.kind !== 'session' || !tab.profileId) return
  const p = profiles.byId(tab.profileId)
  if (!p) return
  sessions.createSession(JSON.parse(JSON.stringify(p)))
}

function onStripContext(e: MouseEvent) {
  showContextMenu(e, [
    { label: t('common.newConnection'), icon: 'plus', onClick: () => emit('new-connection') },
    // 「新建 SFTP」暂时隐藏（功能后续版本恢复）
    // { label: t('common.newSftp'), icon: 'folder', onClick: () => emit('new-sftp') },
  ])
}

/** 标签顶线颜色跟随连接类型（与工具栏图标一致） */
function tabAccent(tab: SessionTab): string {
  switch (tab.kind) {
    case 'local':
      return 'var(--type-local)'
    case 'session':
      return 'var(--type-ssh)'
    default:
      return 'var(--accent)'
  }
}
</script>

<template>
  <div class="tabstrip" @contextmenu="onStripContext" @auxclick.middle="sessions.closeTab(sessions.activeTabId)">
    <TransitionGroup name="tab">
      <div
        v-for="tab in sessions.tabs"
        :key="tab.id"
        class="tab"
        :class="{
          active: tab.id === sessions.activeTabId,
          settings: tab.kind === 'settings',
          dragging: dragId === tab.id,
          'drop-before': dropTargetId === tab.id && dropBefore,
          'drop-after': dropTargetId === tab.id && !dropBefore,
        }"
        :style="{ '--tab-accent': tabAccent(tab) }"
        :title="tab.address || tab.title"
        :data-tab-id="tab.id"
        @click="onTabClick(tab)"
        @auxclick.middle.stop="sessions.closeTab(tab.id)"
        @contextmenu.stop="tabContextMenu($event, tab)"
        @mousedown="onTabMouseDown($event, tab)"
      >
        <span v-if="tab.kind !== 'settings'" class="dot" :class="tab.status" />
        <svg v-else class="dot gear" width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <circle cx="10" cy="10" r="2.1" />
          <path d="M10 3.2v2M10 14.8v2M16.8 10h-2M5.2 10h-2M14.65 5.35l-1.42 1.42M6.77 13.23l-1.42 1.42M14.65 14.65l-1.42-1.42M6.77 6.77L5.35 5.35" />
        </svg>
        <span class="tab-title">{{ tab.kind === 'settings' ? t('tabs.settings') : tab.title }}</span>
        <button class="tab-close" :title="t('tabs.close')" @click.stop="sessions.closeTab(tab.id)">
          <svg width="8" height="8" viewBox="0 0 8 8"><path d="M1 1l6 6M7 1L1 7" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" /></svg>
        </button>
      </div>
    </TransitionGroup>

    <button class="tab-new" :title="t('common.newConnection')" @click="emit('new-connection')">
      <svg width="14" height="14" viewBox="0 0 14 14"><path d="M7 2v10M2 7h10" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
    </button>
    <div class="strip-line" />
  </div>
</template>

<style scoped>
.tabstrip {
  display: flex;
  align-items: flex-end;
  height: 38px;
  padding: 0 6px;
  gap: 2px;
  flex: none;
  position: relative;
}
.tab {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 8px 0 12px;
  max-width: 220px;
  min-width: 110px;
  border-radius: 7px 7px 0 0;
  cursor: pointer;
  color: var(--text-secondary);
  position: relative;
  transition: background 0.12s ease;
}
.tab:hover {
  background: var(--bg-alt-hover);
}
.tab.active {
  background: var(--bg-layer);
  color: var(--text-primary);
  box-shadow: inset 0 1px 0 var(--stroke-card), 0 -1px 0 var(--stroke-card), 1px 0 0 var(--stroke-card), -1px 0 0 var(--stroke-card);
}
.tab.active::after {
  content: '';
  position: absolute;
  top: 0;
  left: 12px;
  right: 12px;
  height: 2.5px;
  border-radius: 2px;
  background: var(--tab-accent, var(--accent));
}
/* 拖拽排序反馈 */
.tab.dragging {
  opacity: 0.4;
}
.tab.drop-before {
  box-shadow: -3px 0 0 0 var(--tab-accent, var(--accent));
}
.tab.drop-after {
  box-shadow: 3px 0 0 0 var(--tab-accent, var(--accent));
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
  background: var(--text-tertiary);
}
.dot.connected {
  background: #2ecc71;
  box-shadow: 0 0 4px rgba(46, 204, 113, 0.7);
}
.dot.connecting {
  background: #f1c40f;
  animation: pulse 1s ease-in-out infinite;
}
@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}
.dot.gear {
  background: none;
  color: var(--text-secondary);
}
.tab-title {
  flex: 1;
  font-size: 12.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tab-close {
  border: none;
  background: transparent;
  width: 18px;
  height: 18px;
  border-radius: 4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary);
  cursor: pointer;
  flex: none;
  opacity: 0.7;
}
.tab:hover .tab-close,
.tab.active .tab-close {
  opacity: 1;
}
.tab-close:hover {
  background: var(--bg-alt-pressed);
  color: var(--text-primary);
}
.tab-new {
  width: 28px;
  height: 28px;
  margin-bottom: 3px;
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
.tab-new:hover {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.strip-line {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 1px;
  background: var(--stroke-divider);
}
.tab-enter-from,
.tab-leave-to {
  opacity: 0;
  transform: translateX(8px);
}
.tab-leave-active {
  transition: all 0.12s ease;
}
</style>
