<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import TitleBar from '../components/TitleBar.vue'
import MenuBar from '../components/MenuBar.vue'
import ToolBar from '../components/ToolBar.vue'
import TabStrip from '../components/TabStrip.vue'
import SessionToolbar from '../components/SessionToolbar.vue'
import StatusBar from '../components/StatusBar.vue'
import TerminalPane from '../components/terminal/TerminalPane.vue'
import SftpView from '../components/sftp/SftpView.vue'
import ConnectDialog from '../components/connect/ConnectDialog.vue'
import PortForwardDialog from '../components/ports/PortForwardDialog.vue'
import PromptDialog from '../components/PromptDialog.vue'
import SettingsView from './SettingsView.vue'
import { FButton, FDialog, FTextBox, toast } from '../ui'
import { useSessionsStore } from '../stores/sessions'
import { useProfilesStore, parseQuickConnect } from '../stores/profiles'
import { useSettingsStore } from '../stores/settings'
import { defaultProfile } from '../api/types'
import { t } from '../i18n'
import { tauriWindow } from '../api/adapter'

const sessions = useSessionsStore()
const profiles = useProfilesStore()
const settings = useSettingsStore()

/* ---------- 命令调度 ---------- */

const renameDialogOpen = ref(false)
const renameValue = ref('')
const aboutOpen = ref(false)

function command(raw: string) {
  // 携带参数的命令：name:payload
  const idx = raw.indexOf(':')
  const name = idx >= 0 ? raw.slice(0, idx) : raw
  const payload = idx >= 0 ? raw.slice(idx + 1) : ''

  switch (name) {
    case 'new-connection':
      sessions.openConnectDialog()
      break
    case 'quick-connect': {
      const info = parseQuickConnect(payload)
      if (!info) return
      const p = defaultProfile({ name: info.host, host: info.host, port: info.port, username: info.username })
      sessions.createSession(p)
      break
    }
    case 'new-sftp': {
      const first = profiles.profiles[0]
      if (!first) {
        toast(t('main.noProfileForSftp'), 'error')
        sessions.openConnectDialog()
        return
      }
      const tab = sessions.createSession(first)
      sessions.setViewMode(tab.id, 'sftp')
      break
    }
    case 'reconnect': {
      const tab = sessions.activeTab
      if (tab?.profileId) {
        const p = profiles.byId(tab.profileId)
        if (p) {
          sessions.reconnect(tab.id)
          toast(t('main.reconnecting', { host: p.host }))
        }
      } else if (tab) {
        toast(t('main.noProfileForReconnect'), 'error')
      }
      break
    }
    case 'disconnect': {
      const tab = sessions.activeTab
      if (tab) {
        sessions.disconnectById(tab.id)
        toast(t('main.disconnected'))
      }
      break
    }
    case 'view-terminal':
      if (sessions.activeTab?.kind === 'session') sessions.setViewMode(sessions.activeTab.id, 'terminal')
      break
    case 'view-sftp':
      if (sessions.activeTab?.kind === 'session') sessions.setViewMode(sessions.activeTab.id, 'sftp')
      else toast(t('main.noSessionForSftp'), 'error')
      break
    case 'open-settings':
      sessions.openSettings()
      break
    case 'theme-light':
      settings.patch({ theme: 'light' })
      break
    case 'theme-dark':
      settings.patch({ theme: 'dark' })
      break
    case 'tab-next':
      sessions.cycleTab(1)
      break
    case 'tab-prev':
      sessions.cycleTab(-1)
      break
    case 'tab-close':
      if (sessions.activeTabId) sessions.closeTab(sessions.activeTabId)
      break
    case 'tab-close-others':
      if (sessions.activeTabId) sessions.closeOthers(sessions.activeTabId)
      break
    case 'activate-tab':
      sessions.activate(payload)
      break
    case 'rename-tab':
      renameValue.value = sessions.tabs.find((t) => t.id === payload)?.title ?? ''
      renameDialogOpen.value = true
      break
    case 'confirm-rename':
      sessions.renameTab(sessions.activeTabId, renameValue.value)
      renameDialogOpen.value = false
      break
    case 'edit-copy':
      toast(t('main.copyHint'))
      break
    case 'edit-paste':
      toast(t('main.pasteHint'))
      break
    case 'edit-clear':
      toast(t('main.clearHint'))
      break
    case 'forward-manager':
    case 'ports-manager': {
      if (sessions.activeTab?.kind === 'session') sessions.openPortsDialog()
      else toast(t('main.noSessionForSftp'), 'error')
      break
    }
    case 'quick-upload':
    case 'quick-download':
      toast(t('main.quickTransferHint'))
      break
    case 'exit':
      tauriWindow().then((w) => w?.close())
      break
    case 'about':
      aboutOpen.value = true
      break
    default:
      if (name && !raw.includes(':')) toast(t('main.later', { name }))
  }
}

function onWindowCommand(e: Event) {
  command((e as CustomEvent<string>).detail)
}

/* ---------- 全局快捷键 ---------- */

function onKeydown(e: KeyboardEvent) {
  const ctrl = e.ctrlKey || e.metaKey
  if (ctrl && e.key === 'n') {
    e.preventDefault()
    command('new-connection')
  } else if (ctrl && e.shiftKey && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    command('view-sftp')
  } else if (ctrl && e.key === 'Tab') {
    e.preventDefault()
    command(e.shiftKey ? 'tab-prev' : 'tab-next')
  } else if (ctrl && e.key.toLowerCase() === 'w') {
    e.preventDefault()
    command('tab-close')
  }
}

onMounted(() => {
  window.addEventListener('hishell-command', onWindowCommand)
  window.addEventListener('keydown', onKeydown)
})
onUnmounted(() => {
  window.removeEventListener('hishell-command', onWindowCommand)
  window.removeEventListener('keydown', onKeydown)
})

/* 首次启动自动打开一个演示会话，便于预览（对应参考截图） */
onMounted(async () => {
  await profiles.init()
  if (sessions.tabs.length === 0 && profiles.profiles.length > 0) {
    sessions.createSession(profiles.profiles[0]!)
    const second = profiles.profiles[1]
    if (second) sessions.createSession(second)
    sessions.activate(sessions.tabs[0]!.id)
  }
})

const activeTab = computed(() => sessions.activeTab)
</script>

<template>
  <div class="main-view">
    <TitleBar />
    <MenuBar @command="command" />
    <ToolBar />
    <TabStrip />

    <div class="main-content">
      <template v-if="activeTab?.kind === 'settings'">
        <SettingsView />
      </template>
      <template v-else-if="activeTab">
        <SessionToolbar :tab="activeTab" />
        <div class="pane-area">
          <!-- 终端/文件视图均保持挂载（v-show 切换），避免 xterm 二次 open 丢内容 -->
          <TerminalPane :tab="activeTab" :class="{ 'view-hidden': activeTab.viewMode !== 'terminal' }" />
          <SftpView :tab="activeTab" :class="{ 'view-hidden': activeTab.viewMode !== 'sftp' }" />
        </div>
      </template>
      <div v-else class="empty-state">
        <div class="empty-logo">&gt;_</div>
        <div class="empty-title">{{ t('main.welcome') }}</div>
        <div class="empty-desc">{{ t('main.welcomeDesc') }}</div>
        <div class="empty-actions">
          <FButton variant="accent" icon="plus" @click="command('new-connection')">{{ t('common.newConnection') }}</FButton>
          <FButton icon="settings" @click="command('open-settings')">{{ t('main.openSettings') }}</FButton>
        </div>
      </div>
    </div>

    <StatusBar />
    <ConnectDialog />
    <PortForwardDialog />
    <PromptDialog />

    <FDialog v-model:open="renameDialogOpen" :title="t('tabs.renameTitle')" :width="420">
      <FTextBox v-model="renameValue" :placeholder="t('tabs.renamePlaceholder')" @keydown.enter="command('confirm-rename')" />
      <template #footer>
        <FButton variant="subtle" @click="renameDialogOpen = false">{{ t('common.cancel') }}</FButton>
        <FButton variant="accent" @click="command('confirm-rename')">{{ t('common.confirm') }}</FButton>
      </template>
    </FDialog>

    <FDialog v-model:open="aboutOpen" :title="t('main.aboutTitle')" :width="420">
      <div class="about-body">
        <div class="about-logo">&gt;_</div>
        <div>
          <div class="about-name">{{ t('main.aboutName') }}</div>
          <div class="about-desc">{{ t('main.aboutDesc') }}</div>
        </div>
      </div>
      <template #footer>
        <FButton variant="accent" @click="aboutOpen = false">{{ t('common.close') }}</FButton>
      </template>
    </FDialog>
  </div>
</template>

<style scoped>
.main-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: transparent;
}
.main-content {
  flex: 1;
  min-height: 0;
  background: var(--bg-base);
  position: relative;
  display: flex;
  flex-direction: column;
}
.main-content > :not(.session-toolbar) {
  flex: 1;
  min-height: 0;
}
.pane-area {
  flex: 1;
  min-height: 0;
  position: relative;
}
.pane-area > * {
  height: 100%;
}
.pane-area > .view-hidden {
  display: none !important;
}
.empty-state {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
}
.empty-logo {
  width: 72px;
  height: 72px;
  border-radius: 16px;
  background: var(--accent);
  color: var(--accent-text-on);
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 26px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 6px;
}
.empty-title {
  font-size: 20px;
  font-weight: 600;
}
.empty-desc {
  color: var(--text-secondary);
  font-size: 13px;
  margin-bottom: 14px;
}
.empty-actions {
  display: flex;
  gap: 10px;
}
.about-body {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}
.about-logo {
  width: 56px;
  height: 56px;
  border-radius: 12px;
  background: var(--accent);
  color: var(--accent-text-on);
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 20px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  flex: none;
}
.about-name {
  font-weight: 600;
  margin-bottom: 6px;
}
.about-desc {
  color: var(--text-secondary);
  font-size: 12.5px;
  line-height: 1.7;
  white-space: pre-line;
}
</style>
