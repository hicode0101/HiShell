<script setup lang="ts">
import { computed } from 'vue'
import { useSessionsStore } from '../stores/sessions'
import { useTransfersStore } from '../stores/transfers'
import { t } from '../i18n'

const sessions = useSessionsStore()
const transfers = useTransfersStore()

const address = computed(() => {
  const tab = sessions.activeTab
  if (!tab || tab.kind !== 'session') return ''
  return tab.address || '未连接'
})
const statusText = computed(() => {
  const tab = sessions.activeTab
  if (!tab || tab.kind !== 'session') return ''
  return tab.status === 'connected' ? 'SSH2' : tab.status === 'connecting' ? t('statusbar.connecting') : t('statusbar.notConnected')
})
</script>

<template>
  <div class="statusbar">
    <div class="st-section address" :title="address">
      <svg width="12" height="12" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
        <rect x="5" y="9" width="10" height="7.5" rx="1.5" /><path d="M7.5 9V7a2.5 2.5 0 0 1 5 0v2" />
      </svg>
      <span>{{ address || t('statusbar.ready') }}</span>
    </div>
    <div class="st-section" v-if="sessions.activeTab?.kind === 'session'">{{ statusText }}</div>
    <div class="st-section" v-if="sessions.activeTab?.kind === 'session'">xterm</div>
    <div class="st-section" v-if="sessions.activeTab?.kind === 'session' && sessions.activeTab.cols">
      {{ sessions.activeTab.cols }}×{{ sessions.activeTab.rows }}
    </div>
    <div class="st-section" v-if="transfers.runningCount > 0">
      <span class="transfer-indicator" />
      {{ t('statusbar.transfers', { n: transfers.runningCount }) }}
    </div>
    <div class="st-spacer" />
    <div class="st-section">{{ t('statusbar.tabCount', { n: sessions.tabs.length }) }}</div>
    <div class="st-section chip">↑↓</div>
    <div class="st-section chip">CAP</div>
    <div class="st-section chip">NUM</div>
  </div>
</template>

<style scoped>
.statusbar {
  display: flex;
  align-items: center;
  height: 26px;
  background: var(--bg-layer);
  border-top: 1px solid var(--stroke-card);
  font-size: 11.5px;
  color: var(--text-secondary);
  flex: none;
  padding: 0 4px;
  gap: 2px;
}
.st-section {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 10px;
  height: 100%;
  white-space: nowrap;
}
.st-section.address {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.st-spacer {
  flex: 1;
}
.chip {
  border-left: 1px solid var(--stroke-divider);
}
.transfer-indicator {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1s ease-in-out infinite;
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
</style>
