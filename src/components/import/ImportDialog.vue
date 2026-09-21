<script setup lang="ts">
/**
 * 会话导入对话框：扫描本机 Xshell（.xsh）与 Tabby（config.yaml）会话，
 * 勾选后写入 HiShell 连接配置（凭据加密存储、不可迁移，导入后按需输入密码）。
 */
import { computed, ref, watch } from 'vue'
import { FButton, FCheckbox, FDialog, toast } from '../../ui'
import { importApply, importScan, type ImportCandidate } from '../../api/backend'
import { useProfilesStore } from '../../stores/profiles'
import { t } from '../../i18n'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const scanning = ref(false)
const importing = ref(false)
const scanned = ref(false)
const candidates = ref<ImportCandidate[]>([])
const checked = ref(new Set<string>())

const groups = computed(() => {
  const bySource = new Map<string, ImportCandidate[]>()
  for (const c of candidates.value) {
    const arr = bySource.get(c.source) ?? []
    arr.push(c)
    bySource.set(c.source, arr)
  }
  return [...bySource.entries()].map(([source, items]) => ({
    source,
    label: source === 'xshell' ? 'Xshell' : 'Tabby',
    items,
  }))
})

const checkedList = computed(() => candidates.value.filter((c) => checked.value.has(keyOf(c))))
const allChecked = computed(() => candidates.value.length > 0 && checkedList.value.length === candidates.value.length)

function keyOf(c: ImportCandidate) {
  return `${c.source}|${c.originPath}|${c.name}|${c.host}:${c.port}`
}

async function rescan() {
  scanning.value = true
  checked.value = new Set()
  try {
    candidates.value = await importScan()
    scanned.value = true
    // 默认全选
    checked.value = new Set(candidates.value.map(keyOf))
  } finally {
    scanning.value = false
  }
}

function toggleAll(v: boolean) {
  checked.value = v ? new Set(candidates.value.map(keyOf)) : new Set()
}

async function doImport() {
  if (!checkedList.value.length) return
  importing.value = true
  try {
    const r = await importApply(checkedList.value)
    await reloadProfiles()
    toast(
      t('import.doneToast', {
        imported: r.imported,
        skipped: r.skipped,
      }),
      r.imported > 0 ? 'success' : 'info',
    )
    if (r.imported > 0) {
      candidates.value = []
      checked.value = new Set()
      scanned.value = false
      emit('update:open', false)
    }
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    importing.value = false
  }
}

async function reloadProfiles() {
  const s = useProfilesStore()
  s.$patch({ loaded: false })
  await s.init()
}

watch(
  () => props.open,
  (v) => {
    if (v && !scanned.value && !scanning.value) rescan()
  },
  { immediate: true },
)
</script>

<template>
  <FDialog :open="open" :title="t('import.title')" :width="620" @update:open="emit('update:open', $event)">
    <div class="import-hint">{{ t('import.hint') }}</div>

    <div v-if="scanning" class="import-state">{{ t('import.scanning') }}</div>
    <div v-else-if="scanned && candidates.length === 0" class="import-state">{{ t('import.none') }}</div>

    <template v-else>
      <div class="import-head">
        <FCheckbox :model-value="allChecked" :label="t('import.selectAll')" @update:model-value="toggleAll" />
        <span class="import-count">{{ t('import.selectedCount', { n: checkedList.length }) }}</span>
      </div>
      <div class="import-list">
        <div v-for="g in groups" :key="g.source" class="import-group">
          <div class="import-group-title">{{ g.label }}</div>
          <label v-for="c in g.items" :key="keyOf(c)" class="import-row">
            <FCheckbox
              :model-value="checked.has(keyOf(c))"
              @update:model-value="(v: boolean) => (v ? checked.add(keyOf(c)) : checked.delete(keyOf(c)))"
            />
            <span class="ir-name" :title="c.name">{{ c.name || c.host }}</span>
            <span class="ir-addr">{{ c.username }}@{{ c.host }}:{{ c.port }}</span>
            <span class="ir-auth">{{ t(`connect.auth.${c.auth === 'keyboardInteractive' ? 'ki' : c.auth === 'publicKey' ? 'privateKey' : 'password'}`) }}</span>
          </label>
        </div>
      </div>
    </template>

    <template #footer>
      <FButton variant="subtle" :disabled="scanning" @click="rescan">{{ t('import.rescan') }}</FButton>
      <div class="import-spacer" />
      <FButton variant="subtle" @click="emit('update:open', false)">{{ t('common.cancel') }}</FButton>
      <FButton variant="accent" :disabled="scanning || importing || !checkedList.length" @click="doImport">
        {{ t('import.apply', { n: checkedList.length }) }}
      </FButton>
    </template>
  </FDialog>
</template>

<style scoped>
.import-hint {
  color: var(--text-tertiary);
  font-size: 12.5px;
  line-height: 1.7;
  margin-bottom: 14px;
}
.import-state {
  color: var(--text-secondary);
  font-size: 13px;
  padding: 26px 0;
  text-align: center;
}
.import-head {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px;
}
.import-count {
  color: var(--text-tertiary);
  font-size: 12px;
}
.import-list {
  max-height: 340px;
  overflow: auto;
  border: 1px solid var(--stroke-card);
  border-radius: var(--radius-md);
  padding: 6px;
  background: var(--bg-card);
}
.import-group-title {
  font-size: 11.5px;
  font-weight: 600;
  color: var(--text-tertiary);
  padding: 6px 8px 2px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.import-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.import-row:hover {
  background: var(--bg-alt-hover);
}
.ir-name {
  font-size: 12.5px;
  color: var(--text-primary);
  max-width: 170px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: none;
}
.ir-addr {
  flex: 1;
  font-size: 12px;
  color: var(--text-secondary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ir-auth {
  font-size: 11.5px;
  color: var(--text-tertiary);
  flex: none;
}
.import-spacer {
  flex: 1;
}
</style>
