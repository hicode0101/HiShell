<script setup lang="ts">
import { computed, ref } from 'vue'
import { t } from '../i18n'
import { useSessionsStore } from '../stores/sessions'
import { COPY_SHORTCUT, PASTE_SHORTCUT } from '../utils/platform'

interface MenuChild {
  label?: string
  command?: string
  shortcut?: string
  separator?: boolean
  disabled?: boolean
  checked?: boolean
}
interface MenuDef {
  label: string
  children: MenuChild[]
}

const emit = defineEmits<{ (e: 'command', id: string): void }>()

const sessions = useSessionsStore()
const openMenu = ref('')

const menus = computed<MenuDef[]>(() => [
  {
    label: t('menus.file'),
    children: [
      { label: t('menus.newLocalTerminal'), command: 'new-local', shortcut: 'Alt+L' },
      { label: t('menus.newConnection'), command: 'new-connection', shortcut: 'Alt+N' },
      { label: t('menus.newSerial'), command: 'new-serial' },
      { label: t('menus.newTelnet'), command: 'new-telnet' },
      { separator: true },
      // 导入会话（Xshell / Tabby）入口暂时隐藏，功能保留，需要时恢复下行即可
      // { label: t('menus.importSessions'), command: 'import-sessions' },
      { label: t('menus.openConfigDir'), command: 'open-config-dir' },
      { separator: true },
      { label: t('menus.exit'), command: 'exit' },
    ],
  },
  {
    label: t('menus.edit'),
    children: [
      { label: t('menus.copy'), command: 'edit-copy', shortcut: COPY_SHORTCUT },
      { label: t('menus.paste'), command: 'edit-paste', shortcut: PASTE_SHORTCUT },
      { separator: true },
      { label: t('menus.selectAll'), command: 'edit-select-all' },
      { label: t('menus.clearScreen'), command: 'edit-clear', shortcut: 'Ctrl+L' },
    ],
  },
  {
    label: t('menus.tools'),
    children: [
      { label: t('menus.sessionManager'), command: 'session-manager' },
      { label: t('menus.userKeys'), command: 'user-keys' },
      { separator: true },
      { label: t('menus.options'), command: 'open-settings' },
    ],
  },
  {
    label: t('menus.tabs'),
    children: [
      { label: t('menus.tabNext'), command: 'tab-next', shortcut: 'Ctrl+Tab' },
      { label: t('menus.tabPrev'), command: 'tab-prev', shortcut: 'Ctrl+Shift+Tab' },
      { separator: true },
      { label: t('menus.tabClose'), command: 'tab-close', shortcut: 'Ctrl+W' },
      { label: t('menus.tabCloseOthers'), command: 'tab-close-others' },
    ],
  },
  {
    label: t('menus.window'),
    children: sessions.tabs.length
      ? sessions.tabs.map((tab) => ({
          label: `${tab.status === 'connected' ? '● ' : '○ '}${tab.title}`,
          command: 'activate-tab:' + tab.id,
          checked: tab.id === sessions.activeTabId,
        }))
      : [{ label: t('menus.noTabs'), command: '', disabled: true }],
  },
  {
    label: t('menus.help'),
    children: [
      { label: t('menus.hotkeys'), command: 'help-hotkeys' },
      { label: t('menus.about'), command: 'about' },
    ],
  },
])

function toggle(label: string) {
  openMenu.value = openMenu.value === label ? '' : label
}
function hover(label: string) {
  if (openMenu.value) openMenu.value = label
}
function run(command: string) {
  openMenu.value = ''
  if (command) emit('command', command)
}

function onDocDown(e: MouseEvent) {
  const el = document.querySelector('.menubar')
  if (el && !el.contains(e.target as Node)) openMenu.value = ''
}
if (typeof document !== 'undefined') document.addEventListener('mousedown', onDocDown)
</script>

<template>
  <div class="menubar">
    <div
      v-for="menu in menus"
      :key="menu.label"
      class="menu-top"
      :class="{ open: openMenu === menu.label }"
      @click="toggle(menu.label)"
      @mouseenter="hover(menu.label)"
    >
      <span class="menu-top-label">{{ menu.label }}</span>
      <div v-if="openMenu === menu.label" class="menu-flyout">
        <template v-for="(item, i) in menu.children" :key="i">
          <div v-if="item.separator" class="menu-sep" />
          <div
            v-else
            class="menu-item"
            :class="{ disabled: item.disabled, danger: item.command === 'exit' }"
            @click.stop="!item.disabled && run(item.command ?? '')"
          >
            <span class="menu-check">{{ item.checked ? '✓' : '' }}</span>
            <span class="menu-item-label">{{ item.label }}</span>
            <span class="menu-shortcut">{{ item.shortcut }}</span>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.menubar {
  display: flex;
  align-items: stretch;
  height: 36px;
  padding: 0 6px;
  flex: none;
  position: relative;
  z-index: 500;
}
.menu-top {
  position: relative;
  display: flex;
  align-items: center;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.menu-top:hover,
.menu-top.open {
  background: var(--bg-alt-hover);
}
.menu-top-label {
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
}
.menu-flyout {
  position: absolute;
  top: calc(100% - 2px);
  left: 0;
  min-width: 230px;
  background: var(--bg-layer);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-flyout);
  padding: 4px;
  z-index: 600;
  animation: hishell-fade-in 0.1s ease;
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 13px;
  white-space: nowrap;
}
.menu-item:hover {
  background: var(--bg-alt-hover);
}
.menu-item.disabled {
  color: var(--text-disabled);
  cursor: default;
}
.menu-item.disabled:hover {
  background: transparent;
}
.menu-item.danger {
  color: var(--danger);
}
.menu-check {
  width: 14px;
  color: var(--accent);
  flex: none;
  font-size: 12px;
}
.menu-item-label {
  flex: 1;
}
.menu-shortcut {
  color: var(--text-tertiary);
  font-size: 12px;
  margin-left: 28px;
}
.menu-sep {
  height: 1px;
  background: var(--stroke-divider);
  margin: 4px 8px;
}
</style>
