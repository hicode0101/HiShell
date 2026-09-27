/**
 * 全局右键菜单状态：任意组件调用 showContextMenu(e, items)，
 * 由 App.vue 挂载的 <FContextMenuHost/> 统一渲染（Fluent flyout 样式）。
 */
import { reactive } from 'vue'

export interface MenuItem {
  label?: string
  icon?: string
  shortcut?: string
  danger?: boolean
  disabled?: boolean
  separator?: boolean
  checked?: boolean
  /** 悬停提示（收藏路径等长文本用） */
  title?: string
  /** 等宽字体渲染 label（路径/代码类条目） */
  mono?: boolean
  /** 提供时在条目右侧渲染 × 删除按钮；点击不关闭菜单，便于连续删除 */
  onRemove?: () => void
  onClick?: () => void
}

export const contextMenuState = reactive({
  open: false,
  x: 0,
  y: 0,
  items: [] as MenuItem[],
})

export function showContextMenu(e: MouseEvent, items: MenuItem[]) {
  e.preventDefault?.()
  contextMenuState.items = items
  contextMenuState.open = true
  // 等渲染后再定位，防止溢出屏幕
  requestAnimationFrame(() => {
    const el = document.getElementById('f-context-menu')
    if (!el) return
    const rect = el.getBoundingClientRect()
    contextMenuState.x = Math.max(4, Math.min(e.clientX, window.innerWidth - rect.width - 8))
    contextMenuState.y = Math.max(4, Math.min(e.clientY, window.innerHeight - rect.height - 8))
  })
}

export function hideContextMenu() {
  contextMenuState.open = false
}
