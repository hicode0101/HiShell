<script setup lang="ts">
import { onMounted } from 'vue'
import { useSettingsStore } from './stores/settings'
import MainView from './views/MainView.vue'
import { FContextMenuHost, FToastHost } from './ui'
import { isTauri, isWindows } from './api/adapter'
import { initGlobalEvents } from './stores/events'

const settings = useSettingsStore()

onMounted(() => {
  settings.applyToDocument()
  initGlobalEvents()
  // Mica/Acrylic 仅在 Windows 生效；mac/linux 保持不透明背景
  if (isTauri && isWindows) {
    document.documentElement.classList.add('env-tauri')
    import('@tauri-apps/api/window')
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().setEffects({ effects: ['mica', 'acrylic'] } as never),
      )
      .catch(() => {})
  }
})
</script>

<template>
  <MainView />
  <FContextMenuHost />
  <FToastHost />
</template>
