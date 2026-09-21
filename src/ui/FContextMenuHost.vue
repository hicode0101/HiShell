<script setup lang="ts">
/**
 * 全局右键菜单宿主。关闭时机（对齐 Windows 原生菜单习惯）：
 * - 在菜单外按下任意鼠标键（左键/右键/中键）→ 关闭；左键点击同时穿透到底下 UI 正常生效
 * - 点菜单项 → 执行并关闭（菜单项自身处理）
 * - Esc / 滚轮 / 窗口失焦 → 关闭
 */
import { onMounted, onUnmounted } from 'vue'
import { contextMenuState, hideContextMenu } from './contextMenu'

function onDocMouseDown(e: MouseEvent) {
  if (!contextMenuState.open) return
  const menu = document.getElementById('f-context-menu')
  if (menu && menu.contains(e.target as Node)) return
  hideContextMenu()
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && contextMenuState.open) hideContextMenu()
}

function onWheel() {
  if (contextMenuState.open) hideContextMenu()
}

function onBlur() {
  if (contextMenuState.open) hideContextMenu()
}

onMounted(() => {
  // capture：先于页面内任何 stopPropagation 处理，保证一定能收到
  document.addEventListener('mousedown', onDocMouseDown, true)
  document.addEventListener('keydown', onKeydown)
  window.addEventListener('wheel', onWheel, { passive: true })
  window.addEventListener('blur', onBlur)
})
onUnmounted(() => {
  document.removeEventListener('mousedown', onDocMouseDown, true)
  document.removeEventListener('keydown', onKeydown)
  window.removeEventListener('wheel', onWheel)
  window.removeEventListener('blur', onBlur)
})
</script>

<template>
  <div
    id="f-context-menu"
    :style="{
      position: 'fixed',
      zIndex: 3000,
      left: contextMenuState.x + 'px',
      top: contextMenuState.y + 'px',
      display: contextMenuState.open ? 'block' : 'none',
    }"
    @contextmenu.prevent
  >
    <div class="f-ctx-menu">
      <template v-for="(item, i) in contextMenuState.items" :key="i">
        <div v-if="item.separator" class="f-ctx-sep" />
        <div
          v-else
          class="f-ctx-item"
          :class="{ danger: item.danger, disabled: item.disabled }"
          @click="
            () => {
              if (item.disabled) return
              contextMenuState.open = false
              item.onClick?.()
            }
          "
        >
          <span class="f-ctx-icon">
            <FIcon v-if="item.checked" name="check" :size="15" />
            <FIcon v-else-if="item.icon" :name="item.icon" :size="15" />
          </span>
          <span class="f-ctx-label">{{ item.label }}</span>
          <span class="f-ctx-shortcut">{{ item.shortcut }}</span>
        </div>
      </template>
    </div>
  </div>
</template>

<script lang="ts">
import FIcon from './FIcon.vue'
export default { components: { FIcon } }
</script>

<style>
.f-ctx-menu {
  min-width: 200px;
  background: var(--bg-layer);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-flyout);
  padding: 4px;
  animation: hishell-fade-in 0.1s ease;
}
.f-ctx-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-primary);
  font-size: 13px;
  white-space: nowrap;
}
.f-ctx-item:hover {
  background: var(--bg-alt-hover);
}
.f-ctx-item.danger {
  color: var(--danger);
}
.f-ctx-item.disabled {
  color: var(--text-disabled);
  cursor: default;
}
.f-ctx-item.disabled:hover {
  background: transparent;
}
.f-ctx-icon {
  width: 16px;
  display: inline-flex;
  justify-content: center;
  color: var(--text-secondary);
}
.f-ctx-item.danger .f-ctx-icon {
  color: var(--danger);
}
.f-ctx-label {
  flex: 1;
}
.f-ctx-shortcut {
  color: var(--text-tertiary);
  font-size: 12px;
  margin-left: 24px;
}
.f-ctx-sep {
  height: 1px;
  background: var(--stroke-divider);
  margin: 4px 8px;
}
</style>
