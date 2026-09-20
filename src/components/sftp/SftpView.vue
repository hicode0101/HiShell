<script setup lang="ts">
import { ref } from 'vue'
import type { SessionTab } from '../../api/types'
import FilePane from './FilePane.vue'
import type { TransferRequest } from './FilePane.vue'
import TransferQueue from './TransferQueue.vue'
import { useTransfersStore } from '../../stores/transfers'
import { mockSpecialDirs } from '../../api/backend'
import { formatBytes } from '../../utils/format'
import { toast } from '../../ui'
import { t } from '../../i18n'

const props = defineProps<{ tab: SessionTab }>()
const transfers = useTransfersStore()

const splitRatio = ref(0.5)
const panesEl = ref<HTMLElement>()
const leftCount = ref({ files: 0, dirs: 0 })
const rightCount = ref({ files: 0, dirs: 0 })
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

/** 拖拽传输请求：计算方向并入队（模拟速率推进，M4 换真实通道） */
function onTransfer(req: TransferRequest) {
  if (req.from === 'remote') {
    for (const entry of req.entries) {
      if (entry.name === '..') continue
      const isDir = entry.kind === 'dir'
      transfers.submit({
        sid: props.tab.sid,
        name: entry.name,
        isDir,
        direction: 'download',
        localPath: `${leftPath.value}\\${entry.name}${isDir ? '\\' : ''}`,
        remotePath: `${req.fromPath === '/' ? '' : req.fromPath}/${entry.name}${isDir ? '/' : ''}`,
        total: entry.size || (isDir ? 12_582_912 : 1_048_576),
      })
    }
    toast(t('sftp.paneLoadingToast', { n: req.entries.length }), 'success')
  } else {
    for (const entry of req.entries) {
      if (entry.name === '..') continue
      const isDir = entry.kind === 'dir'
      transfers.submit({
        sid: props.tab.sid,
        name: entry.name,
        isDir,
        direction: 'upload',
        localPath: `${req.fromPath}\\${entry.name}`,
        remotePath: `${rightPath.value}/${entry.name}${isDir ? '/' : ''}`,
        total: entry.size || (isDir ? 8_388_608 : 1_048_576),
      })
    }
    toast(t('sftp.paneLoadingToastUp', { n: req.entries.length }), 'success')
  }
}
</script>

<template>
  <div class="sftp-view">
    <div ref="panesEl" class="panes">
      <FilePane
        side="local"
        :style="{ width: `calc(${splitRatio * 100}% - 5px)` }"
        @transfer-request="onTransfer"
        @counts-changed="(c) => (leftCount = c)"
        @path-changed="(p) => (leftPath = p)"
      />
      <div class="divider" :class="{ dragging: dividerDragging }" @mousedown="onDividerDown" />
      <FilePane
        side="remote"
        :session="tab"
        class="flex-1"
        @transfer-request="onTransfer"
        @counts-changed="(c) => (rightCount = c)"
        @path-changed="(p) => (rightPath = p)"
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
      <span>{{ t('sftp.filesCount', { n: leftCount.files + rightCount.files }) }}</span>
      <span>{{ t('sftp.dirsCount', { n: leftCount.dirs + rightCount.dirs }) }}</span>
      <span>{{ formatBytes(516_950_000) }}</span>
    </div>
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
</style>
