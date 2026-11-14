<script setup lang="ts">
import { computed, ref } from 'vue'
import { useTransfersStore } from '../../stores/transfers'
import { formatBytes, formatSpeed, formatDuration } from '../../utils/format'
import { FProgressBar, showContextMenu, useColResize } from '../../ui'
import { t } from '../../i18n'
import type { TransferItem } from '../../api/types'

const transfers = useTransfersStore()

/* ---------- 列宽拖拽调整（末列操作列为 1fr，不在宽度表内） ---------- */
const { gridStyle, resizing, startResize } = useColResize(
  // 名称 状态 进度 大小 本地路径 ⇄ 远程路径 速度 剩余 已用时
  [150, 62, 180, 150, 240, 22, 240, 82, 70, 64],
  [90, 52, 100, 100, 90, 18, 90, 56, 46, 46],
)

const statusLabel = computed<Record<TransferItem['status'], string>>(() => ({
  queued: t('transfer.status.queued'),
  running: t('transfer.status.running'),
  paused: t('transfer.status.paused'),
  done: t('transfer.status.done'),
  cancelled: t('transfer.status.cancelled'),
  failed: t('transfer.status.failed'),
}))

const eta = (it: TransferItem) =>
  it.speed > 0 ? formatDuration(((it.total - it.transferred) / it.speed) * 1000) : '--'

/* ---------- 列头排序：点击 名称/状态/进度/大小 切换升序降序，未点击前保持入队顺序 ---------- */
type TqSortKey = 'name' | 'status' | 'progress' | 'size'
const sortKey = ref<TqSortKey | null>(null)
const sortAsc = ref(true)

function setSort(key: TqSortKey) {
  if (sortKey.value === key) sortAsc.value = !sortAsc.value
  else {
    sortKey.value = key
    sortAsc.value = true
  }
}

/* 状态按生命周期排序：排队 → 传输中 → 已暂停 → 已完成 → 已取消 → 失败 */
const statusRank: Record<TransferItem['status'], number> = {
  queued: 0,
  running: 1,
  paused: 2,
  done: 3,
  cancelled: 4,
  failed: 5,
}

const sortedItems = computed(() => {
  if (!sortKey.value) return transfers.items
  const dir = sortAsc.value ? 1 : -1
  const list = [...transfers.items]
  list.sort((a, b) => {
    switch (sortKey.value) {
      case 'name':
        return a.name.localeCompare(b.name, 'zh-CN') * dir
      case 'status':
        return (statusRank[a.status] - statusRank[b.status]) * dir
      case 'progress':
        return (a.transferred / a.total - b.transferred / b.total) * dir
      default:
        return (a.total - b.total) * dir
    }
  })
  return list
})

function itemMenu(e: MouseEvent, it: TransferItem) {
  showContextMenu(e, [
    ...(it.status === 'running'
      ? [{ label: t('transfer.pause'), icon: 'pause', onClick: () => transfers.pause(it.id) }]
      : []),
    ...(it.status === 'paused'
      ? [{ label: t('transfer.resume'), icon: 'play', onClick: () => transfers.resume(it.id) }]
      : []),
    ...(it.status === 'failed' || it.status === 'cancelled'
      ? [{ label: t('transfer.retry'), icon: 'play', onClick: () => transfers.retry(it.id) }]
      : []),
    ...(it.status === 'running' || it.status === 'queued' || it.status === 'paused'
      ? [{ label: t('transfer.cancel'), icon: 'close', danger: true, onClick: () => transfers.cancel(it.id) }]
      : []),
    { separator: true },
    { label: t('transfer.remove'), onClick: () => transfers.clearFinished() },
  ])
}
</script>

<template>
  <div class="transfer-queue">
    <div class="tq-tabs">
      <button class="tq-tab" :class="{ on: transfers.activeTab === 'transfer' }" @click="transfers.activeTab = 'transfer'">
        {{ t('transfer.tabTransfer') }}<span v-if="transfers.runningCount" class="tq-badge">{{ transfers.runningCount }}</span>
      </button>
      <button class="tq-tab" :class="{ on: transfers.activeTab === 'log' }" @click="transfers.activeTab = 'log'">{{ t('transfer.tabLog') }}</button>
      <div class="tq-spacer" />
      <button v-if="transfers.activeTab === 'transfer'" class="tq-action" :title="t('transfer.clearFinished')" @click="transfers.clearFinished">{{ t('transfer.clearFinished') }}</button>
      <button class="tq-action" :title="transfers.expanded ? '收起' : '展开'" @click="transfers.expanded = !transfers.expanded">
        {{ transfers.expanded ? t('transfer.collapse') : t('transfer.expand') }}
      </button>
    </div>

    <div v-show="transfers.expanded" class="tq-body">
      <template v-if="transfers.activeTab === 'transfer'">
        <div class="tq-scroll" :class="{ resizing: resizing >= 0 }">
          <div class="tq-head" :style="gridStyle">
          <span class="col-name sortable" @click="setSort('name')">{{ t('transfer.colName') }}<span v-if="sortKey === 'name'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span><i class="tq-grip" @mousedown.stop="startResize(0, $event)" @click.stop /></span>
          <span class="col-status sortable" @click="setSort('status')">{{ t('transfer.colStatus') }}<span v-if="sortKey === 'status'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span><i class="tq-grip" @mousedown.stop="startResize(1, $event)" @click.stop /></span>
          <span class="col-progress sortable" @click="setSort('progress')">{{ t('transfer.colProgress') }}<span v-if="sortKey === 'progress'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span><i class="tq-grip" @mousedown.stop="startResize(2, $event)" @click.stop /></span>
          <span class="col-size sortable" @click="setSort('size')">{{ t('transfer.colSize') }}<span v-if="sortKey === 'size'" class="sort-arrow">{{ sortAsc ? '▲' : '▼' }}</span><i class="tq-grip" @mousedown.stop="startResize(3, $event)" @click.stop /></span>
          <span class="col-local">{{ t('transfer.colLocal') }}<i class="tq-grip" @mousedown.stop="startResize(4, $event)" @click.stop /></span>
          <span class="col-arrow"></span>
          <span class="col-remote">{{ t('transfer.colRemote') }}<i class="tq-grip" @mousedown.stop="startResize(6, $event)" @click.stop /></span>
          <span class="col-speed">{{ t('transfer.colSpeed') }}<i class="tq-grip" @mousedown.stop="startResize(7, $event)" @click.stop /></span>
          <span class="col-eta">{{ t('transfer.colEta') }}<i class="tq-grip" @mousedown.stop="startResize(8, $event)" @click.stop /></span>
          <span class="col-elapsed">{{ t('transfer.colElapsed') }}<i class="tq-grip" @mousedown.stop="startResize(9, $event)" @click.stop /></span>
          <span class="col-ops"></span>
        </div>
        <div class="tq-rows">
          <div v-if="!sortedItems.length" class="tq-empty">{{ t('transfer.empty') }}</div>
          <div v-for="it in sortedItems" :key="it.id" class="tq-row" :style="gridStyle" @contextmenu.prevent="itemMenu($event, it)">
            <span class="col-name" :title="it.name">
              <svg v-if="it.direction === 'upload'" width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="var(--accent)" stroke-width="1.5" stroke-linecap="round"><path d="M10 13.5v-10M6.2 6.7L10 3l3.8 3.7M4.5 16.5h11" /></svg>
              <svg v-else width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="var(--success)" stroke-width="1.5" stroke-linecap="round"><path d="M10 3.5v9M6.2 9.3l3.8 3.7 3.8-3.7M4.5 16.5h11" /></svg>
              {{ it.name }}
            </span>
            <span class="col-status" :class="it.status" :title="it.error">{{ statusLabel[it.status] }}</span>
            <span class="col-progress">
              <FProgressBar :value="(it.transferred / it.total) * 100" :paused="it.status === 'paused'" />
              <span class="pct">{{ Math.round((it.transferred / it.total) * 100) }}%</span>
            </span>
            <span class="col-size">{{ formatBytes(it.transferred) }} / {{ formatBytes(it.total) }}</span>
            <span class="col-local" :title="it.localPath">{{ it.localPath }}</span>
            <span class="col-arrow">⇄</span>
            <span class="col-remote" :title="it.remotePath">{{ it.remotePath }}</span>
            <span class="col-speed">{{ formatSpeed(it.speed) }}</span>
            <span class="col-eta">{{ eta(it) }}</span>
            <span class="col-elapsed">{{ formatDuration(it.elapsedMs) }}</span>
            <span class="col-ops">
              <button v-if="it.status === 'running'" class="op" :title="t('transfer.pause')" @click="transfers.pause(it.id)">❚❚</button>
              <button v-else-if="it.status === 'paused'" class="op" :title="t('transfer.resume')" @click="transfers.resume(it.id)">▶</button>
              <button
                v-else-if="it.status === 'failed' || it.status === 'cancelled'"
                class="op"
                :title="t('transfer.retry')"
                @click="transfers.retry(it.id)"
              >↻</button>
              <button
                v-if="['running', 'queued', 'paused'].includes(it.status)"
                class="op danger"
                :title="t('transfer.cancel')"
                @click="transfers.cancel(it.id)"
              >✕</button>
            </span>
          </div>
        </div>
        </div>
      </template>

      <div v-else class="tq-log">
        <div v-if="!transfers.logs.length" class="tq-empty">{{ t('transfer.emptyLog') }}</div>
        <div v-for="(log, i) in transfers.logs" :key="i" class="log-line" :class="log.kind">
          <span class="log-time">{{ log.time }}</span>
          <span>{{ log.text }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.transfer-queue {
  border-top: 1px solid var(--stroke-divider);
  background: var(--bg-card);
  display: flex;
  flex-direction: column;
  flex: none;
  height: 210px;
}
.tq-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 8px 0;
  flex: none;
}
.tq-tab {
  border: none;
  background: transparent;
  font: inherit;
  font-size: 12.5px;
  padding: 6px 14px;
  border-radius: 6px 6px 0 0;
  color: var(--text-secondary);
  cursor: pointer;
  border-bottom: 2px solid transparent;
}
.tq-tab.on {
  color: var(--text-primary);
  border-bottom-color: var(--accent);
  font-weight: 600;
}
.tq-badge {
  background: var(--accent);
  color: var(--accent-text-on);
  border-radius: 9px;
  font-size: 10.5px;
  padding: 1px 7px;
  margin-left: 6px;
}
.tq-spacer {
  flex: 1;
}
.tq-action {
  border: none;
  background: transparent;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
}
.tq-action:hover {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.tq-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.tq-head,
.tq-row {
  display: grid;
  gap: 8px;
  align-items: center;
  padding: 0 10px;
}
.tq-head > span {
  position: relative;
  border-right: 1px solid var(--stroke-strong);
  white-space: nowrap;
  overflow: hidden;
}
.tq-head .sortable {
  cursor: pointer;
}
.tq-head .sortable:hover {
  color: var(--text-primary);
}
.sort-arrow {
  font-size: 9px;
}
.tq-head > span:last-child {
  border-right: none;
}
.tq-grip {
  position: absolute;
  top: 0;
  right: -7px;
  width: 8px;
  height: 100%;
  cursor: col-resize;
  z-index: 2;
}
.tq-grip:hover {
  background: var(--stroke-divider);
}
.tq-scroll.resizing {
  user-select: none;
  cursor: col-resize;
}
/* 表头与行共用一个滚动容器，固定列宽超出时横向同步滚动；表头 sticky 常驻 */
.tq-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
}
.tq-head,
.tq-rows {
  width: max-content;
  min-width: 100%;
}
.tq-head {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--bg-card);
  height: 26px;
  font-size: 11.5px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--stroke-divider);
  flex: none;
  user-select: none;
}
.tq-rows {
  flex: none;
}
.tq-row {
  height: 34px;
  font-size: 12px;
  border-radius: var(--radius-sm);
  margin: 1px 4px;
  width: calc(100% - 8px);
}
.tq-row:hover {
  background: var(--bg-alt-hover);
}
.tq-row > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary);
}
.col-name {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-primary) !important;
}
.col-status.running {
  color: var(--accent);
}
.col-status.done {
  color: var(--success);
}
.col-status.paused {
  color: var(--warning);
}
.col-status.cancelled,
.col-status.failed {
  color: var(--danger);
}
.col-progress {
  display: flex;
  align-items: center;
  gap: 6px;
}
.pct {
  font-size: 11px;
  width: 32px;
  flex: none;
  text-align: right;
}
.col-arrow {
  text-align: center;
  color: var(--text-tertiary) !important;
}
.col-ops {
  display: flex;
  gap: 3px;
  justify-content: flex-end;
}
.op {
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 10px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.op:hover {
  background: var(--bg-alt-hover);
  color: var(--text-primary);
}
.op.danger:hover {
  color: var(--danger);
}
.tq-empty {
  padding: 26px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12.5px;
}
.tq-log {
  flex: 1;
  overflow: auto;
  padding: 6px 12px;
  font-size: 12px;
}
.log-line {
  display: flex;
  gap: 10px;
  padding: 2px 0;
  color: var(--text-secondary);
}
.log-line.success {
  color: var(--success);
}
.log-line.error {
  color: var(--danger);
}
.log-time {
  color: var(--text-tertiary);
  flex: none;
}
</style>
