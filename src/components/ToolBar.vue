<script setup lang="ts">
import { ref } from 'vue'
import { useSessionsStore } from '../stores/sessions'
import { parseQuickConnect } from '../stores/profiles'
import { toast } from '../ui'
import { t } from '../i18n'

const sessions = useSessionsStore()
const quickInput = ref('')

function emit(name: string) {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: name }))
}

function quickConnect() {
  const info = parseQuickConnect(quickInput.value)
  if (!info) {
    toast(t('toolbar.parseError'), 'error')
    return
  }
  emit('quick-connect:' + JSON.stringify(info))
  quickInput.value = ''
}
</script>

<template>
  <div class="toolbar">
    <!-- 新建本地终端 -->
    <button class="tb-btn" :title="t('toolbar.newLocalTerminal')" @click="emit('new-local')">
      <svg class="ic-local" width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <rect x="3" y="4.5" width="14" height="11" rx="1.5" /><path d="M6.5 8l2.8 2.3-2.8 2.3M11.2 13h3" />
      </svg>
    </button>
    <!-- 新建 SSH 连接 -->
    <button class="tb-btn" :title="t('toolbar.newConnection')" @click="emit('new-connection')">
      <svg class="ic-ssh" width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <rect x="3" y="4.5" width="14" height="11" rx="1.5" /><path d="M6.5 8l2.8 2.3-2.8 2.3M11.2 13h3" /><path d="M16.5 13.5v5M14 16h5" stroke-width="1.5" />
      </svg>
    </button>
    <!-- 新建 Serial 终端 -->
    <button class="tb-btn" :title="t('toolbar.newSerial')" @click="emit('new-serial')">
      <svg class="ic-serial" width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M6.2 4.5h7.6l2 6.4a1.7 1.7 0 0 1-1.6 2.2H5.8a1.7 1.7 0 0 1-1.6-2.2l2-6.4z" />
        <path d="M7.4 7.6h.01M10 7.6h.01M12.6 7.6h.01M6.7 10.3h.01M13.3 10.3h.01" stroke-width="1.8" />
        <path d="M10 13.1v3.4" />
      </svg>
    </button>
    <!-- 新建 Telnet 终端 -->
    <button class="tb-btn" :title="t('toolbar.newTelnet')" @click="emit('new-telnet')">
      <svg class="ic-telnet" width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <circle cx="10" cy="10" r="6.5" />
        <ellipse cx="10" cy="10" rx="2.9" ry="6.5" />
        <path d="M3.5 10h13M4.7 6.4h10.6M4.7 13.6h10.6" />
      </svg>
    </button>

    <div class="tb-divider" />

    <!-- 会话管理 -->
    <button class="tb-btn" :title="t('toolbar.sessionManager')" @click="emit('session-manager')">
      <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3.5 6.3c0-.7.6-1.3 1.3-1.3h3l1.8 1.8h5.6c.7 0 1.3.6 1.3 1.3v7.6c0 .7-.6 1.3-1.3 1.3H4.8c-.7 0-1.3-.6-1.3-1.3V6.3z" />
      </svg>
    </button>

    <div class="tb-spacer" />

    <div class="address-bar">
      <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" class="addr-lock">
        <rect x="5" y="9" width="10" height="7.5" rx="1.5" /><path d="M7.5 9V7a2.5 2.5 0 0 1 5 0v2" />
      </svg>
      <input
        v-model="quickInput"
        class="addr-input"
        :placeholder="t('toolbar.quickConnectPlaceholder')"
        spellcheck="false"
        @keydown.enter="quickConnect"
      />
    </div>

    <button class="tb-btn" :title="t('toolbar.settings')" @click="emit('open-settings')">
      <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
        <circle cx="10" cy="10" r="2.1" />
        <path d="M10 3.2v2M10 14.8v2M16.8 10h-2M5.2 10h-2M14.65 5.35l-1.42 1.42M6.77 13.23l-1.42 1.42M14.65 14.65l-1.42-1.42M6.77 6.77L5.35 5.35" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 44px;
  padding: 0 8px;
  flex: none;
}
.tb-btn {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 32px;
  min-width: 32px;
  padding: 0 7px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-primary);
  cursor: pointer;
}
.tb-btn:hover:not(:disabled) {
  background: var(--bg-alt-hover);
}
.tb-btn:disabled {
  color: var(--text-disabled);
  cursor: default;
}
/* 类型色图标（与对应标签顶线一致） */
.ic-local {
  color: var(--type-local);
}
.ic-ssh {
  color: var(--type-ssh);
}
.ic-serial {
  color: var(--type-serial);
}
.ic-telnet {
  color: var(--type-telnet);
}
.tb-divider {
  width: 1px;
  height: 20px;
  background: var(--stroke-divider);
  margin: 0 4px;
  flex: none;
}
.tb-spacer {
  flex: 1;
}
.address-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  width: 380px;
  max-width: 34vw;
  padding: 0 10px;
  background: var(--bg-card);
  border: 1px solid var(--stroke-control);
  border-radius: var(--radius-sm);
}
.address-bar:focus-within {
  border-color: var(--accent);
}
.addr-lock {
  color: var(--text-tertiary);
  flex: none;
}
.addr-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font: inherit;
  font-size: 12.5px;
  color: var(--text-primary);
}
.addr-input::placeholder {
  color: var(--text-tertiary);
}
</style>
