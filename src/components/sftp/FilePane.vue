<script setup lang="ts">
/**
 * 文件窗格：本地 / 远程通用。
 * - 本地：真实列目录（Tauri）/ mock 文件树（浏览器）
 * - 远程：真实 SFTP（sftp_list），连接前显示提示
 * - 拖拽：行可拖拽 → 落到对侧窗格入传输队列；系统文件拖入由 SftpView 经
 *   Tauri drag-drop 事件处理（真实路径），浏览器预览模式仍走 HTML5 files
 * - 右键：下载/上传、编辑、重命名、删除、属性、新建文件夹、双击打开（远程文件先下载到临时目录）
 * - 键盘：Delete 删除选中项；打印字符键入定位（type-ahead），按当前排序前缀匹配并滚动选中
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { DirEntry, SessionTab } from '../../api/types'
import {
  listLocalDir,
  listRemoteDir,
  specialDir,
  mockSpecialDirs,
  fsRename,
  fsDelete,
  fsMkdir,
  fsStat,
  openPath,
  sftpRename,
  sftpRm,
  sftpMkdir,
  sftpStat,
  sftpChmod,
  type FileStat,
  type RemoteStat,
} from '../../api/backend'
import { isTauri } from '../../api/adapter'
import { joinLocalPath } from '../../api/localpath'
import { useTransfersStore } from '../../stores/transfers'
import { useSettingsStore } from '../../stores/settings'
import { formatBytes, formatTime, fileKindLabel } from '../../utils/format'
import { t } from '../../i18n'
import { FButton, FIcon, FTextBox, FDialog, showContextMenu, contextMenuState, toast, useColResize } from '../../ui'
import type { MenuItem } from '../../ui'
import FileEditDialog from './FileEditDialog.vue'

const props = defineProps<{
  side: 'local' | 'remote'
  session?: SessionTab
}>()

const emit = defineEmits<{
  (e: 'transfer-request', payload: TransferRequest): void
  (e: 'counts-changed', counts: { files: number; dirs: number }): void
  (e: 'path-changed', path: string): void
  (e: 'selection-changed', entries: DirEntry[]): void
  (e: 'drag-move', x: number, y: number, from: 'local' | 'remote'): void
  (e: 'drag-drop', x: number, y: number, payload: TransferRequest): void
}>()


export interface TransferRequest {
  entries: DirEntry[]
  /** 来源窗格 */
  from: 'local' | 'remote'
  fromPath: string
}

const path = ref('')
const entries = ref<DirEntry[]>([])
const loading = ref(false)
const loadError = ref('')
const selected = ref(new Set<string>())
const lastSelected = ref('')
const sortKey = ref<'name' | 'size' | 'type' | 'mtime'>('name')
const sortAsc = ref(true)
const dragOver = ref(false)

const backStack = ref<string[]>([])
const forwardStack = ref<string[]>([])

/* ---------- 文件操作对话框（重命名/新建/删除确认/属性） ---------- */
const transfers = useTransfersStore()

type DialogKind = 'rename' | 'newFolder' | 'delete' | 'props'
const dialog = ref<DialogKind | null>(null)
const dialogBusy = ref(false)
const dialogName = ref('')
const dialogTargets = ref<DirEntry[]>([])
const propsData = ref<Partial<FileStat & RemoteStat> | null>(null)
const propsLoading = ref(false)

const deleteTargets = computed(() => dialogTargets.value.filter((x) => x.name !== '..'))

function targetPath(entry: DirEntry): string {
  // 盘符等特殊条目：显示名 ≠ 实际路径，直接用后端携带的真实路径
  return entry.path ?? joinPath(path.value, entry.name)
}

function openRename(entry: DirEntry) {
  if (!ensureRemote()) return
  dialogTargets.value = [entry]
  dialogName.value = entry.name
  dialog.value = 'rename'
}

function openNewFolder() {
  if (!ensureRemote()) return
  dialogTargets.value = []
  dialogName.value = t('sftp.newFolderDefault')
  dialog.value = 'newFolder'
}

function openDelete(entry: DirEntry) {
  if (!ensureRemote()) return
  const sel = entries.value.filter((x) => selected.value.has(x.name) && x.name !== '..')
  dialogTargets.value = sel.length > 1 && sel.some((x) => x.name === entry.name) ? sel : [entry]
  dialog.value = 'delete'
}

async function openProps(entry: DirEntry) {
  if (!ensureRemote()) return
  dialog.value = 'props'
  propsLoading.value = true
  propsData.value = null
  try {
    if (!isTauri) {
      propsData.value = {
        name: entry.name,
        kind: entry.kind,
        size: entry.size,
        mtime: entry.mtime,
        attrs: entry.attrs ?? '',
        readonly: false,
      }
      return
    }
    const full = targetPath(entry)
    if (props.side === 'remote' && props.session?.sid !== undefined) {
      propsData.value = await sftpStat(props.session.sid, full)
    } else {
      propsData.value = await fsStat(full)
    }
  } catch (e) {
    toast(String(e), 'error')
    dialog.value = null
  } finally {
    propsLoading.value = false
  }
}

async function applyRename() {
  const entry = dialogTargets.value[0]
  const newName = dialogName.value.trim()
  if (!entry || !newName || newName === entry.name) {
    dialog.value = null
    return
  }
  dialogBusy.value = true
  try {
    if (props.side === 'remote') {
      await sftpRename(props.session!.sid!, targetPath(entry), joinPath(path.value, newName))
    } else {
      await fsRename(targetPath(entry), joinPath(path.value, newName))
    }
    dialog.value = null
    reload()
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    dialogBusy.value = false
  }
}

async function applyNewFolder() {
  const name = dialogName.value.trim()
  if (!name) return
  dialogBusy.value = true
  try {
    const full = joinPath(path.value, name)
    if (props.side === 'remote') {
      await sftpMkdir(props.session!.sid!, full)
    } else {
      await fsMkdir(full)
    }
    dialog.value = null
    reload()
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    dialogBusy.value = false
  }
}

async function applyDelete() {
  dialogBusy.value = true
  try {
    for (const entry of deleteTargets.value) {
      if (props.side === 'remote') {
        await sftpRm(props.session!.sid!, targetPath(entry))
      } else {
        await fsDelete(targetPath(entry))
      }
    }
    toast(
      t('sftp.deletedToast', { n: deleteTargets.value.length }),
      'success',
    )
    dialog.value = null
    reload()
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    dialogBusy.value = false
  }
}

const paneTitle = computed(() => (props.side === 'local' ? t('sftp.local') : props.session?.title || t('sftp.remote')))
const paneTabLabel = computed(() =>
  props.side === 'local' ? `${paneTitle.value}` : `${paneTitle.value}（${currentDirName.value}）`,
)
const currentDirName = computed(() => {
  if (!path.value) return t('sftp.thisPC')
  const norm = path.value.replace(/[\\/]+$/, '')
  const idx = Math.max(norm.lastIndexOf('\\'), norm.lastIndexOf('/'))
  return idx >= 0 ? norm.slice(idx + 1) : norm
})

const quickLinks = [
  { labelKey: 'sftp.quick.desktop', kind: 'desktop', icon: 'monitor' },
  { labelKey: 'sftp.quick.documents', kind: 'documents', icon: 'file-text' },
  { labelKey: 'sftp.quick.downloads', kind: 'downloads', icon: 'download' },
  { labelKey: 'sftp.quick.pictures', kind: 'pictures', icon: 'file' },
  { labelKey: 'sftp.quick.home', kind: 'home', icon: 'server' },
] as const

/** 本机是否 Windows（决定此电脑视图形式：盘符列表 / 根目录） */
const isWindows = /windows/i.test(navigator.userAgent)
/** 磁盘分区列表（本地窗格侧栏快捷切换 + 此电脑视图，path 为真实跳转路径） */
const drives = ref<DirEntry[]>([])

/** 此电脑 / 根目录：Windows 进入盘符列表视图，Linux/macOS 直接到 / */
function gotoThisPc() {
  if (isWindows) load('')
  else load('/')
}

/* ---------- 隐藏文件显示（仅远程面板） ---------- */

const showHidden = ref(false)

function toggleShowHidden() {
  if (!ensureRemote()) return
  showHidden.value = !showHidden.value
  reload()
}

/* ---------- 列宽拖拽调整 ---------- */
const rootEl = ref<HTMLElement>()
const { gridStyle, resizing, startResize } = useColResize(
  props.side === 'remote' ? [190, 80, 110, 170] : [230, 80, 110],
  [100, 56, 64, 90],
)

/** 点击列表空白处（非文件行）时清除选中状态 */
function onBodyClick(e: MouseEvent) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  if ((e.target as HTMLElement).closest('.ft-row')) return
  if (selected.value.size) {
    selected.value = new Set()
    lastSelected.value = ''
  }
}

// 选中变化时通知父组件（远程面板用于状态栏显示选中文件大小）
watch(selected, () => {
  emit(
    'selection-changed',
    entries.value.filter((x) => selected.value.has(x.name) && x.name !== '..'),
  )
})

/* ---------- 远程操作守卫：连接断开后拦截一切远程文件操作 ----------
 * - disconnected：阻断并弹右下角提示（用户显式操作）
 * - connecting：仅静默放行导航类初始加载（挂载时握手未完成，失败由既有的
 *   窗格错误展示），操作类（重命名/删除等）一律阻断
 * - 重连成功后守卫自动放行，watch(connected) 会重新加载目录 */

function toastDisconnected() {
  toast(t('sftp.disconnectedToast'), 'error')
}

/** 操作类守卫：非已连接一律阻断 */
function ensureRemote(): boolean {
  if (props.side === 'local') return true
  if (props.session?.status === 'connected') return true
  if (props.session?.status === 'disconnected') toastDisconnected()
  return false
}

/** 导航类守卫：断开状态阻断并提示；connecting 放行（初始加载） */
function navAllowed(): boolean {
  if (props.side === 'local') return true
  if (props.session?.status === 'disconnected') {
    toastDisconnected()
    return false
  }
  return true
}

/* ---------- 加载 ---------- */

async function load(p: string, pushBack = true) {
  if (!navAllowed()) return
  loading.value = true
  selected.value = new Set()
  typeaheadBuf = ''
  try {
    const list =
      props.side === 'local'
        ? await listLocalDir(p)
        : await listRemoteDir(props.session?.sid, p, props.session?.status === 'connected')
    // 远程面板：未勾选「显示隐藏」时过滤 dot 开头的隐藏项；
    // 非根目录时在最前插入 '..'（双击返回上一级）
    let shown =
      props.side === 'remote' && !showHidden.value
        ? list.filter((x) => x.name === '..' || !x.name.startsWith('.'))
        : list
    if (props.side === 'remote' && p !== '/' && p !== '') {
      shown = [{ name: '..', kind: 'dir', size: 0, mtime: 0 }, ...shown]
    }
    entries.value = shown
    loadError.value = ''
    if (pushBack && p !== path.value) {
      backStack.value.push(path.value)
      forwardStack.value = []
    }
    path.value = p
    emit('path-changed', p)
    emit('counts-changed', {
      files: shown.filter((x) => x.kind !== 'dir' && x.name !== '..').length,
      dirs: shown.filter((x) => x.kind === 'dir' && x.name !== '..').length,
    })
  } catch (e) {
    entries.value = []
    loadError.value = String(e)
  } finally {
    loading.value = false
  }
}

function reload() {
  load(path.value, false)
}

onMounted(async () => {
  if (props.side === 'local') {
    const desktop = await specialDir('desktop').catch(() => '')
    load(desktop || mockSpecialDirs.desktop || '')
    // 磁盘分区列表（侧栏快捷切换 + 此电脑视图）
    listLocalDir('')
      .then((d) => (drives.value = d))
      .catch(() => {})
  } else {
    load('/root', false)
  }
})

// 远程窗格：会话变为已连接时自动（重新）加载真实目录
watch(
  () => props.session?.status,
  (st) => {
    if (props.side === 'remote' && st === 'connected') reload()
  },
)

watch(
  () => props.session?.id,
  () => {
    if (props.side === 'remote') load('/root', false)
  },
)

/* ---------- 导航 ---------- */

function goUp() {
  const p = path.value
  if (props.side === 'remote') {
    if (p === '' || p === '/') return
    const parent = p.slice(0, p.replace(/\/$/, '').lastIndexOf('/')) || '/'
    load(parent === '/' ? '/' : parent)
  } else {
    const norm = p.replace(/[\\/]+$/, '')
    if (!norm) return
    const idx = Math.max(norm.lastIndexOf('\\'), norm.lastIndexOf('/'))
    if (idx < 0) {
      // 已是盘符根（C:）或其它顶级：进入此电脑视图
      load('')
    } else if (isWindows && /^[a-zA-Z]:$/.test(norm.slice(0, idx))) {
      // C:\Users 上级是 C:，补全为 C:\（避免 Windows 盘符相对路径语义）
      load(norm.slice(0, idx) + '\\')
    } else if (!isWindows && idx === 0) {
      // Linux/macOS：/home 上级是 /
      load('/')
    } else {
      load(norm.slice(0, idx))
    }
  }
}
function goBack() {
  if (!backStack.value.length) return
  const prev = backStack.value.pop()!
  forwardStack.value.push(path.value)
  load(prev, false)
}
function goForward() {
  if (!forwardStack.value.length) return
  const next = forwardStack.value.pop()!
  backStack.value.push(path.value)
  load(next, false)
}
function goto(p: string) {
  load(p)
}

/** 快速链接：解析系统特殊目录后跳转 */
async function gotoSpecial(kind: string) {
  const target = await specialDir(kind).catch(() => '')
  if (target) load(target)
}

/* ---------- 选择 / 排序 ---------- */

function onRowClick(e: MouseEvent, entry: DirEntry, idx: number) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  // Shift+点击：从锚点（首个普通点击项）到当前行之间全部选中。
  // idx 是显示顺序（sortedEntries）中的下标，范围必须按同一顺序计算；
  // 锚点不随 shift+点击移动，连续扩展始终以首个点击项为基准。
  if (e.shiftKey && lastSelected.value) {
    const start = sortedEntries.value.findIndex((x) => x.name === lastSelected.value)
    if (start >= 0) {
      const range = sortedEntries.value.slice(Math.min(start, idx), Math.max(start, idx) + 1)
      const next = e.ctrlKey ? new Set(selected.value) : new Set<string>()
      range.forEach((x) => next.add(x.name))
      selected.value = next
      return
    }
  }
  if (e.ctrlKey) {
    const next = new Set(selected.value)
    if (next.has(entry.name)) next.delete(entry.name)
    else next.add(entry.name)
    selected.value = next
  } else {
    selected.value = new Set([entry.name])
  }
  lastSelected.value = entry.name
}

function setSort(key: typeof sortKey.value) {
  if (sortKey.value === key) sortAsc.value = !sortAsc.value
  else {
    sortKey.value = key
    sortAsc.value = true
  }
}

const sortedEntries = computed(() => {
  const dirFirst = (a: DirEntry, b: DirEntry) => (a.name === '..' ? -1 : b.name === '..' ? 1 : a.kind === b.kind ? 0 : a.kind === 'dir' ? -1 : 1)
  const list = [...entries.value]
  const dir = sortAsc.value ? 1 : -1
  list.sort((a, b) => {
    const d = dirFirst(a, b)
    if (d !== 0) return d
    switch (sortKey.value) {
      case 'size':
        return (a.size - b.size) * dir
      case 'mtime':
        return (a.mtime - b.mtime) * dir
      case 'type':
        return a.kind.localeCompare(b.kind) * dir
      default:
        return a.name.localeCompare(b.name, 'zh-CN') * dir
    }
  })
  return list
})

const counts = computed(() => ({
  files: entries.value.filter((e) => e.kind !== 'dir' && e.name !== '..').length,
  dirs: entries.value.filter((e) => e.kind === 'dir' && e.name !== '..').length,
}))

/* ---------- 行拖拽（鼠标模拟） ----------
 * dragDropEnabled:true 后 WebView2 拦截了全部 HTML5 拖拽（含面板之间），
 * 因此用 mousedown + mousemove 自行实现：超过位移阈值出现 ghost，
 * 松开时由 SftpView 按落点判定目标窗格。 */

const dragGhost = ref<{ x: number; y: number; label: string } | null>(null)
let pressEntries: DirEntry[] | null = null
let pressX = 0
let pressY = 0
let rowDragging = false
let suppressClick = false

function onRowMouseDown(e: MouseEvent, entry: DirEntry) {
  if (e.button !== 0) return
  // 自定义行拖拽用 mouse 事件模拟：先阻止原生文字选取（WebKit 选区手势一旦开始，
  // 中途设置 user-select 无法停止扩展）；preventDefault 会吞掉点击聚焦，手动归还窗格
  e.preventDefault()
  rootEl.value?.focus()
  const sel = entries.value.filter((x) => selected.value.has(x.name))
  const items = (sel.some((x) => x.name === entry.name) ? sel : [entry]).filter((x) => x.name !== '..')
  pressEntries = items
  pressX = e.clientX
  pressY = e.clientY
  rowDragging = false
  document.addEventListener('mousemove', onRowDragMove)
  document.addEventListener('mouseup', onRowDragUp)
}

function onRowDragMove(e: MouseEvent) {
  if (!pressEntries || !pressEntries.length) return
  if (!rowDragging) {
    if (Math.abs(e.clientX - pressX) + Math.abs(e.clientY - pressY) < 5) return
    rowDragging = true
    document.body.style.userSelect = 'none'
    window.getSelection()?.removeAllRanges()
  }
  dragGhost.value = {
    x: e.clientX,
    y: e.clientY,
    label: pressEntries.length > 1 ? t('sftp.dragItems', { n: pressEntries.length }) : pressEntries[0]!.name,
  }
  emit('drag-move', e.clientX, e.clientY, props.side)
}

function onRowDragUp(e: MouseEvent) {
  document.removeEventListener('mousemove', onRowDragMove)
  document.removeEventListener('mouseup', onRowDragUp)
  document.body.style.userSelect = ''
  const dragging = rowDragging
  rowDragging = false
  dragGhost.value = null
  const payload = pressEntries
  pressEntries = null
  if (dragging && payload && payload.length) {
    // 只吞掉 mouseup 后紧随的那次 click（拖拽跨元素时浏览器不会再派发行 click，
    // 定时复位避免残留标志吞掉用户下一次点击）
    suppressClick = true
    setTimeout(() => {
      suppressClick = false
    }, 0)
    emit('drag-drop', e.clientX, e.clientY, { entries: payload, from: props.side, fromPath: path.value })
  }
}

/* ---------- 空白处框选（rubber band）----------
 * 在列表空白区按住左键拖动画框，框住的行全部选中；Ctrl+拖动在已有选择上追加。
 * 行内的左键拖拽仍走上方"行拖拽（文件搬运）"逻辑，二者互不干扰。 */

const marquee = ref<{ left: number; top: number; width: number; height: number } | null>(null)
let mqBase: Set<string> | null = null
let mqStartX = 0
let mqStartY = 0
let mqActive = false

function onBodyMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  if ((e.target as HTMLElement).closest('.ft-row')) return
  const el = e.currentTarget as HTMLElement
  // 点在滚动条上不启动框选
  if (e.offsetX > el.clientWidth || e.offsetY > el.clientHeight) return
  // 阻止框选手势触发原生文字选取，焦点手动归还窗格
  e.preventDefault()
  rootEl.value?.focus()
  mqBase = e.ctrlKey ? new Set(selected.value) : null
  mqStartX = e.clientX
  mqStartY = e.clientY
  mqActive = false
  document.addEventListener('mousemove', onMarqueeMove)
  document.addEventListener('mouseup', onMarqueeUp)
}

function onMarqueeMove(e: MouseEvent) {
  if (!mqActive && Math.abs(e.clientX - mqStartX) + Math.abs(e.clientY - mqStartY) < 5) return
  if (!mqActive) {
    mqActive = true
    document.body.style.userSelect = 'none'
    window.getSelection()?.removeAllRanges()
  }
  const left = Math.min(mqStartX, e.clientX)
  const top = Math.min(mqStartY, e.clientY)
  marquee.value = { left, top, width: Math.abs(e.clientX - mqStartX), height: Math.abs(e.clientY - mqStartY) }
  // 视口坐标与各行矩形求交；DOM 行顺序与 sortedEntries 一致
  const mqR = left + marquee.value.width
  const mqB = top + marquee.value.height
  const next = mqBase ? new Set(mqBase) : new Set<string>()
  const rows = bodyEl.value ? (Array.from(bodyEl.value.children) as HTMLElement[]) : []
  let lastHit = ''
  rows.forEach((row, i) => {
    if (!row.classList.contains('ft-row')) return
    const r = row.getBoundingClientRect()
    if (r.bottom <= top || r.top >= mqB || r.right <= left || r.left >= mqR) return
    const hit = sortedEntries.value[i]
    if (!hit || hit.name === '..') return
    next.add(hit.name)
    lastHit = hit.name
  })
  selected.value = next
  if (lastHit) lastSelected.value = lastHit
}

function onMarqueeUp() {
  document.removeEventListener('mousemove', onMarqueeMove)
  document.removeEventListener('mouseup', onMarqueeUp)
  document.body.style.userSelect = ''
  marquee.value = null
  const was = mqActive
  mqActive = false
  mqBase = null
  if (was) {
    // 吞掉 mouseup 后落在空白处的 click，避免 onBodyClick 清掉刚框选的结果
    suppressClick = true
    setTimeout(() => {
      suppressClick = false
    }, 0)
  }
}

/** 系统外部文件拖入（仅浏览器预览模式；tauri 下由 SftpView 经 Tauri 事件处理） */
function onDrop(e: DragEvent) {  dragOver.value = false
  e.preventDefault()
  const files = Array.from(e.dataTransfer?.files ?? [])
  if (!files.length) return
  const entriesPayload: DirEntry[] = files.map((f) => ({
    name: f.name,
    kind: 'file',
    size: f.size,
    mtime: f.lastModified,
  }))
  emit('transfer-request', { entries: entriesPayload, from: props.side, fromPath: path.value })
}

onUnmounted(() => {
  document.removeEventListener('mousemove', onRowDragMove)
  document.removeEventListener('mouseup', onRowDragUp)
  document.removeEventListener('mousemove', onMarqueeMove)
  document.removeEventListener('mouseup', onMarqueeUp)
  document.body.style.userSelect = ''
})

/* ---------- 键盘：Delete 删除选中项 + 字母快速定位（type-ahead） ---------- */

function onPaneKeydown(e: KeyboardEvent) {
  if ((e.target as HTMLElement).closest('input, textarea, [contenteditable]')) return
  if (e.key === 'Delete') {
    e.preventDefault()
    deleteSelection()
    return
  }
  // Backspace 退回上一级：keydown 挂在每个窗格根元素上，焦点在哪个窗格就退哪个
  if (e.key === 'Backspace') {
    e.preventDefault()
    goUp()
    return
  }
  // 打印字符键：按名称前缀定位条目（资源管理器式），Ctrl/Alt/Meta 组合与输入法组合中不拦截
  if (!e.ctrlKey && !e.altKey && !e.metaKey && !e.isComposing && e.key.length === 1 && e.key !== ' ') {
    e.preventDefault()
    typeaheadChar(e.key)
  }
}

function deleteSelection() {
  // 排除 '..' 与磁盘根（盘符不可删除，后端亦拒绝）
  const targets = entries.value.filter((x) => selected.value.has(x.name) && x.name !== '..' && !x.path)
  if (!targets.length || dialog.value) return
  dialogTargets.value = targets
  dialog.value = 'delete'
}

/* ---------- 键入定位（type-ahead）----------
 * 在窗格获得焦点时按可打印字符检索：按当前排序在显示条目（不含 '..'）中
 * 前缀匹配文件/文件夹名，单选命中项并滚动到可视位置。
 * - 1 秒内连续按键累积前缀（"d" → "do" → "doc"）
 * - 同字符快速连按：在以该字符开头的条目间向下循环
 * - 累积无匹配则退回用新字符重新匹配，仍无匹配清空缓冲 */

const TYPEAHEAD_TIMEOUT = 1000
let typeaheadBuf = ''
let typeaheadAt = 0
const bodyEl = ref<HTMLElement>()

function typeaheadChar(c: string) {
  const now = Date.now()
  if (now - typeaheadAt > TYPEAHEAD_TIMEOUT) typeaheadBuf = ''
  typeaheadAt = now
  const lower = (s: string) => s.toLocaleLowerCase()
  // 当前显示顺序（已按用户排序），排除 '..' 伪条目
  const candidates = sortedEntries.value.filter((x) => x.name !== '..')
  if (!candidates.length) return

  const selectAt = (idx: number, buf: string) => {
    typeaheadBuf = buf
    const entry = candidates[idx]!
    selected.value = new Set([entry.name])
    lastSelected.value = entry.name
    // DOM 子元素顺序与 sortedEntries 一致（含列表头部的 '..' 伪行），
    // candidates 已排除 '..'，须按名称换算真实行索引后再滚动
    const domIdx = sortedEntries.value.findIndex((x) => x.name === entry.name)
    const el = bodyEl.value?.children[domIdx] as HTMLElement | undefined
    el?.scrollIntoView({ block: 'nearest' })
  }

  const combined = typeaheadBuf + c
  // 1) 累积前缀匹配（"d" → "do" → "doc"）
  const combinedIdx = candidates.findIndex((x) => lower(x.name).startsWith(lower(combined)))
  if (combinedIdx >= 0) {
    selectAt(combinedIdx, combined)
    return
  }
  // 2) 同字符连按：从当前选中处向下循环到下一个以该字符开头的条目（支持环绕）
  if (typeaheadBuf.length === 1 && typeaheadBuf.toLowerCase() === c.toLowerCase()) {
    const n = candidates.length
    const start = candidates.findIndex((x) => selected.value.has(x.name))
    for (let k = 1; k <= n; k++) {
      const idx = (start + k) % n
      if (lower(candidates[idx]!.name).startsWith(lower(c))) {
        selectAt(idx, c)
        return
      }
    }
  }
  // 3) 累积无匹配：退回单字符重新开始
  const freshIdx = candidates.findIndex((x) => lower(x.name).startsWith(lower(c)))
  if (freshIdx >= 0) {
    selectAt(freshIdx, c)
    return
  }
  // 4) 无匹配：清空缓冲等待下一次输入
  typeaheadBuf = ''
}

/* ---------- 右键菜单 ---------- */

function rowContextMenu(e: MouseEvent, entry: DirEntry) {
  // 右键选中规则：目标行不在选中集合中 → 仅选中它；已在（多）选中 → 保留整组选中。
  // 不能走 onRowClick——它的普通点击分支会把选中重置为单行，多选在右键瞬间塌缩。
  if (!selected.value.has(entry.name)) {
    selected.value = new Set([entry.name])
    lastSelected.value = entry.name
  }
  const targets = entries.value.filter((x) => selected.value.has(x.name))
  const realTargets = targets.filter((x) => x.name !== '..')
  const multi = realTargets.length > 1
  const isDotDot = entry.name === '..'
  const isDrive = !!entry.path
  const hasDrive = realTargets.some((x) => !!x.path)
  showContextMenu(e, [
    // 在线编辑：仅远程窗格的文件条目（多选时不适用）
    ...(props.side === 'remote' && entry.kind === 'file' && !isDotDot && !multi
      ? [{ label: t('sftp.edit'), icon: 'code', onClick: () => openEdit(entry) }]
      : []),
    {
      label: props.side === 'remote' ? t('sftp.download') : t('sftp.upload'),
      icon: props.side === 'remote' ? 'download' : 'upload',
      disabled: !realTargets.length || hasDrive,
      onClick: () => emit('transfer-request', { entries: realTargets, from: props.side, fromPath: path.value }),
    },
    { separator: true },
    // 重命名/属性只对单个条目有意义，多选时隐藏
    ...(multi
      ? []
      : [
          {
            label: t('sftp.rename'),
            icon: 'edit',
            disabled: isDotDot || isDrive,
            onClick: () => openRename(entry),
          },
        ]),
    {
      label: t('sftp.delete'),
      icon: 'trash',
      danger: true,
      disabled: isDotDot || isDrive,
      onClick: () => openDelete(entry),
    },
    ...(props.side === 'remote'
      ? [
          ...(!isDotDot
            ? [
                {
                  label: t('sftp.chmod'),
                  icon: 'lock',
                  onClick: () => openChmod(entry),
                },
              ]
            : []),
          {
            label: t('sftp.showHidden'),
            icon: showHidden.value ? 'eye' : 'eye-off',
            checked: showHidden.value,
            onClick: toggleShowHidden,
          },
        ]
      : []),
    { separator: true },
    // 新建文件夹作用于当前目录，与右键目标条目无关（'..'、盘符行上同样可用）
    {
      label: t('sftp.newFolder'),
      icon: 'folder',
      // 本地「此电脑」视图下没有当前目录，禁用以免按相对路径建错位置
      disabled: !path.value || (props.side === 'remote' && props.session?.status !== 'connected'),
      onClick: () => openNewFolder(),
    },
    ...(multi ? [] : [{ label: t('sftp.props'), icon: 'info', onClick: () => openProps(entry) }]),
  ])
}

function paneContextMenu(e: MouseEvent) {
  showContextMenu(e, [
    { label: t('common.refresh'), icon: 'refresh', onClick: reload },
    ...(props.side === 'remote'
      ? [
          {
            label: t('sftp.showHidden'),
            icon: showHidden.value ? 'eye' : 'eye-off',
            checked: showHidden.value,
            onClick: toggleShowHidden,
          },
          { separator: true },
        ]
      : []),
    {
      label: t('sftp.newFolder'),
      icon: 'folder',
      // 本地「此电脑」视图下没有当前目录，禁用以免按相对路径建错位置
      disabled: !path.value || (props.side === 'remote' && props.session?.status !== 'connected'),
      onClick: () => openNewFolder(),
    },
    { separator: true },
    { label: t('sftp.paste'), icon: 'copy', disabled: true },
  ])
}

/* ---------- 收藏常用路径（本地/远程各自独立，持久化于 settings） ----------
 * 路径栏末尾星形按钮：当前路径已收藏时呈金色实心；
 * 点击弹出菜单——首项收藏/取消收藏当前路径，下方列出全部收藏，
 * 点击跳转，条目右侧 × 删除（不关菜单，可连续删除）。 */

const settings = useSettingsStore()

const isFav = computed(() => !!path.value && settings.s.sftpFavorites[props.side].includes(path.value))

function toggleFavorite() {
  if (!path.value) return
  const added = settings.toggleSftpFavorite(props.side, path.value)
  toast(t(added ? 'sftp.favAdded' : 'sftp.favRemoved', { path: path.value }), 'success')
}

/** 依据当前收藏状态构建菜单（删除条目后就地重建，菜单保持打开） */
function favoriteMenuItems(): MenuItem[] {
  const side = props.side
  const list = settings.s.sftpFavorites[side]
  return [
    {
      label: isFav.value ? t('sftp.favRemove') : t('sftp.favAdd'),
      icon: isFav.value ? 'star-fill' : 'star',
      onClick: toggleFavorite,
    },
    ...(list.length
      ? [
          { separator: true },
          ...list.map((p) => ({
            label: p,
            icon: 'folder',
            mono: true,
            title: p,
            onClick: () => load(p),
            onRemove: () => {
              settings.removeSftpFavorite(side, p)
              toast(t('sftp.favRemoved', { path: p }), 'success')
              contextMenuState.items = favoriteMenuItems()
            },
          })),
        ]
      : [{ separator: true }, { label: t('sftp.favEmpty'), disabled: true }]),
  ]
}

function openFavorites(e: MouseEvent) {
  showContextMenu(e, favoriteMenuItems())
}

/* ---------- 更改权限（远程，参考 Xftp） ---------- */

const permDialog = ref(false)
const permBusy = ref(false)
const permName = ref('')
/** 应用权限的目标条目（多选时为全部选中项） */
const permTargets = ref<DirEntry[]>([])
const permIsDir = ref(false)
const permOctal = ref('644')
const permRecursive = ref(false)

const permInvalid = computed(() => !/^[0-7]{3}$/.test(permOctal.value))

const PERM_ROLES = [0, 1, 2] as const
const PERM_BITS = [0, 1, 2] as const
const PERM_SHIFTS = [6, 3, 0]
const PERM_MASKS = [4, 2, 1]

/** role: 0=所有者 1=组 2=其他；bit: 0=读 1=写 2=执行 */
function permChecked(role: 0 | 1 | 2, bit: 0 | 1 | 2): boolean {
  const v = parseInt(permOctal.value, 8)
  return Number.isNaN(v) ? false : (v >> PERM_SHIFTS[role]! & PERM_MASKS[bit]!) !== 0
}
function setPermBit(role: 0 | 1 | 2, bit: 0 | 1 | 2, on: boolean) {
  let v = parseInt(permOctal.value, 8)
  if (Number.isNaN(v)) v = 0
  const mask = PERM_MASKS[bit]! << PERM_SHIFTS[role]!
  v = on ? v | mask : v & ~mask
  permOctal.value = (v & 0o777).toString(8).padStart(3, '0')
}
function onOctalInput() {
  permOctal.value = permOctal.value.replace(/[^0-7]/g, '').slice(0, 3)
}

/** 从 "drwxr-xr-x" 形式的属性串解析八进制（缺省 644） */
function octalFromAttrs(attrs: string | undefined): string {
  const s = (attrs ?? '').slice(-9)
  if (s.length < 9) return '644'
  let v = 0
  for (let i = 0; i < 9; i++) {
    if (s[i] !== '-') v |= 1 << (8 - i)
  }
  return (v & 0o777).toString(8).padStart(3, '0')
}

async function openChmod(entry: DirEntry) {
  if (props.side !== 'remote') return
  if (!ensureRemote()) return
  // 多选时对全部选中条目生效；单选时即右键目标
  const sel = entries.value.filter((x) => selected.value.has(x.name) && x.name !== '..')
  permTargets.value = sel.length > 1 ? sel : [entry]
  permName.value = permTargets.value.length > 1
    ? t('sftp.chmodMultiName', { n: permTargets.value.length })
    : entry.name
  permIsDir.value = entry.kind === 'dir'
  permRecursive.value = false
  if (entry.attrs) {
    permOctal.value = octalFromAttrs(entry.attrs)
  } else {
    try {
      const st = await sftpStat(props.session!.sid!, targetPath(entry))
      permOctal.value = octalFromAttrs(st?.attrs)
    } catch {
      permOctal.value = '644'
    }
  }
  permDialog.value = true
}

/* ---------- 在线编辑远程文件（仅远程文件，≤10MB） ---------- */

const EDIT_MAX_BYTES = 10 * 1024 * 1024
const editDialogOpen = ref(false)
const editFile = ref<{ name: string; path: string; size: number } | null>(null)

function openEdit(entry: DirEntry) {
  if (!ensureRemote()) return
  if (isTauri && entry.size > EDIT_MAX_BYTES) {
    toast(t('sftp.editor.tooLarge'), 'error')
    return
  }
  editFile.value = { name: entry.name, path: targetPath(entry), size: entry.size }
  editDialogOpen.value = true
}

async function applyChmod() {
  if (!isTauri || props.session?.sid === undefined || permInvalid.value || permBusy.value) return
  permBusy.value = true
  try {
    const mode = parseInt(permOctal.value, 8)
    let n = 0
    for (const tgt of permTargets.value) {
      n += await sftpChmod(props.session.sid, targetPath(tgt), mode, permRecursive.value)
    }
    toast(t('sftp.chmodDone', { name: permName.value, n }), 'success')
    permDialog.value = false
    reload()
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    permBusy.value = false
  }
}

async function openEntry(entry: DirEntry) {
  if (entry.name === '..') {
    goUp()
    return
  }
  if (entry.kind === 'dir') {
    // 盘符等特殊条目（display name ≠ 真实路径）走 entry.path
    load(targetPath(entry))
    return
  }
  if (!isTauri) {
    // 浏览器预览模式无后端
    toast(t('sftp.openFileToast', { name: entry.name }))
    return
  }
  const full = targetPath(entry)
  if (props.side === 'local') {
    openPath(full).catch((e) => toast(String(e), 'error'))
    return
  }
  // 远程文件：下载到临时目录后用系统默认程序打开
  if (!ensureRemote()) return
  try {
    const temp = await specialDir('temp')
    const targetDir = joinLocalPath(temp, 'hishell-open')
    fsMkdir(targetDir).catch(() => {})
    const localPath = joinLocalPath(targetDir, entry.name)
    const id = await transfers.submit({
      sid: props.session!.sid!,
      name: entry.name,
      isDir: false,
      direction: 'download',
      localPath,
      remotePath: full,
      total: entry.size || 1,
    })
    if (!id) return
    toast(t('sftp.openDownloading', { name: entry.name }))
    waitForTransferDone(id, localPath)
  } catch (e) {
    toast(String(e), 'error')
  }
}

/** 等传输完成后用系统程序打开 */
function waitForTransferDone(id: string, localPath: string) {
  const deadline = Date.now() + 120_000
  const timer = window.setInterval(() => {
    const it = transfers.items.find((x) => x.id === id)
    if (!it) {
      window.clearInterval(timer)
      return
    }
    if (it.status === 'done') {
      window.clearInterval(timer)
      openPath(localPath).catch((e) => toast(String(e), 'error'))
    } else if (it.status === 'failed' || it.status === 'cancelled') {
      window.clearInterval(timer)
      toast(t('sftp.openDownloadFailed', { name: it.name }), 'error')
    } else if (Date.now() > deadline) {
      window.clearInterval(timer)
    }
  }, 300)
}

function joinPath(base: string, name: string): string {
  if (!base) return name
  return props.side === 'remote'
    ? (base === '/' ? '' : base) + '/' + name
    : joinLocalPath(base, name)
}

defineExpose({
  counts,
  /** 根元素：SftpView 用于 Tauri 系统拖放的落点命中判定 */
  rootEl,
  /** 系统文件拖入悬停高亮（Tauri 拦截了原生 HTML5 事件，由 SftpView 驱动） */
  setExternalOver: (v: boolean) => {
    dragOver.value = v
  },
  navigateTo: (p: string) => load(p),
  reload,
  /** 快捷下载等场景：获取当前选中的真实条目 */
  getSelectedEntries: () => entries.value.filter((x) => selected.value.has(x.name) && x.name !== '..'),
})
</script>

<template>
  <div
    ref="rootEl"
    class="file-pane"
    :class="{ 'drag-over': dragOver, loading, 'is-remote': side === 'remote' }"
    tabindex="0"
    @keydown="onPaneKeydown"
    @dragover.prevent="dragOver = true"
    @dragleave="dragOver = false"
    @drop="onDrop"
    @contextmenu="paneContextMenu"
  >
    <div class="pane-head">
      <span class="pane-tab" :class="side">
        <span class="pane-dot" />
        {{ paneTitle }}<template v-if="side === 'remote'"> · {{ currentDirName }}</template>
      </span>
    </div>

    <div class="pathbar">
      <button class="pb-btn" :title="t('sftp.back')" :disabled="!backStack.length" @click="goBack">
        <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M12.5 5L7.5 10l5 5" /></svg>
      </button>
      <button class="pb-btn" :title="t('sftp.forward')" :disabled="!forwardStack.length" @click="goForward">
        <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M7.5 5l5 5-5 5" /></svg>
      </button>
      <button class="pb-btn" :title="t('sftp.levelUp')" @click="goUp">
        <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M5 12.5L10 7.5l5 5" /></svg>
      </button>
      <div class="pb-path">
        <svg v-if="side === 'local'" width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="3" y="4.5" width="14" height="9.5" rx="1.2" /><path d="M7 17h6M10 14v3" /></svg>
        <svg v-else width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><rect x="3.5" y="3.5" width="13" height="5.5" rx="1" /><rect x="3.5" y="11" width="13" height="5.5" rx="1" /><path d="M6 6.25h.01M6 13.75h.01" /></svg>
        <input
          class="pb-input"
          :value="path || (side === 'local' ? t('sftp.thisPC') : '/')"
          spellcheck="false"
          @keydown.enter="goto(($event.target as HTMLInputElement).value.trim())"
        />
        <button class="pb-btn" :title="t('common.refresh')" @click="reload">
          <svg width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M16 10a6 6 0 1 1-1.76-4.24M16.5 3.5v3h-3" /></svg>
        </button>
      </div>
      <button
        class="pb-btn pb-fav"
        :class="{ active: isFav }"
        :title="t('sftp.favTitle')"
        :disabled="!path"
        @click.stop="openFavorites"
      >
        <FIcon :name="isFav ? 'star-fill' : 'star'" :size="15" />
      </button>
    </div>

    <div class="pane-body">
      <div v-if="side === 'local'" class="quick-links">
        <div
          class="quick-link"
          :title="isWindows ? t('sftp.thisPC') : t('sftp.rootDir')"
          @click="gotoThisPc"
        >
          <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <ellipse cx="10" cy="5.2" rx="6.6" ry="2.6" />
            <path d="M3.4 5.2v9.6c0 1.44 2.95 2.6 6.6 2.6s6.6-1.16 6.6-2.6V5.2M3.4 10c0 1.44 2.95 2.6 6.6 2.6s6.6-1.16 6.6-2.6" />
          </svg>
          <span>{{ isWindows ? t('sftp.thisPC') : t('sftp.rootDir') }}</span>
        </div>
        <div class="ql-sep" />
        <div
          v-for="ql in quickLinks"
          :key="ql.kind"
          class="quick-link"
          :title="t(ql.labelKey)"
          @click="gotoSpecial(ql.kind)"
        >
          <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
            <template v-if="ql.icon === 'monitor'"><rect x="3" y="4.5" width="14" height="9.5" rx="1.2" /><path d="M7 17h6M10 14v3" /></template>
            <template v-else-if="ql.icon === 'file-text'"><path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6M7.5 11.5h5M7.5 14h5" /></template>
            <template v-else-if="ql.icon === 'download'"><path d="M10 3.5v9M6.2 9.3l3.8 3.7 3.8-3.7M4.5 16.5h11" /></template>
            <template v-else-if="ql.icon === 'file'"><path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6" /></template>
            <template v-else><rect x="3.5" y="3.5" width="13" height="5.5" rx="1" /><rect x="3.5" y="11" width="13" height="5.5" rx="1" /><path d="M6 6.25h.01M6 13.75h.01" /></template>
          </svg>
          <span>{{ t(ql.labelKey) }}</span>
        </div>
        <template v-if="isWindows && drives.length">
          <div class="ql-sep" />
          <div
            v-for="d in drives"
            :key="d.path"
            class="quick-link"
            :title="`${d.name} — ${d.path}`"
            @click="load(d.path ?? d.name)"
          >
            <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
              <ellipse cx="10" cy="5.2" rx="6.6" ry="2.6" />
              <path d="M3.4 5.2v9.6c0 1.44 2.95 2.6 6.6 2.6s6.6-1.16 6.6-2.6V5.2M3.4 10c0 1.44 2.95 2.6 6.6 2.6s6.6-1.16 6.6-2.6" />
            </svg>
            <span>{{ d.path ?? d.name }}</span>
          </div>
        </template>
      </div>

      <div class="file-table-wrap">
        <div class="file-table">
          <div class="ft-head" :style="gridStyle">
            <span class="col-name" @click="setSort('name')">
              {{ t('sftp.colName') }}
              <span v-if="sortKey === 'name'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span>
              <i class="ft-grip" @mousedown.stop="startResize(0, $event)" @click.stop />
            </span>
            <span class="col-size" @click="setSort('size')">{{ t('sftp.colSize') }} <span v-if="sortKey === 'size'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span>
              <i class="ft-grip" @mousedown.stop="startResize(1, $event)" @click.stop />
            </span>
            <span class="col-type" @click="setSort('type')">{{ t('sftp.colType') }} <span v-if="sortKey === 'type'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span>
              <i class="ft-grip" @mousedown.stop="startResize(2, $event)" @click.stop />
            </span>
            <span class="col-mtime" @click="setSort('mtime')">{{ t('sftp.colMtime') }} <span v-if="sortKey === 'mtime'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span>
              <i v-if="side === 'remote'" class="ft-grip" @mousedown.stop="startResize(3, $event)" @click.stop />
            </span>
            <span v-if="side === 'remote'" class="col-attrs">{{ t('sftp.colAttrs') }}</span>
          </div>
          <div ref="bodyEl" class="ft-body" @click="onBodyClick" @mousedown="onBodyMouseDown">
            <div
              v-for="(entry, idx) in sortedEntries"
              :key="entry.name"
              class="ft-row"
              :class="{ selected: selected.has(entry.name), dir: entry.kind === 'dir' }"
              :style="gridStyle"
              @click="onRowClick($event, entry, idx)"
              @dblclick="openEntry(entry)"
              @mousedown="onRowMouseDown($event, entry)"
              @contextmenu.prevent.stop="rowContextMenu($event, entry)"
            >
              <span class="col-name">
                <svg v-if="entry.kind === 'dir'" width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" class="ic dir-ic">
                  <path d="M3.5 6.3c0-.7.6-1.3 1.3-1.3h3l1.8 1.8h5.6c.7 0 1.3.6 1.3 1.3v7.6c0 .7-.6 1.3-1.3 1.3H4.8c-.7 0-1.3-.6-1.3-1.3V6.3z" />
                </svg>
                <svg v-else width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" class="ic">
                  <path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6" />
                </svg>
                {{ entry.name }}
              </span>
              <span class="col-size">{{ entry.kind === 'dir' ? '' : formatBytes(entry.size) }}</span>
              <span class="col-type">{{ fileKindLabel(entry.kind, entry.name) }}</span>
              <span class="col-mtime">{{ formatTime(entry.mtime) }}</span>
              <span v-if="side === 'remote'" class="col-attrs mono">{{ entry.attrs || '' }}</span>
            </div>
            <div v-if="!sortedEntries.length && !loading" class="ft-empty">{{ loadError || t('sftp.emptyFolder') }}</div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="loading" class="pane-loading">{{ t('sftp.loading') }}</div>

    <!-- 空白处框选矩形 -->
    <div v-if="marquee" class="marquee" :style="{ left: marquee.left + 'px', top: marquee.top + 'px', width: marquee.width + 'px', height: marquee.height + 'px' }" />

    <!-- 重命名 / 新建文件夹 -->
    <FDialog
      :open="dialog === 'rename' || dialog === 'newFolder'"
      :title="dialog === 'rename' ? t('sftp.rename') : t('sftp.newFolder')"
      :width="420"
      autofocus-select
      @update:open="(v: boolean) => (!v && (dialog = null))"
    >
      <FTextBox
        v-model="dialogName"
        :placeholder="t('sftp.namePlaceholder')"
        spellcheck="false"
        @keydown.enter="dialog === 'rename' ? applyRename() : applyNewFolder()"
      />
      <template #footer>
        <FButton variant="subtle" @click="dialog = null">{{ t('common.cancel') }}</FButton>
        <FButton
          variant="accent"
          :disabled="dialogBusy || !dialogName.trim()"
          @click="dialog === 'rename' ? applyRename() : applyNewFolder()"
        >
          {{ t('common.confirm') }}
        </FButton>
      </template>
    </FDialog>

    <!-- 删除确认 -->
    <FDialog
      :open="dialog === 'delete'"
      :title="t('sftp.deleteTitle')"
      :width="440"
      enter-confirm
      @update:open="(v: boolean) => (!v && (dialog = null))"
      @confirm="!dialogBusy && applyDelete()"
    >
      <div class="op-dialog-body">
        <template v-if="deleteTargets.length === 1">
          {{ t('sftp.deleteConfirmOne', { name: deleteTargets[0]?.name ?? '' }) }}
        </template>
        <template v-else>{{ t('sftp.deleteConfirmMany', { n: deleteTargets.length }) }}</template>
        <div class="op-dialog-warn">{{ t('sftp.deleteWarn') }}</div>
      </div>
      <template #footer>
        <FButton variant="subtle" @click="dialog = null">{{ t('common.cancel') }}</FButton>
        <FButton variant="danger" :disabled="dialogBusy" @click="applyDelete">{{ t('common.delete') }}</FButton>
      </template>
    </FDialog>

    <!-- 更改权限（远程） -->
    <FDialog
      :open="permDialog"
      :title="t('sftp.chmod')"
      :width="480"
      @update:open="(v: boolean) => (!v && (permDialog = false))"
    >
      <div class="perm-head">
        <span class="perm-name" :title="permName">{{ permName }}</span>
        <span class="perm-octal-label">{{ t('sftp.chmodOctal') }}</span>
        <input
          class="perm-octal mono"
          :value="permOctal"
          spellcheck="false"
          :class="{ invalid: permInvalid }"
          @input="onOctalInput(); permOctal = ($event.target as HTMLInputElement).value.replace(/[^0-7]/g, '').slice(0, 3)"
        />
      </div>

      <div class="perm-grid">
        <div v-for="role in PERM_ROLES" :key="role" class="perm-col">
          <div class="perm-col-title">{{ t(['sftp.chmodOwner', 'sftp.chmodGroup', 'sftp.chmodOther'][role]!) }}</div>
          <label v-for="bit in PERM_BITS" :key="bit" class="perm-check">
            <input
              type="checkbox"
              :checked="permChecked(role, bit)"
              @change="setPermBit(role, bit, ($event.target as HTMLInputElement).checked)"
            />
            <span>{{ t(['sftp.permRead', 'sftp.permWrite', 'sftp.permExec'][bit]!) }}</span>
          </label>
        </div>
      </div>

      <label class="perm-recursive">
        <input v-model="permRecursive" type="checkbox" />
        <span>{{ t('sftp.chmodRecursive') }}</span>
      </label>
      <div class="perm-hint">{{ t('sftp.chmodHint') }}</div>

      <template #footer>
        <FButton variant="subtle" @click="permDialog = false">{{ t('common.cancel') }}</FButton>
        <FButton variant="accent" :disabled="permBusy || permInvalid" @click="applyChmod">{{ t('common.confirm') }}</FButton>
      </template>
    </FDialog>

    <!-- 属性 -->
    <FDialog
      :open="dialog === 'props'"
      :title="t('sftp.props')"
      :width="460"
      @update:open="(v: boolean) => (!v && (dialog = null))"
    >
      <div v-if="propsLoading" class="op-dialog-body">{{ t('sftp.loading') }}</div>
      <div v-else-if="propsData" class="props-grid">
        <span>{{ t('sftp.colName') }}</span><span class="mono break">{{ propsData.name }}</span>
        <span>{{ t('sftp.colType') }}</span><span>{{ propsData.kind === 'dir' ? t('sftp.folder') : fileKindLabel(propsData.kind || 'file', propsData.name || '') }}</span>
        <span>{{ t('sftp.colSize') }}</span><span>{{ propsData.kind === 'dir' ? '—' : formatBytes(propsData.size ?? 0) }}<template v-if="propsData.kind !== 'dir'"> ({{ propsData.size }} {{ t('sftp.bytes') }})</template></span>
        <span>{{ t('sftp.colMtime') }}</span><span>{{ propsData.mtime ? formatTime(propsData.mtime) : '—' }}</span>
        <template v-if="propsData.attrs">
          <span>{{ t('sftp.colAttrs') }}</span><span class="mono">{{ propsData.attrs }}</span>
        </template>
        <template v-if="propsData.readonly !== undefined && side === 'local'">
          <span>{{ t('sftp.readonly') }}</span><span>{{ propsData.readonly ? t('common.yes') : t('common.no') }}</span>
        </template>
      </div>
      <template #footer>
        <FButton variant="accent" @click="dialog = null">{{ t('common.close') }}</FButton>
      </template>
    </FDialog>

    <!-- 在线编辑远程文件（毛玻璃/可缩放，语法高亮） -->
    <FileEditDialog
      v-if="side === 'remote'"
      v-model:open="editDialogOpen"
      :session="props.session"
      :file="editFile"
      @saved="reload()"
    />

    <!-- 行拖拽跟随 ghost（鼠标模拟拖拽） -->
    <div
      v-if="dragGhost"
      class="drag-ghost"
      :style="{ left: `${dragGhost.x + 14}px`, top: `${dragGhost.y + 10}px` }"
    >
      <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"><path d="M6 3.5h4.8L15.5 8v8.5a1 1 0 0 1-1 1h-8.5a1 1 0 0 1-1-1v-12a1 1 0 0 1 1-1zM10.5 3.8V8h4.6" /></svg>
      <span class="drag-ghost-label">{{ dragGhost.label }}</span>
    </div>
  </div>
</template>

<style scoped>
.file-pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  background: var(--bg-card);
  overflow: hidden;
  position: relative;
}
.file-pane.drag-over {
  border-color: var(--accent);
  box-shadow: inset 0 0 0 1px var(--accent);
}
.file-pane:focus {
  outline: none;
}
.file-pane:focus-visible {
  box-shadow: inset 0 0 0 1px var(--accent);
}
.drag-ghost {
  position: fixed;
  z-index: 1000;
  pointer-events: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 260px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--bg-card);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-sm);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
}
.drag-ghost-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.pane-head {
  display: flex;
  align-items: center;
  padding: 6px 8px 2px;
  flex: none;
}
.pane-tab {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  background: var(--bg-alt-hover);
  padding: 4px 12px;
  border-radius: 6px 6px 0 0;
}
.pane-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #2ecc71;
}
.pane-dot + * {
  margin-left: 2px;
}
.pathbar {
  display: flex;
  align-items: center;
  gap: 3px;
  padding: 4px 8px 8px;
  flex: none;
}
.pb-btn {
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex: none;
}
.pb-btn:hover:not(:disabled) {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.pb-btn:disabled {
  color: var(--text-disabled);
  cursor: default;
}
.pb-fav.active {
  color: #f8b64c;
}
.pb-fav.active:hover {
  color: #f8b64c;
}
.pb-path {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 4px 0 10px;
  background: var(--bg-card);
  border: 1px solid var(--stroke-control);
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
}
.pb-path:focus-within {
  border-color: var(--accent);
}
.pb-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-size: 12.5px;
  color: var(--text-primary);
}
.pane-body {
  flex: 1;
  display: flex;
  min-height: 0;
}
.quick-links {
  width: 108px;
  flex: none;
  border-right: 1px solid var(--stroke-divider);
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow: auto;
}
.quick-link {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: 12.5px;
  color: var(--text-primary);
  white-space: nowrap;
}
.quick-link:hover {
  background: var(--bg-alt-hover);
}
.quick-link svg {
  color: var(--accent);
  flex: none;
}
.quick-link span {
  overflow: hidden;
  text-overflow: ellipsis;
}
.ql-sep {
  height: 1px;
  background: var(--stroke-divider);
  margin: 4px 2px;
  flex: none;
}
.file-table-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.file-table {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.ft-head,
.ft-row {
  display: grid;
  grid-template-columns: minmax(140px, 2fr) 76px 90px 150px;
  gap: 8px;
  align-items: center;
  padding: 0 10px;
}
.is-remote .ft-head,
.is-remote .ft-row {
  grid-template-columns: minmax(140px, 2fr) 76px 90px 150px 110px;
}
.ft-head {
  height: 28px;
  font-size: 12px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--stroke-divider);
  user-select: none;
  flex: none;
}
.ft-head span {
  cursor: pointer;
  white-space: nowrap;
  position: relative;
  border-right: 1px solid var(--stroke-strong);
}
.ft-head span:last-child {
  border-right: none;
}
.ft-grip {
  position: absolute;
  top: 0;
  right: -6px;
  width: 7px;
  height: 100%;
  cursor: col-resize;
  z-index: 2;
}
.ft-grip:hover {
  background: var(--stroke-divider);
}
.ft-head span:hover {
  color: var(--text-primary);
}
.sort-arrow {
  font-size: 9px;
}
.ft-body {
  flex: 1;
  overflow: auto;
  padding-bottom: 4px;
}
.ft-row {
  height: 28px;
  font-size: 12.5px;
  cursor: default;
  border-radius: var(--radius-sm);
  margin: 0 4px;
  width: calc(100% - 8px);
}
.ft-row:hover {
  background: var(--bg-alt-hover);
}
.ft-row.selected {
  background: var(--bg-selected);
}
.ft-row.selected .col-name {
  outline: 1px solid var(--stroke-card);
  border-radius: 3px;
}
.col-name {
  display: flex;
  align-items: center;
  gap: 7px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.col-size,
.col-type,
.col-mtime,
.col-attrs {
  color: var(--text-secondary);
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ic {
  color: #f8b64c;
  flex: none;
}
.ic:not(.dir-ic) {
  color: var(--text-tertiary);
}
.ft-empty {
  padding: 24px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12.5px;
}
.pane-loading {
  position: absolute;
  right: 10px;
  bottom: 6px;
  font-size: 11.5px;
  color: var(--text-tertiary);
  background: var(--bg-card);
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-sm);
  padding: 2px 8px;
}
/* 空白处框选矩形（视口坐标，pointer-events 穿透） */
.marquee {
  position: fixed;
  z-index: 90;
  pointer-events: none;
  border: 1px solid var(--accent);
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  border-radius: 2px;
}
.op-dialog-body {
  font-size: 13px;
  color: var(--text-primary);
  line-height: 1.7;
}
.op-dialog-warn {
  margin-top: 10px;
  font-size: 12px;
  color: var(--danger);
}
.props-grid {
  display: grid;
  grid-template-columns: 90px minmax(0, 1fr);
  gap: 10px 14px;
  font-size: 12.5px;
  align-items: center;
}
.props-grid > span:nth-child(odd) {
  color: var(--text-secondary);
  text-align: right;
}
.mono {
  font-family: 'Cascadia Mono', Consolas, monospace;
}
.break {
  word-break: break-all;
}
.perm-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 16px;
}
.perm-name {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.perm-octal-label {
  font-size: 12.5px;
  color: var(--text-secondary);
  flex: none;
}
.perm-octal {
  width: 56px;
  height: 30px;
  border: 1px solid var(--stroke-control);
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  color: var(--text-primary);
  font-size: 14px;
  text-align: center;
  outline: none;
  flex: none;
}
.perm-octal:focus {
  border-color: var(--accent);
}
.perm-octal.invalid {
  border-color: var(--danger);
}
.perm-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
  margin-bottom: 14px;
}
.perm-col {
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  padding: 10px 12px;
  background: var(--bg-card);
}
.perm-col-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}
.perm-check {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  color: var(--text-secondary);
  padding: 3px 0;
  cursor: pointer;
}
.perm-check input,
.perm-recursive input {
  accent-color: var(--accent);
  width: 15px;
  height: 15px;
  cursor: pointer;
}
.perm-recursive {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  color: var(--text-primary);
  cursor: pointer;
  margin-bottom: 10px;
}
.perm-hint {
  font-size: 12px;
  color: var(--text-tertiary);
}
</style>
