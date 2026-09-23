/**
 * 列表列宽拖拽调整（共享工具）。
 * 用法：表头与行容器绑定 gridStyle；每个可调列头内放
 * <i class="col-grip" @mousedown="startResize(i, $event)" />，
 * 表头单元格用 var(--stroke-strong) 右边框做灰色分割线。
 */
import { computed, ref, type CSSProperties } from 'vue'

export function useColResize(widths: number[], mins: number[] = []) {
  const colWidths = ref<number[]>([...widths])
  const resizing = ref(-1)
  let startX = 0
  let startW = 0

  /** 前若干列为固定宽度，末列吸收剩余空间（1fr） */
  const gridStyle = computed<CSSProperties>(() => ({
    gridTemplateColumns: colWidths.value.map((w) => `${w}px`).join(' ') + ' 1fr',
  }))

  function onMove(e: MouseEvent) {
    const i = resizing.value
    if (i < 0) return
    colWidths.value[i] = Math.max(mins[i] ?? 40, startW + e.clientX - startX)
  }

  function stop() {
    resizing.value = -1
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', stop)
  }

  function startResize(i: number, e: MouseEvent) {
    resizing.value = i
    startX = e.clientX
    startW = colWidths.value[i]
    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', stop)
    e.preventDefault()
    e.stopPropagation()
  }

  return { colWidths, gridStyle, resizing, startResize }
}
