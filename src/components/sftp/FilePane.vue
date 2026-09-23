<script setup lang="ts">
/**
 * 文件窗格：本地 / 远程通用。
 * - 本地：真实列目录（Tauri）/ mock 文件树（浏览器）
 * - 远程：真实 SFTP（sftp_list），连接前显示提示
 * - 拖拽：行可拖拽 → 落到对侧窗格入传输队列；系统文件拖入由 SftpView 经
 *   Tauri drag-drop 事件处理（真实路径），浏览器预览模式仍走 HTML5 files
 * - 右键：下载/上传、重命名、删除、属性、新建文件夹、双击打开（远程文件先下载到临时目录）
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
import { useTransfersStore } from '../../stores/transfers'
import { formatBytes, formatTime, fileKindLabel } from '../../utils/format'
import { t } from '../../i18n'
import { FButton, FTextBox, FDialog, showContextMenu, toast, useColResize } from '../../ui'

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
  return joinPath(path.value, entry.name)
}

function openRename(entry: DirEntry) {
  dialogTargets.value = [entry]
  dialogName.value = entry.name
  dialog.value = 'rename'
}

function openNewFolder() {
  dialogTargets.value = []
  dialogName.value = t('sftp.newFolderDefault')
  dialog.value = 'newFolder'
}

function openDelete(entry: DirEntry) {
  const sel = entries.value.filter((x) => selected.value.has(x.name))
  dialogTargets.value = sel.length > 1 && sel.some((x) => x.name === entry.name) ? sel : [entry]
  dialog.value = 'delete'
}

async function openProps(entry: DirEntry) {
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

/* ---------- 隐藏文件显示（仅远程面板） ---------- */

const showHidden = ref(false)

function toggleShowHidden() {
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

/* ---------- 加载 ---------- */

async function load(p: string, pushBack = true) {
  loading.value = true
  selected.value = new Set()
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
    if (idx < 0) load('')
    else load(norm.slice(0, idx))
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
  if (e.shiftKey && lastSelected.value) {
    const start = entries.value.findIndex((x) => x.name === lastSelected.value)
    const range = entries.value.slice(Math.min(start, idx), Math.max(start, idx) + 1)
    if (!e.ctrlKey) selected.value = new Set()
    range.forEach((x) => selected.value.add(x.name))
  } else if (e.ctrlKey) {
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

/** 系统外部文件拖入（仅浏览器预览模式；tauri 下由 SftpView 经 Tauri 事件处理） */
function onDrop(e: DragEvent) {
  dragOver.value = false
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
  document.body.style.userSelect = ''
})

/* ---------- 键盘：Delete 删除选中项（带确认对话框） ---------- */

function onPaneKeydown(e: KeyboardEvent) {
  if ((e.target as HTMLElement).closest('input, textarea, [contenteditable]')) return
  if (e.key === 'Delete') {
    e.preventDefault()
    deleteSelection()
  }
}

function deleteSelection() {
  const targets = entries.value.filter((x) => selected.value.has(x.name) && x.name !== '..')
  if (!targets.length || dialog.value) return
  dialogTargets.value = targets
  dialog.value = 'delete'
}

/* ---------- 右键菜单 ---------- */

function rowContextMenu(e: MouseEvent, entry: DirEntry) {
  onRowClick(e, entry, entries.value.findIndex((x) => x.name === entry.name))
  const targets = entries.value.filter((x) => selected.value.has(x.name))
  const realTargets = targets.length ? targets : [entry]
  const isDotDot = entry.name === '..'
  showContextMenu(e, [
    {
      label: props.side === 'remote' ? t('sftp.download') : t('sftp.upload'),
      icon: props.side === 'remote' ? 'download' : 'upload',
      disabled: realTargets.every((x) => x.name === '..'),
      onClick: () => emit('transfer-request', { entries: realTargets, from: props.side, fromPath: path.value }),
    },
    { separator: true },
    {
      label: t('sftp.rename'),
      icon: 'edit',
      disabled: isDotDot,
      onClick: () => openRename(entry),
    },
    {
      label: t('sftp.delete'),
      icon: 'trash',
      danger: true,
      disabled: isDotDot,
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
    {
      label: t('sftp.props'),
      icon: 'info',
      onClick: () => openProps(entry),
    },
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
      disabled: props.side === 'remote' && props.session?.status !== 'connected',
      onClick: () => openNewFolder(),
    },
    { separator: true },
    { label: t('sftp.paste'), icon: 'copy', disabled: true },
  ])
}

/* ---------- 更改权限（远程，参考 Xftp） ---------- */

const permDialog = ref(false)
const permBusy = ref(false)
const permName = ref('')
const permPath = ref('')
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
  if (props.session?.sid === undefined) {
    toast(t('sftp.notConnected'), 'error')
    return
  }
  permName.value = entry.name
  permPath.value = targetPath(entry)
  permIsDir.value = entry.kind === 'dir'
  permRecursive.value = false
  if (entry.attrs) {
    permOctal.value = octalFromAttrs(entry.attrs)
  } else {
    try {
      const st = await sftpStat(props.session.sid, permPath.value)
      permOctal.value = octalFromAttrs(st?.attrs)
    } catch {
      permOctal.value = '644'
    }
  }
  permDialog.value = true
}

async function applyChmod() {
  if (!isTauri || props.session?.sid === undefined || permInvalid.value || permBusy.value) return
  permBusy.value = true
  try {
    const n = await sftpChmod(props.session.sid, permPath.value, parseInt(permOctal.value, 8), permRecursive.value)
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
    load(joinPath(path.value, entry.name))
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
  if (props.session?.status !== 'connected' || props.session?.sid === undefined) {
    toast(t('sftp.notConnected'), 'error')
    return
  }
  try {
    const temp = await specialDir('temp')
    const targetDir = `${temp.replace(/[\\/]+$/, '')}\\hishell-open`
    fsMkdir(targetDir).catch(() => {})
    const localPath = `${targetDir}\\${entry.name}`
    const id = await transfers.submit({
      sid: props.session.sid,
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
    : base.replace(/[\\/]+$/, '') + '\\' + name
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
    </div>

    <div class="pane-body">
      <div v-if="side === 'local'" class="quick-links">
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
          <div class="ft-body" @click="onBodyClick">
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

    <!-- 重命名 / 新建文件夹 -->
    <FDialog
      :open="dialog === 'rename' || dialog === 'newFolder'"
      :title="dialog === 'rename' ? t('sftp.rename') : t('sftp.newFolder')"
      :width="420"
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
      @update:open="(v: boolean) => (!v && (dialog = null))"
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
  grid-template-columns: 90px 1fr;
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
