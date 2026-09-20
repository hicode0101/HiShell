/** 全局 Toast 通知（右下角，Fluent 样式） */
import { reactive } from 'vue'

export interface ToastItem {
  id: number
  text: string
  kind: 'info' | 'success' | 'error'
}

let nextId = 1

export const toastState = reactive({
  items: [] as ToastItem[],
})

export function toast(text: string, kind: ToastItem['kind'] = 'info') {
  const id = nextId++
  toastState.items.push({ id, text, kind })
  setTimeout(() => {
    const idx = toastState.items.findIndex((t) => t.id === id)
    if (idx >= 0) toastState.items.splice(idx, 1)
  }, 2600)
}
