<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
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
      { label: t('menus.localSessions'), command: 'local-sessions' },
      { separator: true },
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
  itemIndex.value = -1
  openMenu.value = openMenu.value === label ? '' : label
}
function hover(label: string) {
  if (openMenu.value) {
    itemIndex.value = -1
    openMenu.value = label
  }
}
function run(command: string) {
  openMenu.value = ''
  itemIndex.value = -1
  if (command) emit('command', command)
}

/* ---------- 键盘助记键与菜单键盘导航 ----------
 * Windows/Linux 惯例：Alt+字母 打开/收起对应顶级菜单（字母取标签尾部 (X)，与具体布局无关）；
 * 菜单打开后：字母 跳顶级菜单、←→ 切换、↑↓ 移动条目、回车执行、Esc 关闭；单独按 Alt 打开/收起首菜单。
 * macOS 不拦截：Option+字母 是特殊字符输入（平台惯例不同），菜单保持鼠标操作，
 * 且 Cmd 系快捷键由 MainView 的全局分发器处理（metaKey 已覆盖）。 */
const isMac = /mac|iphone|ipad/i.test(navigator.platform)
const itemIndex = ref(-1)
let altAlone = false

function mnemonicOf(label: string): string {
  const m = label.match(/[（(]([A-Za-z])[)）]$/)
  return m ? m[1].toLowerCase() : ''
}

/** 可键盘选择的条目（跳过分割线与禁用项）在 children 中的下标 */
function actionableIdxs(menu: MenuDef): number[] {
  const out: number[] = []
  menu.children.forEach((c, i) => {
    if (!c.separator && !c.disabled && c.label !== undefined) out.push(i)
  })
  return out
}

function openByLetter(letter: string): boolean {
  const menu = menus.value.find((m) => mnemonicOf(m.label) === letter)
  if (!menu) return false
  itemIndex.value = -1
  openMenu.value = openMenu.value === menu.label ? '' : menu.label
  return true
}

function onMenuKeydown(e: KeyboardEvent) {
  if (e.key === 'Alt') {
    // AltGr（欧洲键盘）会同时带 ctrlKey，排除掉避免误触发
    if (!e.repeat && !e.ctrlKey && !e.metaKey && !e.shiftKey) altAlone = true
    return
  }
  altAlone = false

  // Alt+字母 → 打开/收起对应顶级菜单；未匹配到助记键的放行（如 Alt+N 新建连接、Alt+L 本地终端）
  if (!isMac && e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && /^[a-z]$/i.test(e.key)) {
    if (openByLetter(e.key.toLowerCase())) {
      e.preventDefault()
      e.stopPropagation()
    }
    return
  }

  if (!openMenu.value) return
  const menu = menus.value.find((m) => m.label === openMenu.value)
  if (!menu) return
  const target = e.target as HTMLElement | null
  const typing = !!target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)

  switch (e.key) {
    case 'Escape':
      openMenu.value = ''
      itemIndex.value = -1
      e.preventDefault()
      e.stopPropagation()
      break
    case 'ArrowRight':
    case 'ArrowLeft': {
      const n = menus.value.length
      const idx = menus.value.findIndex((m) => m.label === openMenu.value)
      const next = menus.value[(idx + (e.key === 'ArrowRight' ? 1 : n - 1)) % n]
      openMenu.value = next.label
      itemIndex.value = -1
      e.preventDefault()
      e.stopPropagation()
      break
    }
    case 'ArrowDown':
    case 'ArrowUp': {
      if (typing) return
      const idxs = actionableIdxs(menu)
      if (!idxs.length) return
      const dir = e.key === 'ArrowDown' ? 1 : -1
      itemIndex.value =
        itemIndex.value < 0
          ? dir > 0
            ? idxs[0]
            : idxs[idxs.length - 1]
          : idxs[(idxs.indexOf(itemIndex.value) + dir + idxs.length) % idxs.length]
      e.preventDefault()
      e.stopPropagation()
      break
    }
    case 'Enter': {
      if (typing || itemIndex.value < 0) return
      const item = menu.children[itemIndex.value]
      if (item && !item.separator && !item.disabled && item.command) {
        e.preventDefault()
        e.stopPropagation()
        run(item.command)
      }
      break
    }
    default: {
      // 菜单打开时直接按字母 → 跳到对应顶级菜单
      if (!typing && !isMac && !e.altKey && !e.ctrlKey && !e.metaKey && /^[a-z]$/i.test(e.key)) {
        if (openByLetter(e.key.toLowerCase())) {
          e.preventDefault()
          e.stopPropagation()
        }
      }
    }
  }
}

function onMenuKeyup(e: KeyboardEvent) {
  if (e.key !== 'Alt' || !altAlone) return
  altAlone = false
  itemIndex.value = -1
  openMenu.value = openMenu.value ? '' : menus.value[0]?.label ?? ''
}

function onWinBlur() {
  altAlone = false
}

window.addEventListener('keydown', onMenuKeydown, true)
window.addEventListener('keyup', onMenuKeyup)
window.addEventListener('blur', onWinBlur)
onUnmounted(() => {
  window.removeEventListener('keydown', onMenuKeydown, true)
  window.removeEventListener('keyup', onMenuKeyup)
  window.removeEventListener('blur', onWinBlur)
})

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
            :class="{ disabled: item.disabled, danger: item.command === 'exit', 'kb-focus': openMenu === menu.label && itemIndex === i }"
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
/* 键盘 ↑↓ 选中态（与鼠标 hover 同色） */
.menu-item.kb-focus {
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
