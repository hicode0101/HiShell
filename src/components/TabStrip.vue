<script setup lang="ts">
import { useSessionsStore } from '../stores/sessions'
import { showContextMenu } from '../ui'
import { t } from '../i18n'
import type { SessionTab } from '../api/types'

const sessions = useSessionsStore()

function emit(name: string) {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: name }))
}

function tabContextMenu(e: MouseEvent, tab: SessionTab) {
  showContextMenu(e, [
    { label: t('tabs.rename'), icon: 'edit', onClick: () => emit('rename-tab:' + tab.id) },
    { label: t('tabs.copyAddress'), icon: 'copy', onClick: () => navigator.clipboard?.writeText(tab.address) },
    { separator: true },
    { label: t('tabs.close'), shortcut: 'Ctrl+W', icon: 'close', onClick: () => sessions.closeTab(tab.id) },
    { label: t('tabs.closeOthers'), onClick: () => sessions.closeOthers(tab.id) },
    { label: t('tabs.closeRight'), onClick: () => sessions.closeToRight(tab.id) },
  ])
}

function onStripContext(e: MouseEvent) {
  showContextMenu(e, [
    { label: t('common.newConnection'), icon: 'plus', onClick: () => emit('new-connection') },
    { label: t('common.newSftp'), icon: 'folder', onClick: () => emit('new-sftp') },
  ])
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
        }"
        :title="tab.address || tab.title"
        @click="sessions.activate(tab.id)"
        @auxclick.middle.stop="sessions.closeTab(tab.id)"
        @contextmenu.stop="tabContextMenu($event, tab)"
      >
        <span v-if="tab.kind === 'session'" class="dot" :class="tab.status" />
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
  background: var(--accent);
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
