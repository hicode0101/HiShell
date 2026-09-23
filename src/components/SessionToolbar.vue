<script setup lang="ts">
/**
 * 选项卡内功能区：视图切换（终端/文件）+ 会话操作 + 连接信息徽标。
 * 每个会话选项卡内容区顶部一行，参考 Xshell 每会话工具行。
 */
import { computed } from 'vue'
import { t } from '../i18n'
import type { SessionTab } from '../api/types'
import { useProfilesStore } from '../stores/profiles'

const props = defineProps<{ tab: SessionTab }>()

const profiles = useProfilesStore()

function cmd(name: string, payload?: string) {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: payload ? `${name}:${payload}` : name }))
}
function tabCmd(cmdName: string) {
  window.dispatchEvent(new CustomEvent('hishell-tab-command', { detail: { tabId: props.tab.id, cmd: cmdName } }))
}

const isSession = computed(() => props.tab.kind === 'session')

/** 会话配置中存在端口转发规则 → 「端口」按钮高亮提示 */
const forwardCount = computed(() => {
  const p = props.tab.profileId ? profiles.byId(props.tab.profileId) : undefined
  return p?.forwardedPorts?.length ?? 0
})
const hasForwards = computed(() => forwardCount.value > 0)

const proxyBadge = computed(() => {
  const p = props.tab.profileId ? profiles.byId(props.tab.profileId) : undefined
  if (!p) return null
  switch (p.proxy.kind) {
    case 'socks5':
      return { icon: 'shield', text: `SOCKS5 ${p.proxy.host}:${p.proxy.port}` }
    case 'http':
      return { icon: 'shield', text: `HTTP ${p.proxy.host}:${p.proxy.port}` }
    case 'jump': {
      const j = profiles.byId(p.proxy.profileId)
      return { icon: 'link', text: t('sessionToolbar.jump', { name: j?.name ?? '?' }) }
    }
    case 'proxyCommand':
      return { icon: 'terminal', text: t('sessionToolbar.proxyCommand') }
    default:
      return { icon: 'check', text: t('sessionToolbar.direct') }
  }
})
</script>

<template>
  <div class="session-toolbar" v-if="tab.kind !== 'settings'">
    <template v-if="isSession">
      <div class="view-switch">
        <button
          class="vs-btn"
          :class="{ on: tab.viewMode === 'terminal' }"
          :title="t('common.terminal')"
          @click="cmd('view-terminal')"
        >
          <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
            <rect x="3" y="4.5" width="14" height="11" rx="1.5" /><path d="M6.5 8l2.8 2.3-2.8 2.3M11.2 13h3" />
          </svg>
          {{ t('sessionToolbar.terminal') }}
        </button>
        <button
          class="vs-btn"
          :class="{ on: tab.viewMode === 'sftp' }"
          :title="t('sessionToolbar.fileViewShortcut')"
          @click="cmd('view-sftp')"
        >
          <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 6.3c0-.7.6-1.3 1.3-1.3h3l1.8 1.8h5.6c.7 0 1.3.6 1.3 1.3v7.6c0 .7-.6 1.3-1.3 1.3H4.8c-.7 0-1.3-.6-1.3-1.3V6.3z" />
          </svg>
          {{ t('sessionToolbar.files') }}
        </button>
      </div>

      <div class="st-divider" />

      <button
        class="st-btn"
        :class="{ 'has-forwards': hasForwards }"
        :title="hasForwards ? t('sessionToolbar.hasForwardsTip') : t('ports.dialogTitle')"
        @click="cmd('ports-manager')"
      >
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <path d="M8 8.5V4M12 8.5V4" />
          <path d="M5.5 8.5h9v2.2a4.5 4.5 0 0 1-9 0V8.5z" />
          <path d="M10 15.2v1.6" />
        </svg>
        {{ t('ports.title') }}
        <span v-if="hasForwards" class="fw-count">{{ forwardCount }}</span>
      </button>
      <button class="st-btn" :title="t('common.reconnect')" :disabled="tab.status === 'connected'" @click="cmd('reconnect')">
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <path d="M8.3 11.7l3.4-3.4M8.8 4.9l1.7-1.7a3.3 3.3 0 0 1 4.7 4.7l-1.7 1.7M11.2 15.1l-1.7 1.7a3.3 3.3 0 0 1-4.7-4.7l1.7-1.7" />
        </svg>
        {{ t('sessionToolbar.connect') }}
      </button>
      <button class="st-btn" :title="t('sessionToolbar.disconnect')" :disabled="tab.status === 'disconnected'" @click="cmd('disconnect')">
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <path d="M10 3.5V10M5.9 6.2a6 6 0 1 0 8.2 0" />
        </svg>
        {{ t('common.disconnect') }}
      </button>
    </template>

    <template v-if="tab.viewMode === 'terminal'">
      <div v-if="isSession" class="st-divider" />
      <button class="st-btn" :title="t('sessionToolbar.clearShortcut')" @click="tabCmd('clear')">
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 5h12l-4.6 5.3v4.2L8.6 16v-5.7L4 5z" />
        </svg>
        {{ t('sessionToolbar.clear') }}
      </button>
      <button class="st-btn" :title="t('terminal.search')" @click="tabCmd('search')">
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
          <circle cx="9" cy="9" r="4.75" /><path d="M12.5 12.5l4 4" />
        </svg>
        {{ t('sessionToolbar.search') }}
      </button>
    </template>

    <div class="st-spacer" />

    <div v-if="isSession && proxyBadge" class="proxy-badge" :title="t('sessionToolbar.proxyTooltip', { mode: proxyBadge.text })">
      <svg width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <template v-if="proxyBadge.icon === 'shield'"><path d="M10 3l6 2.2v4.9c0 3.9-2.5 6.4-6 7.9-3.5-1.5-6-4-6-7.9V5.2L10 3z" /></template>
        <template v-else-if="proxyBadge.icon === 'link'"><path d="M8.3 11.7l3.4-3.4M8.8 4.9l1.7-1.7a3.3 3.3 0 0 1 4.7 4.7l-1.7 1.7M11.2 15.1l-1.7 1.7a3.3 3.3 0 0 1-4.7-4.7l1.7-1.7" /></template>
        <template v-else-if="proxyBadge.icon === 'terminal'"><rect x="3" y="4.5" width="14" height="11" rx="1.5" /><path d="M6.5 8l2.8 2.3-2.8 2.3M11.2 13h3" /></template>
        <template v-else><path d="M4.5 10.5l3.5 3.5 7.5-8.5" /></template>
      </svg>
      {{ proxyBadge.text }}
    </div>
    <div class="addr-badge" :title="tab.address">{{ tab.kind === 'local' ? t('sessionToolbar.localBadge') : tab.address }}</div>
  </div>
</template>

<style scoped>
.session-toolbar {
  display: flex;
  align-items: center;
  gap: 3px;
  height: 38px;
  padding: 0 10px;
  flex: none;
}
.view-switch {
  display: flex;
  background: var(--bg-alt-pressed);
  border-radius: var(--radius-md);
  padding: 2px;
  gap: 2px;
}
.vs-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 12px;
  border: none;
  background: transparent;
  border-radius: 5px;
  font: inherit;
  font-size: 12.5px;
  color: var(--text-secondary);
  cursor: pointer;
  white-space: nowrap;
}
.vs-btn.on {
  background: var(--bg-card);
  color: var(--text-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
  font-weight: 600;
}
.st-divider {
  width: 1px;
  height: 18px;
  background: var(--stroke-divider);
  margin: 0 6px;
  flex: none;
}
.st-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 9px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  font: inherit;
  font-size: 12.5px;
  color: var(--text-secondary);
  cursor: pointer;
  white-space: nowrap;
}
.st-btn:hover:not(:disabled) {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.st-btn:disabled {
  color: var(--text-disabled);
  cursor: default;
}
/* 配置了端口转发规则：端口按钮高亮，一眼可见 */
.st-btn.has-forwards {
  background: var(--accent-light);
  color: var(--accent);
  font-weight: 600;
}
.st-btn.has-forwards:hover {
  background: var(--accent-light);
  color: var(--accent);
}
.fw-count {
  background: var(--accent);
  color: var(--accent-text-on);
  border-radius: 9px;
  font-size: 10.5px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
}
.st-spacer {
  flex: 1;
}
.proxy-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 9px;
  border-radius: 11px;
  background: var(--accent-light);
  color: var(--accent);
  font-size: 11.5px;
  white-space: nowrap;
  flex: none;
}
.addr-badge {
  font-size: 11.5px;
  color: var(--text-tertiary);
  margin-left: 10px;
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
