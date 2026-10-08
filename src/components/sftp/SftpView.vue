<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { SessionTab, DirEntry } from '../../api/types'
import FilePane from './FilePane.vue'
import type { TransferRequest } from './FilePane.vue'
import TransferQueue from './TransferQueue.vue'
import { useTransfersStore } from '../../stores/transfers'
import { fsStat, mockSpecialDirs } from '../../api/backend'
import { isTauri } from '../../api/adapter'
import { useOverwriteConfirm } from '../../ui/overwriteConfirm'
import { joinLocalPath, LOCAL_SEP } from '../../api/localpath'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { formatBytes } from '../../utils/format'
import { toast, FConfirmDialog } from '../../ui'
import { t } from '../../i18n'

const props = defineProps<{ tab: SessionTab }>()
const transfers = useTransfersStore()

/** 覆盖确认（上传/下载目标端存在同名条目时先弹框） */
const {
  open: owOpen,
  message: owMessage,
  ensure: ensureOverwrite,
  settle: settleOverwrite,
} = useOverwriteConfirm()

const remotePane = ref<InstanceType<typeof FilePane>>()
const leftPane = ref<InstanceType<typeof FilePane>>()

/** 快捷下载（工具菜单）使用：远程当前路径与选中条目 */
function getRemotePath(): string {
  return rightPath.value
}
function getRemoteSelection(): DirEntry[] {
  return remotePane.value?.getSelectedEntries() ?? []
}
defineExpose({ getRemotePath, getRemoteSelection })

const splitRatio = ref(0.5)
const panesEl = ref<HTMLElement>()
const leftCount = ref({ files: 0, dirs: 0 })
const rightCount = ref({ files: 0, dirs: 0 })
const leftSel = ref<DirEntry[]>([])
const rightSel = ref<DirEntry[]>([])
/** 远程面板选中的文件总大小；选中文件夹或未选中时为 null（状态栏不显示） */
const selSize = ref<number | null>(null)

function onLocalSelection(entries: DirEntry[]) {
  leftSel.value = entries
}

function onRemoteSelection(entries: DirEntry[]) {
  rightSel.value = entries
  const real = entries.filter((e) => e.name !== '..')
  if (!real.length || real.some((e) => e.kind === 'dir')) {
    selSize.value = null
    return
  }
  selSize.value = real.reduce((sum, e) => sum + (e.size || 0), 0)
}

/** 状态栏汇总：窗格有选中 → 显示选中统计；无选中 → 显示当前目录统计 */
function summarize(count: { files: number; dirs: number }, sel: DirEntry[]) {
  if (!sel.length) return { dirs: count.dirs, files: count.files, selected: false }
  const dirs = sel.filter((e) => e.kind === 'dir').length
  return { dirs, files: sel.length - dirs, selected: true }
}
const leftSummary = computed(() => summarize(leftCount.value, leftSel.value))
const rightSummary = computed(() => summarize(rightCount.value, rightSel.value))
const leftPath = ref(mockSpecialDirs.desktop)
const rightPath = ref('/root')

const dividerDragging = ref(false)

function onDividerDown() {
  dividerDragging.value = true
  const move = (ev: MouseEvent) => {
    const el = panesEl.value
    if (!el) return
    const rect = el.getBoundingClientRect()
    splitRatio.value = Math.min(0.8, Math.max(0.2, (ev.clientX - rect.left) / rect.width))
  }
  const up = () => {
    dividerDragging.value = false
    window.removeEventListener('mousemove', move)
    window.removeEventListener('mouseup', up)
  }
  window.addEventListener('mousemove', move)
  window.addEventListener('mouseup', up)
}

/** 拖拽/右键传输请求：计算方向、检查目标端同名冲突后入队（tauri 走后端真实通道，浏览器为模拟） */
async function onTransfer(req: TransferRequest) {
  // 远程方向需要真实通道：断开时拦截并提示
  if (req.from === 'remote' && props.tab.status !== 'connected') {
    toast(
      t(props.tab.status === 'disconnected' ? 'sftp.disconnectedToast' : 'sftp.notConnected'),
      'error',
    )
    return
  }
  const entries = req.entries.filter((e) => e.name !== '..')
  if (!entries.length) return
  const isDownload = req.from === 'remote'
  const items = entries.map((entry) => {
    const isDir = entry.kind === 'dir'
    return {
      sid: props.tab.sid,
      name: entry.name,
      isDir,
      direction: (isDownload ? 'download' : 'upload') as 'download' | 'upload',
      localPath: isDownload
        ? joinLocalPath(leftPath.value, entry.name) + (isDir ? LOCAL_SEP : '')
        : joinLocalPath(req.fromPath, entry.name),
      remotePath: isDownload
        ? `${req.fromPath === '/' ? '' : req.fromPath}/${entry.name}${isDir ? '/' : ''}`
        : `${rightPath.value}/${entry.name}${isDir ? '/' : ''}`,
      total: entry.size || (isDir ? (isDownload ? 12_582_912 : 8_388_608) : 1_048_576),
    }
  })
  // 目标端已存在同名条目 → 确认后才继续（用户取消则整批不入队）
  if (!(await ensureOverwrite(items))) return
  for (const it of items) transfers.submit(it)
  toast(t(isDownload ? 'sftp.paneLoadingToast' : 'sftp.paneLoadingToastUp', { n: entries.length }), 'success')
}

/* ---------- 系统文件拖入 ----------
 * dragDropEnabled:true 时 OS 拖放被 Tauri 拦截，HTML5 File 对象不再出现，
 * 由 onDragDropEvent 提供真实文件路径。落在远程栏 → 真实路径上传；
 * 落在本地栏 → 单个目录直接定位过去。 */

type DragSide = 'local' | 'remote'

function hitPane(x: number, y: number): DragSide | null {
  const pairs: Array<[DragSide, InstanceType<typeof FilePane> | undefined]> = [
    ['local', leftPane.value],
    ['remote', remotePane.value],
  ]
  for (const [side, pane] of pairs) {
    const el = pane?.rootEl
    if (!el) continue
    const r = el.getBoundingClientRect()
    if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return side
  }
  return null
}

function setOver(side: DragSide | null) {
  leftPane.value?.setExternalOver(side === 'local')
  remotePane.value?.setExternalOver(side === 'remote')
}

async function onExternalDrop(side: DragSide, paths: string[]) {
  if (!paths.length) return
  if (side === 'remote') {
    const tab = props.tab
    if (tab.status !== 'connected' || tab.sid === undefined) {
      toast(
        t(tab.status === 'disconnected' ? 'sftp.disconnectedToast' : 'sftp.notConnected'),
        'error',
      )
      return
    }
    let n = 0
    const base = rightPath.value === '/' ? '' : rightPath.value
    const items: {
      sid: number
      name: string
      isDir: boolean
      direction: 'upload'
      localPath: string
      remotePath: string
      total: number
    }[] = []
    for (const p of paths) {
      let name = p.split(/[\\/]/).filter(Boolean).pop() || p
      let isDir = false
      let size = 0
      try {
        const st = await fsStat(p)
        if (st.name) name = st.name
        isDir = st.kind === 'dir'
        size = st.size || 0
      } catch {
        /* stat 失败仍入队，由后端给出真实错误 */
      }
      items.push({
        sid: tab.sid,
        name,
        isDir,
        direction: 'upload',
        localPath: p,
        remotePath: `${base}/${name}${isDir ? '/' : ''}`,
        total: size,
      })
      n++
    }
    // 远程端已存在同名条目 → 确认后才继续
    if (n && !(await ensureOverwrite(items))) return
    for (const it of items) await transfers.submit(it)
    if (n) toast(t('sftp.dropUploadQueued', { n }), 'success')
  } else {
    const st = paths.length === 1 ? await fsStat(paths[0]!).catch(() => null) : null
    if (st?.kind === 'dir') {
      leftPane.value?.navigateTo(paths[0]!)
      return
    }
    toast(t('sftp.dropLocalHint', { n: paths.length }), 'info')
  }
}

/* ---------- 窗格间行拖拽（鼠标模拟，事件来自 FilePane） ---------- */

function onPaneDragMove(x: number, y: number, from: 'local' | 'remote') {
  const side = hitPane(x, y)
  setOver(side && side !== from ? side : null)
}

function onPaneDragDrop(x: number, y: number, payload: TransferRequest) {
  const side = hitPane(x, y)
  setOver(null)
  if (side && side !== payload.from) onTransfer(payload)
}

/* ---------- 传输完成自动刷新对应窗格 ---------- */

const refreshedDone = new Set<string>()

watch(
  () => transfers.items.map((i) => `${i.id}:${i.status}`).join('|'),
  () => {
    let up = false
    let down = false
    for (const it of transfers.items) {
      if (it.status !== 'done' || refreshedDone.has(it.id) || it.sid !== props.tab.sid) continue
      if (it.direction === 'upload') {
        const base = `${rightPath.value.replace(/\/+$/, '')}/`
        if (rightPath.value === '/' || it.remotePath.startsWith(base)) {
          refreshedDone.add(it.id)
          up = true
        }
      } else {
        const base = `${leftPath.value.replace(/[\\/]+$/, '')}${LOCAL_SEP}`
        if (it.localPath.toLowerCase().startsWith(base.toLowerCase())) {
          refreshedDone.add(it.id)
          down = true
        }
      }
    }
    if (up) remotePane.value?.reload()
    if (down) leftPane.value?.reload()
  },
)

let unlistenDrag: (() => void) | null = null

onMounted(async () => {
  if (!isTauri) return
  try {
    unlistenDrag = await getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload
      if (p.type === 'leave') {
        setOver(null)
        return
      }
      // 事件坐标为物理像素，换算为 CSS 逻辑像素后做命中判定
      const dpr = window.devicePixelRatio || 1
      const side = hitPane(p.position.x / dpr, p.position.y / dpr)
      if (p.type === 'drop') {
        setOver(null)
        if (side) void onExternalDrop(side, p.paths)
      } else {
        setOver(side)
      }
    })
  } catch {
    /* 事件不可用时保持 HTML5 拖放路径 */
  }
})

onUnmounted(() => {
  unlistenDrag?.()
  unlistenDrag = null
})
</script>

<template>
  <div class="sftp-view">
    <div ref="panesEl" class="panes">
      <FilePane
        ref="leftPane"
        side="local"
        :style="{ width: `calc(${splitRatio * 100}% - 5px)` }"
        @transfer-request="onTransfer"
        @counts-changed="(c) => (leftCount = c)"
        @selection-changed="onLocalSelection"
        @path-changed="(p) => (leftPath = p)"
        @drag-move="onPaneDragMove"
        @drag-drop="onPaneDragDrop"
      />
      <div class="divider" :class="{ dragging: dividerDragging }" @mousedown="onDividerDown" />
      <FilePane
        ref="remotePane"
        side="remote"
        :session="tab"
        class="flex-1"
        @transfer-request="onTransfer"
        @counts-changed="(c) => (rightCount = c)"
        @path-changed="(p) => (rightPath = p)"
        @selection-changed="onRemoteSelection"
        @drag-move="onPaneDragMove"
        @drag-drop="onPaneDragDrop"
      />
    </div>

    <TransferQueue />

    <div class="sftp-status">
      <span class="ss-conn">
        <span class="ss-dot" :class="tab.status" />
        {{ tab.status === 'connected' ? t('sftp.connected', { addr: tab.address.replace('ssh://', '') }) : tab.status === 'connecting' ? t('sftp.connecting') : t('sftp.notConnected') }}
      </span>
      <div class="ss-spacer" />
      <span>{{ t('sftp.binary') }}</span>
      <span class="ss-pane" :class="{ sel: leftSummary.selected }">
        <span class="ss-name">{{ t('sftp.paneLocal') }}</span>
        <span>{{ t(leftSummary.selected ? 'sftp.selDirs' : 'sftp.dirsCount', { n: leftSummary.dirs }) }}</span>
        <span>{{ t(leftSummary.selected ? 'sftp.selFiles' : 'sftp.filesCount', { n: leftSummary.files }) }}</span>
      </span>
      <span class="ss-pane" :class="{ sel: rightSummary.selected }">
        <span class="ss-name">{{ t('sftp.paneRemote') }}</span>
        <span>{{ t(rightSummary.selected ? 'sftp.selDirs' : 'sftp.dirsCount', { n: rightSummary.dirs }) }}</span>
        <span>{{ t(rightSummary.selected ? 'sftp.selFiles' : 'sftp.filesCount', { n: rightSummary.files }) }}</span>
      </span>
      <span v-if="selSize !== null" class="ss-size">{{ formatBytes(selSize) }}</span>
    </div>

    <!-- 覆盖确认：上传/下载目标端存在同名条目时 -->
    <FConfirmDialog
      :open="owOpen"
      :title="t('sftp.overwriteTitle')"
      :message="owMessage"
      :confirm-text="t('sftp.overwriteBtn')"
      @update:open="(v: boolean) => !v && settleOverwrite(false)"
      @confirm="settleOverwrite(true)"
    />
  </div>
</template>

<style scoped>
.sftp-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 6px 8px 0;
  gap: 0;
  min-height: 0;
}
.panes {
  flex: 1;
  display: flex;
  min-height: 0;
}
.divider {
  width: 10px;
  cursor: col-resize;
  flex: none;
  position: relative;
}
.divider::after {
  content: '';
  position: absolute;
  left: 4.5px;
  top: 8px;
  bottom: 8px;
  width: 1px;
  background: transparent;
}
.divider:hover::after,
.divider.dragging::after {
  background: var(--accent);
}
.sftp-status {
  display: flex;
  align-items: center;
  gap: 16px;
  height: 24px;
  font-size: 11.5px;
  color: var(--text-secondary);
  flex: none;
  padding: 0 4px 2px;
}
.ss-conn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.ss-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--text-tertiary);
}
.ss-dot.connected {
  background: #2ecc71;
}
.ss-dot.connecting {
  background: #f1c40f;
}
.ss-spacer {
  flex: 1;
}
.ss-size {
  color: var(--text-primary);
  font-weight: 600;
}
/* 窗格统计组：有选中时整体高亮为 accent，与"目录统计"区分 */
.ss-pane {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}
.ss-pane.sel {
  color: var(--accent);
}
.ss-pane .ss-name {
  color: var(--text-tertiary);
}
</style>
