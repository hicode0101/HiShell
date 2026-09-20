<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { tauriWindow } from '../api/adapter'
import { isTauri } from '../api/adapter'
import { showContextMenu } from '../ui'
import { t } from '../i18n'

const maximizeState = ref<'maximized' | 'normal'>('normal')
let win: Awaited<ReturnType<typeof tauriWindow>> = null

async function refreshMaximized() {
  if (!win) return
  try {
    maximizeState.value = (await (win as any).isMaximized?.()) ? 'maximized' : 'normal'
  } catch {
    /* ignore */
  }
}

onMounted(async () => {
  win = await tauriWindow()
  await refreshMaximized()
})

async function minimize() {
  win?.minimize()
}
async function toggleMaximize() {
  win?.toggleMaximize()
  setTimeout(refreshMaximized, 200)
}
async function close() {
  win?.close()
}

function onContextMenu(e: MouseEvent) {
  if (!isTauri) return
  showContextMenu(e, [
    { label: t('titlebar.restore'), onClick: () => win?.toggleMaximize(), disabled: maximizeState.value !== 'maximized' },
    { label: t('titlebar.minimize'), onClick: minimize },
    { label: t('titlebar.maximize'), onClick: toggleMaximize, disabled: maximizeState.value === 'maximized' },
    { separator: true },
    { label: t('common.close'), shortcut: 'Alt+F4', onClick: close },
  ])
}
</script>

<template>
  <div class="titlebar" data-tauri-drag-region @contextmenu="onContextMenu">
    <div class="titlebar-left" data-tauri-drag-region>
      <svg class="app-mark" width="16" height="16" viewBox="0 0 20 20">
        <rect x="0" y="0" width="20" height="20" rx="4.5" fill="var(--accent)" />
        <path d="M4.5 7l3 2.5-3 2.5M9.3 12.7h4" stroke="#fff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" fill="none" />
      </svg>
      <span class="app-title" data-tauri-drag-region>HiShell</span>
    </div>
    <div class="caption-buttons">
      <button class="caption-btn" :title="t('titlebar.minimize')" @click="minimize">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" stroke-width="1" /></svg>
      </button>
      <button class="caption-btn" :title="maximizeState === 'maximized' ? t('titlebar.restore') : t('titlebar.maximize')" @click="toggleMaximize">
        <svg v-if="maximizeState !== 'maximized'" width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.5" y="0.5" width="9" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 10 10">
          <rect x="0.5" y="2.5" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="1" />
          <path d="M2.5 2.5v-1a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      </button>
      <button class="caption-btn close" :title="t('common.close')" @click="close">
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" /></svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.titlebar {
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex: none;
}
.titlebar-left {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-left: 14px;
  height: 100%;
  flex: 1;
}
.app-mark {
  flex: none;
}
.app-title {
  font-size: 12.5px;
  color: var(--text-secondary);
  letter-spacing: 0.3px;
}
.caption-buttons {
  display: flex;
  height: 100%;
  flex: none;
}
.caption-btn {
  width: 46px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--text-primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
.caption-btn:hover {
  background: var(--bg-alt-hover);
}
.caption-btn.close:hover {
  background: #c42b1c;
  color: #fff;
}
</style>
