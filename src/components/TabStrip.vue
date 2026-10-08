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
 * 改用 mousedown + mousemove 自行实现。
 * 参考主流终端/浏览器：按住进入拖动态后光标切换为移动图标，
 * 拖动过程中实时换位——光标压到某个标签上即把被拖标签插到它的
 * 靠近来向一侧（从左往右压到即插其右、从右往左压到即插其左），
 * 无需等松开鼠标。 */
const dragId = ref('')
const stripEl = ref<HTMLElement | null>(null)
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
    document.body.classList.add('tab-dragging')
  }
  const strip = stripEl.value
  if (!strip) return
  // 用 offsetLeft/offsetWidth（布局位置，不受 FLIP 滑动动画的 transform 影响）做命中判定，避免来回抖动
  const x = e.clientX - strip.getBoundingClientRect().left
  const els = Array.from(strip.querySelectorAll<HTMLElement>('.tab'))
  const from = els.findIndex((el) => el.dataset.tabId === dragId.value)
  if (from < 0) return
  let index = 0
  for (let i = 0; i < els.length; i++) {
    if (i === from) continue
    const left = els[i].offsetLeft
    const right = left + els[i].offsetWidth
    if (x > left && x < right) {
      // 压在标签 i 上 → 移到它的位置（自动落在靠近来向的一侧）
      index = i
      break
    }
    if (right <= x) index = i < from ? i + 1 : i
  }
  if (index !== from) sessions.moveTabToIndex(dragId.value, index)
}

function onTabDragUp() {
  document.removeEventListener('mousemove', onTabDragMove)
  document.removeEventListener('mouseup', onTabDragUp)
  document.body.style.userSelect = ''
  document.body.classList.remove('tab-dragging')
  if (tabDragging) {
    // 只吞掉 mouseup 后紧随的那次 click，避免残留标志吞掉下一次标签点击
    suppressClick = true
    setTimeout(() => {
      suppressClick = false
    }, 0)
  }
  tabDragging = false
  pressId = ''
  dragId.value = ''
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
  <div ref="stripEl" class="tabstrip" @contextmenu="onStripContext" @auxclick.middle="sessions.closeTab(sessions.activeTabId)">
    <TransitionGroup name="tab">
      <div
        v-for="tab in sessions.tabs"
        :key="tab.id"
        class="tab"
        :class="{
          active: tab.id === sessions.activeTabId,
          settings: tab.kind === 'settings',
          dragging: dragId === tab.id,
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
  transition: background 0.12s ease, box-shadow 0.12s ease, opacity 0.12s ease;
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
/* 拖拽排序反馈：被拖标签“抬起”，其余标签实时让位（配合 .tab-move 滑动动画） */
.tab.dragging {
  opacity: 0.88;
  z-index: 5;
  box-shadow: 0 3px 14px rgba(0, 0, 0, 0.35);
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
/* 实时换位时其余标签的滑动动画 */
.tab-move {
  transition: transform 0.16s cubic-bezier(0.22, 1, 0.36, 1);
}
</style>

<style>
/* 拖动标签期间全局光标切换为“移动”十字图标（body 不受 scoped 约束，需非 scoped 块） */
body.tab-dragging,
body.tab-dragging * {
  cursor: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24'%3E%3Cpath d='M12 3v18M3 12h18M9 6l3-3l3 3M9 18l3 3l3-3M6 9l-3 3l3 3M18 9l3 3l-3 3' fill='none' stroke='%23ffffff' stroke-width='3.5' stroke-linecap='round' stroke-linejoin='round'/%3E%3Cpath d='M12 3v18M3 12h18M9 6l3-3l3 3M9 18l3 3l3-3M6 9l-3 3l3 3M18 9l3 3l-3 3' fill='none' stroke='%230067c0' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E")
      12 12,
    move !important;
}
</style>
