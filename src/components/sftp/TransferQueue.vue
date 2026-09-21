<script setup lang="ts">
import { computed } from 'vue'
import { useTransfersStore } from '../../stores/transfers'
import { formatBytes, formatSpeed, formatDuration } from '../../utils/format'
import { FProgressBar, showContextMenu } from '../../ui'
import { t } from '../../i18n'
import type { TransferItem } from '../../api/types'

const transfers = useTransfersStore()

const statusLabel = computed<Record<TransferItem['status'], string>>(() => ({
  queued: t('transfer.status.queued'),
  running: t('transfer.status.running'),
  paused: t('transfer.status.paused'),
  done: t('transfer.status.done'),
  cancelled: t('transfer.status.cancelled'),
  failed: t('transfer.status.failed'),
}))

const items = computed(() => transfers.items)
const eta = (it: TransferItem) =>
  it.speed > 0 ? formatDuration(((it.total - it.transferred) / it.speed) * 1000) : '--'

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
        <div class="tq-head">
          <span class="col-name">{{ t('transfer.colName') }}</span>
          <span class="col-status">{{ t('transfer.colStatus') }}</span>
          <span class="col-progress">{{ t('transfer.colProgress') }}</span>
          <span class="col-size">{{ t('transfer.colSize') }}</span>
          <span class="col-local">{{ t('transfer.colLocal') }}</span>
          <span class="col-arrow"></span>
          <span class="col-remote">{{ t('transfer.colRemote') }}</span>
          <span class="col-speed">{{ t('transfer.colSpeed') }}</span>
          <span class="col-eta">{{ t('transfer.colEta') }}</span>
          <span class="col-elapsed">{{ t('transfer.colElapsed') }}</span>
          <span class="col-ops"></span>
        </div>
        <div class="tq-rows">
          <div v-if="!items.length" class="tq-empty">{{ t('transfer.empty') }}</div>
          <div v-for="it in items" :key="it.id" class="tq-row" @contextmenu.prevent="itemMenu($event, it)">
            <span class="col-name" :title="it.name">
              <svg v-if="it.direction === 'upload'" width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="var(--accent)" stroke-width="1.5" stroke-linecap="round"><path d="M10 13.5v-10M6.2 6.7L10 3l3.8 3.7M4.5 16.5h11" /></svg>
              <svg v-else width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="var(--success)" stroke-width="1.5" stroke-linecap="round"><path d="M10 3.5v9M6.2 9.3l3.8 3.7 3.8-3.7M4.5 16.5h11" /></svg>
              {{ it.name }}
            </span>
            <span class="col-status" :class="it.status">{{ statusLabel[it.status] }}</span>
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
  grid-template-columns: 150px 62px minmax(130px, 1.2fr) 150px minmax(120px, 1fr) 22px minmax(120px, 1fr) 82px 70px 64px 52px;
  gap: 8px;
  align-items: center;
  padding: 0 10px;
}
.tq-head {
  height: 26px;
  font-size: 11.5px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--stroke-divider);
  flex: none;
}
.tq-rows {
  flex: 1;
  overflow: auto;
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
