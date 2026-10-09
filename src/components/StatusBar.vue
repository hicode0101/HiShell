<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useSessionsStore } from '../stores/sessions'
import { useTransfersStore } from '../stores/transfers'
import { useMcpServerStore } from '../stores/mcpServer'
import { t } from '../i18n'

/* 大小写 / 数字键锁定状态：按键事件里读取（未按键时保持上次状态，默认熄灭） */
const capsOn = ref(false)
const numOn = ref(false)

function onKeyState(e: KeyboardEvent) {
  capsOn.value = e.getModifierState('CapsLock')
  numOn.value = e.getModifierState('NumLock')
}

let removeKeyState: (() => void) | undefined = undefined
onMounted(() => {
  window.addEventListener('keydown', onKeyState)
  window.addEventListener('keyup', onKeyState)
  removeKeyState = () => {
    window.removeEventListener('keydown', onKeyState)
    window.removeEventListener('keyup', onKeyState)
  }
})
onUnmounted(() => removeKeyState?.())

/* 锁定键提示随当前状态变化，方便用户了解开启/关闭 */
const capsTitle = computed(() => t(capsOn.value ? 'statusbar.capsLockOn' : 'statusbar.capsLockOff'))
const numTitle = computed(() => t(numOn.value ? 'statusbar.numLockOn' : 'statusbar.numLockOff'))

const sessions = useSessionsStore()
const transfers = useTransfersStore()
const mcpServer = useMcpServerStore()
onMounted(() => mcpServer.load())

const address = computed(() => {
  const tab = sessions.activeTab
  if (!tab) return ''
  if (tab.kind === 'local') return t('statusbar.localShell')
  if (tab.kind !== 'session') return ''
  return tab.address || '未连接'
})
const statusText = computed(() => {
  const tab = sessions.activeTab
  if (!tab || tab.kind !== 'session') return ''
  return tab.status === 'connected' ? 'SSH2' : tab.status === 'connecting' ? t('statusbar.connecting') : t('statusbar.notConnected')
})

/* MCP 服务状态灯：与 CAP/NUM 同一套配色，运行中绿色、关闭灰色；异常原因放悬停提示 */
const mcpOn = computed(() => mcpServer.running)
const mcpTitle = computed(() => {
  if (mcpServer.running) return t('statusbar.mcpOn', { host: mcpServer.listenHost, port: mcpServer.port })
  if (mcpServer.enabled && mcpServer.error) return t('statusbar.mcpError', { error: mcpServer.error })
  return t('statusbar.mcpOff')
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
    <div class="st-section" v-if="sessions.activeTab && sessions.activeTab.kind !== 'settings'">xterm</div>
    <div class="st-section" v-if="sessions.activeTab && sessions.activeTab.kind !== 'settings' && sessions.activeTab.cols">
      {{ sessions.activeTab.cols }}×{{ sessions.activeTab.rows }}
    </div>
    <div class="st-section" v-if="transfers.runningCount > 0">
      <span class="transfer-indicator" />
      {{ t('statusbar.transfers', { n: transfers.runningCount }) }}
    </div>
    <div class="st-spacer" />
    <div class="st-section" :title="t('statusbar.tabCountTip')">{{ t('statusbar.tabCount', { n: sessions.tabs.length }) }}</div>
    <div class="st-section chip" :title="t('statusbar.historyNav')">↑↓</div>
    <div class="st-section chip lock" :class="{ on: mcpOn }" :title="mcpTitle">MCP</div>
    <div class="st-section chip lock" :class="{ on: capsOn }" :title="capsTitle">CAP</div>
    <div class="st-section chip lock" :class="{ on: numOn }" :title="numTitle">NUM</div>
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
.chip.lock {
  color: var(--text-tertiary);
  opacity: 0.55;
}
.chip.lock.on {
  color: #2fd66f;
  opacity: 1;
  font-weight: 700;
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
