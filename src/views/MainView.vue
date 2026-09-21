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
import ImportDialog from '../components/import/ImportDialog.vue'
import SessionManagerDialog from '../components/SessionManagerDialog.vue'
import UserKeysDialog from '../components/UserKeysDialog.vue'
import SettingsView from './SettingsView.vue'
import { FButton, FDialog, FTextBox, toast } from '../ui'
import { useSessionsStore } from '../stores/sessions'
import { useTransfersStore } from '../stores/transfers'
import { useProfilesStore, parseQuickConnect } from '../stores/profiles'
import { useSettingsStore } from '../stores/settings'
import { defaultProfile, type SessionTab } from '../api/types'
import { openConfigDir } from '../api/backend'
import { t } from '../i18n'
import { tauriWindow } from '../api/adapter'

const sessions = useSessionsStore()
const profiles = useProfilesStore()
const settings = useSettingsStore()

/* ---------- 命令调度 ---------- */

const renameDialogOpen = ref(false)
const renameValue = ref('')
const aboutOpen = ref(false)
const importDialogOpen = ref(false)
const sessionManagerOpen = ref(false)
const userKeysOpen = ref(false)
const hotkeysOpen = ref(false)
const sftpRef = ref<InstanceType<typeof SftpView>>()

/** 向当前激活标签派发终端命令（清屏/复制/粘贴/全选/搜索） */
function activeTabCommand(cmd: string) {
  const tab = sessions.activeTab
  if (!tab) return
  window.dispatchEvent(new CustomEvent('hishell-tab-command', { detail: { tabId: tab.id, cmd } }))
}

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
    case 'new-local':
      sessions.createLocalTab()
      break
    case 'new-serial':
      toast(t('main.serialTodo'))
      break
    case 'new-telnet':
      toast(t('main.telnetTodo'))
      break
    case 'import-sessions':
      importDialogOpen.value = true
      break
    case 'open-config-dir':
      openConfigDir().catch((e) => toast(String(e), 'error'))
      break
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
      activeTabCommand('copy')
      break
    case 'edit-paste':
      activeTabCommand('paste')
      break
    case 'edit-clear':
      activeTabCommand('clear')
      break
    case 'edit-select-all':
      activeTabCommand('select-all')
      break
    case 'help-hotkeys':
      hotkeysOpen.value = true
      break
    case 'session-manager':
      sessionManagerOpen.value = true
      break
    case 'user-keys':
      userKeysOpen.value = true
      break
    case 'forward-manager':
    case 'ports-manager': {
      if (sessions.activeTab?.kind === 'session') sessions.openPortsDialog()
      else toast(t('main.noSessionForSftp'), 'error')
      break
    }
    case 'quick-upload':
      quickUpload()
      break
    case 'quick-download':
      quickDownload()
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

/* ---------- 快捷上传/下载（工具菜单） ---------- */

const transfersStore = useTransfersStore()

/** 需要一个已连接的 SSH 会话（SFTP 视图可用时才可取到远程路径） */
function connectedSession() {
  const tab = sessions.activeTab
  if (!tab || tab.kind !== 'session' || tab.status !== 'connected' || tab.sid === undefined) {
    toast(t('main.needConnectedSession'), 'error')
    return null
  }
  return tab
}

async function quickUpload() {
  const tab = connectedSession()
  if (!tab) return
  const remotePath = sftpRef.value?.getRemotePath()
  if (!remotePath) {
    toast(t('main.needConnectedSession'), 'error')
    return
  }
  const { open } = await import('@tauri-apps/plugin-dialog')
  const files = await open({ multiple: true, title: t('sftp.upload') })
  if (!files) return
  const list = Array.isArray(files) ? files : [files]
  for (const f of list) {
    const name = f.split(/[\\/]/).pop() ?? f
    await transfersStore.submit({
      sid: tab.sid,
      name,
      isDir: false,
      direction: 'upload',
      localPath: f,
      remotePath: `${remotePath.replace(/\/+$/, '')}/${name}`,
      total: 1,
    })
  }
  toast(t('sftp.paneLoadingToastUp', { n: list.length }), 'success')
}

async function quickDownload() {
  const tab = connectedSession()
  if (!tab) return
  const targets = sftpRef.value?.getRemoteSelection() ?? []
  if (!targets.length) {
    toast(t('main.noSelectionForDownload'), 'error')
    return
  }
  const { open } = await import('@tauri-apps/plugin-dialog')
  const dir = await open({ directory: true, title: t('sftp.download') })
  if (typeof dir !== 'string' || !dir) return
  const remoteDir = sftpRef.value?.getRemotePath() ?? '/'
  for (const entry of targets) {
    await transfersStore.submit({
      sid: tab.sid,
      name: entry.name,
      isDir: entry.kind === 'dir',
      direction: 'download',
      localPath: `${dir.replace(/[\\/]+$/, '')}\\${entry.name}`,
      remotePath: `${remoteDir.replace(/\/+$/, '')}/${entry.name}`,
      total: entry.size || 1,
    })
  }
  toast(t('sftp.paneLoadingToast', { n: targets.length }), 'success')
}

/* ---------- 全局快捷键 ---------- */

function onKeydown(e: KeyboardEvent) {
  const ctrl = e.ctrlKey || e.metaKey
  if (ctrl && e.key === 'n') {
    e.preventDefault()
    command('new-connection')
  } else if (e.altKey && e.key.toLowerCase() === 'n') {
    e.preventDefault()
    command('new-connection')
  } else if (e.altKey && e.key.toLowerCase() === 's') {
    e.preventDefault()
    command('new-sftp')
  } else if (e.altKey && e.key.toLowerCase() === 'l') {
    e.preventDefault()
    command('new-local')
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

/** 快捷键一览（与实际实现一致） */
const hotkeyRows = computed(() => [
  { action: t('menus.newConnection'), keys: 'Ctrl+N / Alt+N' },
  { action: t('menus.newSftp'), keys: 'Alt+S' },
  { action: t('menus.fileView'), keys: 'Ctrl+Shift+F' },
  { action: t('menus.newLocalTerminal'), keys: 'Alt+L' },
  { action: t('menus.tabNext'), keys: 'Ctrl+Tab' },
  { action: t('menus.tabPrev'), keys: 'Ctrl+Shift+Tab' },
  { action: t('menus.tabClose'), keys: 'Ctrl+W' },
  { action: t('menus.clearScreen'), keys: 'Ctrl+L' },
])

/** 终端视图何时隐藏：SSH 会话按视图模式切换；本地终端永远显示终端 */
function isHidden(tab: { kind: SessionTab['kind']; viewMode: SessionTab['viewMode'] }) {
  return tab.kind === 'session' && tab.viewMode !== 'terminal'
}
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
          <!-- 终端/文件视图均保持挂载（v-show 切换），避免 xterm 二次 open 丢内容；
               本地终端标签没有 SFTP 视图 -->
          <TerminalPane :tab="activeTab" :class="{ 'view-hidden': isHidden(activeTab) }" />
          <SftpView
            v-if="activeTab.kind === 'session'"
            ref="sftpRef"
            :tab="activeTab"
            :class="{ 'view-hidden': activeTab.viewMode !== 'sftp' }"
          />
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
    <ImportDialog v-model:open="importDialogOpen" />
    <SessionManagerDialog v-model:open="sessionManagerOpen" />
    <UserKeysDialog v-model:open="userKeysOpen" />

    <FDialog v-model:open="hotkeysOpen" :title="t('menus.hotkeys')" :width="480">
      <div class="hotkeys-list">
        <div v-for="h in hotkeyRows" :key="h.action" class="hotkey-row">
          <span>{{ h.action }}</span>
          <span class="hotkey-keys">{{ h.keys }}</span>
        </div>
      </div>
      <template #footer>
        <FButton variant="accent" @click="hotkeysOpen = false">{{ t('common.close') }}</FButton>
      </template>
    </FDialog>

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
.hotkeys-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.hotkey-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 7px 6px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
}
.hotkey-row:hover {
  background: var(--bg-alt-hover);
}
.hotkey-keys {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 11.5px;
  color: var(--text-secondary);
  background: var(--bg-alt-pressed);
  padding: 2px 8px;
  border-radius: 4px;
}
.about-desc {
  color: var(--text-secondary);
  font-size: 12.5px;
  line-height: 1.7;
  white-space: pre-line;
}
</style>
