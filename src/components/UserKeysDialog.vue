<script setup lang="ts">
/**
 * 用户密钥管理（参考 Xshell 用户密钥）：列出 ~/.ssh 下的密钥，
 * 右侧操作：生成 / 属性 / 删除 / 导入 / 导出。
 * 生成走四步生成向导；导入选择 OpenSSH 私钥文件（可带口令）；
 * 导出支持公钥（OpenSSH / IETF SECSH）与私钥（可重设口令加密）。
 */
import { computed, ref, watch } from 'vue'
import { FButton, FComboBox, FConfirmDialog, FDialog, FRadioGroup, FTextBox, toast } from '../ui'
import {
  userKeyList,
  userKeyDelete,
  userKeyRename,
  userKeyInspect,
  userKeyImport,
  userKeyPublicContent,
  userKeyExportPrivateTo,
  userKeyWriteText,
  type UserKeyInfo,
  type KeyInspect,
} from '../api/backend'
import { toRfc4716 } from '../utils/pubkeyFormat'
import { isTauri } from '../api/adapter'
import { t } from '../i18n'
import KeyGenWizard from './KeyGenWizard.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const keys = ref<UserKeyInfo[]>([])
const selected = ref('')
const loading = ref(false)
const propsOpen = ref(false)
const wizardOpen = ref(false)
const deleteOpen = ref(false)
const deleteName = ref('')

const current = computed(() => keys.value.find((k) => k.name === selected.value))

async function refresh() {
  loading.value = true
  try {
    keys.value = await userKeyList()
    if (!keys.value.some((k) => k.name === selected.value)) {
      selected.value = keys.value[0]?.name ?? ''
    }
  } finally {
    loading.value = false
  }
}

watch(
  () => props.open,
  (v) => {
    if (v) refresh()
  },
)

function select(name: string) {
  selected.value = name
}

const editName = ref('')

function openProps() {
  if (!current.value) return
  editName.value = current.value.name
  propsOpen.value = true
}

async function saveRename() {
  const k = current.value
  if (!k) return
  const nn = editName.value.trim()
  if (!nn || nn === k.name) return
  try {
    const info = await userKeyRename(k.publicPath, k.privatePath, nn)
    toast(t('userKeys.renamedToast', { name: info.name }), 'success')
    selected.value = info.name
    editName.value = info.name
    await refresh()
  } catch (e) {
    toast(String(e), 'error')
  }
}

function removeKey() {
  const k = current.value
  if (!k) return
  deleteName.value = k.name
  deleteOpen.value = true
}

async function confirmDelete() {
  const k = current.value
  if (!k) return
  try {
    await userKeyDelete(k.publicPath, k.privatePath)
    toast(t('userKeys.deletedToast', { name: k.name }), 'success')
    selected.value = ''
    await refresh()
  } catch (e) {
    toast(String(e), 'error')
  }
}

/* ---------- 列宽拖拽调整 ---------- */
const colWidths = ref<number[]>([170, 110, 60])
const gridStyle = computed(() => ({
  gridTemplateColumns: colWidths.value.map((w) => `${w}px`).join(' ') + ' 1fr',
}))
const resizing = ref(-1)
let resizeStartX = 0
let resizeStartW = 0
const colMinWidths = [80, 60, 40]

function startResize(i: number, e: MouseEvent) {
  resizing.value = i
  resizeStartX = e.clientX
  resizeStartW = colWidths.value[i]
  document.addEventListener('mousemove', onResizeMove)
  document.addEventListener('mouseup', stopResize)
  e.preventDefault()
}
function onResizeMove(e: MouseEvent) {
  const i = resizing.value
  if (i < 0) return
  colWidths.value[i] = Math.max(colMinWidths[i] ?? 40, resizeStartW + e.clientX - resizeStartX)
}
function stopResize() {
  resizing.value = -1
  document.removeEventListener('mousemove', onResizeMove)
  document.removeEventListener('mouseup', stopResize)
}

/* ---------- 导入 ---------- */

const importOpen = ref(false)
const importSource = ref('')
const importName = ref('')
const importPass = ref('')
const importEncrypted = ref(false)
const importKeyType = ref('')
const importBits = ref(0)
const importBusy = ref(false)

async function pickImportFile() {
  if (!isTauri) {
    toast(t('connect.browserMockToast'))
    return
  }
  const { open } = await import('@tauri-apps/plugin-dialog')
  const file = await open({
    multiple: false,
    title: t('userKeys.importPickFile'),
    filters: [
      { name: '私钥文件', extensions: ['pem', 'key'] },
      { name: '所有文件', extensions: ['*'] },
    ],
  })
  if (typeof file !== 'string') return
  let inspect: KeyInspect
  try {
    inspect = await userKeyInspect(file)
  } catch (e) {
    toast(String(e), 'error')
    return
  }
  importSource.value = file
  const base = file.split(/[\\/]/).pop() ?? ''
  importName.value = base.replace(/\.(pub|pem|key)$/i, '')
  importEncrypted.value = inspect.encrypted
  importKeyType.value = inspect.keyType ?? ''
  importBits.value = inspect.bits ?? 0
  importPass.value = ''
  importOpen.value = true
}

async function doImport() {
  if (importBusy.value) return
  importBusy.value = true
  try {
    const info = await userKeyImport({
      sourcePath: importSource.value,
      name: importName.value,
      passphrase: importPass.value,
    })
    toast(t('userKeys.importSuccess', { name: info.name }), 'success')
    importOpen.value = false
    await refresh()
    selected.value = info.name
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    importBusy.value = false
  }
}

/* ---------- 导出 ---------- */

const exportOpen = ref(false)
const exportFormat = ref('SSH2 - OpenSSH')
const exportFormatOptions = ['SSH2 - OpenSSH', 'SSH2 - IETF SECSH']
const exportCurrentPass = ref('')
const exportNewPass = ref('')
const exportEncrypted = ref(false)
const exportBusy = ref(false)
const exportWhich = ref<'public' | 'private'>('private')
const exportWhichOptions = computed(() => [
  { label: t('userKeys.exportItemPrivate'), value: 'private', disabled: !current.value?.privatePath },
  { label: t('userKeys.exportItemPublic'), value: 'public' },
])
/* 私钥导出格式（对应 Xshell 密钥类型） */
const keyFormatOptions = ['OpenSSH New Format Private Keys', 'PEM(OpenSSH2) Private Keys', 'PuTTY .ppk']
const exportKeyFormat = ref('OpenSSH New Format Private Keys')
function keyFormatValue(label: string): 'openssh' | 'pem' | 'ppk' {
  if (label === 'PEM(OpenSSH2) Private Keys') return 'pem'
  if (label === 'PuTTY .ppk') return 'ppk'
  return 'openssh'
}

async function openExport() {
  const k = current.value
  if (!k) return
  // 默认导出私钥；没有私钥文件的密钥退回公钥
  exportWhich.value = k.privatePath ? 'private' : 'public'
  exportFormat.value = 'SSH2 - OpenSSH'
  exportCurrentPass.value = ''
  exportNewPass.value = ''
  exportEncrypted.value = false
  exportOpen.value = true
  if (k.privatePath) {
    try {
      const ins = await userKeyInspect(k.privatePath)
      exportEncrypted.value = ins.encrypted
    } catch {
      exportEncrypted.value = false // 导出时后端会再校验
    }
  }
}

async function doExport() {
  const k = current.value
  if (!k || exportBusy.value) return
  exportBusy.value = true
  try {
    const { save } = await import('@tauri-apps/plugin-dialog')
    if (exportWhich.value === 'public') {
      let content = await userKeyPublicContent(k.publicPath)
      if (exportFormat.value === 'SSH2 - IETF SECSH') content = toRfc4716(content, k.comment)
      const path = await save({
        defaultPath: `${k.name}.pub`,
        filters: [{ name: '公钥文件', extensions: ['pub'] }],
      })
      if (!path) {
        toast(t('userKeys.exportCancel'))
        return
      }
      await userKeyWriteText(path, content)
      toast(t('userKeys.exportSuccess', { path }), 'success')
    } else {
      if (!k.privatePath) return
      const fmt = keyFormatValue(exportKeyFormat.value)
      const ext = fmt === 'ppk' ? '.ppk' : fmt === 'pem' ? '.pem' : ''
      const path = await save({ defaultPath: k.name + ext })
      if (!path) {
        toast(t('userKeys.exportCancel'))
        return
      }
      await userKeyExportPrivateTo({
        privatePath: k.privatePath,
        currentPassphrase: exportCurrentPass.value,
        newPassphrase: fmt === 'openssh' ? exportNewPass.value : '',
        format: fmt,
        destPath: path,
      })
      toast(t('userKeys.exportSuccess', { path }), 'success')
    }
    exportOpen.value = false
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    exportBusy.value = false
  }
}
</script>

<template>
  <FDialog :open="open" :title="t('userKeys.title')" :width="700" @update:open="emit('update:open', $event)">
    <div class="uk-body">
      <div class="uk-list">
        <div class="uk-head" :style="gridStyle">
          <span class="col-name">
            {{ t('userKeys.colName') }}
            <i class="col-grip" @mousedown="startResize(0, $event)" />
          </span>
          <span class="col-type">
            {{ t('userKeys.colType') }}
            <i class="col-grip" @mousedown="startResize(1, $event)" />
          </span>
          <span class="col-bits">
            {{ t('userKeys.colBits') }}
            <i class="col-grip" @mousedown="startResize(2, $event)" />
          </span>
          <span class="col-comment">{{ t('userKeys.colComment') }}</span>
        </div>
        <div class="uk-rows" :class="{ resizing: resizing >= 0 }">
          <div v-if="loading" class="uk-empty">{{ t('sftp.loading') }}</div>
          <div v-else-if="!keys.length" class="uk-empty">{{ t('userKeys.empty') }}</div>
          <div
            v-for="k in keys"
            :key="k.name"
            class="uk-row"
            :class="{ selected: k.name === selected }"
            :style="gridStyle"
            :title="k.name"
            @click="select(k.name)"
            @dblclick="selected = k.name; openProps()"
          >
            <span class="col-name">
              <svg width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="7" cy="7" r="3.2" /><path d="M9.5 9.5L16 16M13.5 13.5l-1.8 1.8M15.8 11.2L14 13" />
              </svg>
              {{ k.name }}
            </span>
            <span class="col-type">{{ k.keyType }}</span>
            <span class="col-bits">{{ k.bits || '—' }}</span>
            <span class="col-comment" :title="k.comment">{{ k.comment }}</span>
          </div>
        </div>
      </div>

      <div class="uk-ops">
        <FButton @click="wizardOpen = true">{{ t('userKeys.generate') }}</FButton>
        <FButton :disabled="!current" @click="openProps">{{ t('userKeys.props') }}</FButton>
        <FButton variant="danger" :disabled="!current" @click="removeKey">
          {{ t('common.delete') }}
        </FButton>
        <FButton @click="pickImportFile">{{ t('userKeys.import') }}</FButton>
        <FButton :disabled="!current" @click="openExport">{{ t('userKeys.export') }}</FButton>
      </div>
    </div>

    <template #footer>
      <FButton variant="accent" @click="emit('update:open', false)">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>

  <!-- 密钥生成向导 -->
  <KeyGenWizard :open="wizardOpen" @update:open="wizardOpen = $event" @generated="refresh" />

  <!-- 导入用户密钥 -->
  <FDialog :open="importOpen" :title="t('userKeys.importTitle')" :width="560" @update:open="importOpen = $event">
    <div class="uk-form">
      <span class="lbl">{{ t('userKeys.importSource') }}</span>
      <span class="mono break">{{ importSource }}</span>
      <span class="lbl">{{ t('userKeys.colType') }}</span>
      <span>
        {{ importKeyType }}<template v-if="importBits"> · {{ importBits }} {{ t('userKeys.bitsUnit') }}</template>
        · {{ importEncrypted ? t('userKeys.encrypted') : t('userKeys.plain') }}
      </span>
      <span class="lbl">{{ t('userKeys.wizKeyName') }}</span>
      <FTextBox v-model="importName" spellcheck="false" />
      <span class="lbl">{{ t('userKeys.wizPass') }}</span>
      <FTextBox v-model="importPass" type="password" />
    </div>
    <p class="uk-hint">{{ t('userKeys.importPassHint') }}</p>
    <template #footer>
      <FButton @click="importOpen = false">{{ t('userKeys.wizCancel') }}</FButton>
      <FButton variant="accent" :disabled="importBusy" @click="doImport">{{ t('userKeys.importBtn') }}</FButton>
    </template>
  </FDialog>

  <!-- 导出用户密钥 -->
  <FDialog :open="exportOpen" :title="t('userKeys.exportTitle')" :width="520" @update:open="exportOpen = $event">
    <div class="uk-form uk-export-form">
      <span class="lbl">{{ t('userKeys.exportWhich') }}</span>
      <FRadioGroup v-model="exportWhich" :options="exportWhichOptions" />
      <template v-if="exportWhich === 'public'">
        <span class="lbl">{{ t('userKeys.wizPubFormat') }}</span>
        <FComboBox v-model="exportFormat" :options="exportFormatOptions" />
      </template>
      <template v-else-if="current?.privatePath">
        <span class="lbl">{{ t('userKeys.exportKeyType') }}</span>
        <FComboBox v-model="exportKeyFormat" :options="keyFormatOptions" />
        <template v-if="exportEncrypted">
          <span class="lbl">{{ t('userKeys.exportCurrentPass') }}</span>
          <FTextBox v-model="exportCurrentPass" type="password" />
          <span></span>
          <span class="hint">{{ t('userKeys.exportCurrentPassHint') }}</span>
        </template>
        <template v-if="keyFormatValue(exportKeyFormat) === 'openssh'">
          <span class="lbl">{{ t('userKeys.exportNewPass') }}</span>
          <FTextBox v-model="exportNewPass" type="password" />
          <span></span>
          <span class="hint">{{ t('userKeys.exportNewPassHint') }}</span>
        </template>
        <template v-else>
          <span></span>
          <span class="hint">{{ t('userKeys.exportFmtNoPassHint') }}</span>
        </template>
      </template>
      <template v-else>
        <span></span>
        <span class="hint">{{ t('userKeys.exportNoPrivate') }}</span>
      </template>
    </div>
    <template #footer>
      <FButton @click="exportOpen = false">{{ t('userKeys.wizCancel') }}</FButton>
      <FButton
        variant="accent"
        :disabled="exportBusy || (exportWhich === 'private' && !current?.privatePath)"
        @click="doExport"
      >
        {{ t('userKeys.exportBtn') }}
      </FButton>
    </template>
  </FDialog>

  <!-- 删除确认对话框 -->
  <FConfirmDialog
    :open="deleteOpen"
    :title="t('common.delete')"
    :message="t('userKeys.confirmDelete', { name: deleteName })"
    @update:open="deleteOpen = $event"
    @confirm="confirmDelete"
  />

  <!-- 密钥属性 -->
  <FDialog :open="propsOpen" :title="t('userKeys.propsTitle')" :width="520" @update:open="(v: boolean) => (propsOpen = v)">
    <div v-if="current" class="uk-props">
      <span>{{ t('userKeys.colName') }}</span>
      <span class="rename-row">
        <FTextBox v-model="editName" spellcheck="false" />
        <button
          class="ren-save"
          :title="t('common.save')"
          :disabled="!editName.trim() || editName.trim() === current.name"
          @click="saveRename"
        >
          <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4.5 3.5h9.3l2.7 2.7v9.3a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z" />
            <path d="M6.8 3.5v3.8h6.2V3.5M6.8 16.5v-4.6h6.4v4.6" />
          </svg>
        </button>
      </span>
      <span>{{ t('userKeys.colType') }}</span><span class="mono">{{ current.keyType }}</span>
      <span>{{ t('userKeys.colBits') }}</span><span>{{ current.bits || '—' }}</span>
      <span>{{ t('userKeys.colComment') }}</span><span class="mono break">{{ current.comment || '—' }}</span>
      <span>{{ t('userKeys.fingerprint') }}</span><span class="mono break">{{ current.fingerprint }}</span>
      <span>{{ t('userKeys.pubPath') }}</span><span class="mono break">{{ current.publicPath }}</span>
      <span>{{ t('userKeys.privPath') }}</span><span class="mono break">{{ current.privatePath || '—' }}</span>
    </div>
    <template #footer>
      <FButton variant="accent" @click="propsOpen = false">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>
</template>

<style scoped>
.uk-body {
  display: flex;
  gap: 12px;
  height: 380px;
}
.uk-list {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  background: var(--bg-card);
  overflow: hidden;
}
.uk-head,
.uk-row {
  display: grid;
  gap: 8px;
  align-items: center;
  padding: 0 10px;
}
.uk-head > span,
.uk-row > span {
  position: relative;
}
.uk-head > span {
  border-right: 1px solid var(--stroke-divider);
}
.uk-head > span:last-child {
  border-right: none;
}
.uk-rows.resizing {
  user-select: none;
  cursor: col-resize;
}
.col-grip {
  position: absolute;
  top: 0;
  right: -7px;
  width: 8px;
  height: 100%;
  cursor: col-resize;
  z-index: 2;
}
.col-grip:hover {
  background: var(--stroke-divider);
}
.col-name {
  overflow: hidden !important;
}
.col-type {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.uk-head {
  height: 28px;
  font-size: 12px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--stroke-divider);
  flex: none;
  user-select: none;
}
.uk-rows {
  flex: 1;
  overflow: auto;
}
.uk-row {
  height: 30px;
  font-size: 12.5px;
  cursor: default;
  border-radius: var(--radius-sm);
  margin: 1px 4px;
  width: calc(100% - 8px);
}
.uk-row:hover {
  background: var(--bg-alt-hover);
}
.uk-row.selected {
  background: var(--bg-selected);
}
.uk-row > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary);
}
.col-name {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--text-primary) !important;
}
.col-name svg {
  color: var(--accent);
  flex: none;
}
.col-bits {
  text-align: left;
}
.uk-empty {
  padding: 30px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12.5px;
}
.uk-ops {
  width: 108px;
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.uk-form {
  display: grid;
  grid-template-columns: 92px 1fr;
  gap: 10px 14px;
  font-size: 12.5px;
  align-items: center;
}
.uk-export-form {
  /* 在原对话框基础上加高 120px */
  min-height: 230px;
  align-content: start;
}
.uk-form > .lbl {
  color: var(--text-secondary);
  text-align: right;
}
.uk-form .f-textbox,
.uk-form .f-combo {
  width: 100%;
}
.uk-form .hint {
  color: var(--text-tertiary);
  font-size: 12px;
}
.uk-hint {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.6;
}
.uk-props {
  display: grid;
  grid-template-columns: 90px 1fr;
  gap: 10px 14px;
  font-size: 12.5px;
  align-items: center;
}
.uk-props > span:nth-child(odd) {
  color: var(--text-secondary);
  text-align: right;
}
.mono {
  font-family: 'Cascadia Mono', Consolas, monospace;
}
.rename-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.rename-row .f-textbox {
  flex: 1;
}
.ren-save {
  width: 30px;
  height: 30px;
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--stroke-control);
  border-radius: var(--radius-sm);
  background: var(--bg-card);
  color: var(--accent);
  cursor: pointer;
}
.ren-save:hover:not(:disabled) {
  background: var(--bg-alt-hover);
}
.ren-save:disabled {
  color: var(--text-disabled);
  cursor: default;
}
.break {
  word-break: break-all;
}
</style>
