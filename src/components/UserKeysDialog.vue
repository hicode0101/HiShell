<script setup lang="ts">
/**
 * 用户密钥管理（参考 Xshell 用户密钥）：列出 ~/.ssh 下的密钥，
 * 右侧操作：生成 / 属性 / 删除 / 导入 / 导出。
 * 生成、导入、导出的具体逻辑按需求后续接入（当前提示待接入）。
 */
import { computed, ref, watch } from 'vue'
import { FButton, FDialog, toast } from '../ui'
import { userKeyList, userKeyDelete, type UserKeyInfo } from '../api/backend'
import { t } from '../i18n'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const keys = ref<UserKeyInfo[]>([])
const selected = ref('')
const loading = ref(false)
const confirmingDelete = ref(false)
const propsOpen = ref(false)

const current = computed(() => keys.value.find((k) => k.name === selected.value))

async function refresh() {
  loading.value = true
  try {
    keys.value = await userKeyList()
    if (!keys.value.some((k) => k.name === selected.value)) {
      selected.value = keys.value[0]?.name ?? ''
    }
    confirmingDelete.value = false
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
  confirmingDelete.value = false
}

/* 待接入功能（生成/导入/导出，逻辑等需求确认） */
function pendingFeature() {
  toast(t('userKeys.todoToast'))
}

function openProps() {
  if (!current.value) return
  propsOpen.value = true
}

async function removeKey() {
  const k = current.value
  if (!k) return
  // 两段确认：再点一次生效
  if (!confirmingDelete.value) {
    confirmingDelete.value = true
    return
  }
  try {
    await userKeyDelete(k.publicPath, k.privatePath)
    toast(t('userKeys.deletedToast', { name: k.name }), 'success')
    selected.value = ''
    await refresh()
  } catch (e) {
    toast(String(e), 'error')
  }
}
</script>

<template>
  <FDialog :open="open" :title="t('userKeys.title')" :width="700" @update:open="emit('update:open', $event)">
    <div class="uk-body">
      <div class="uk-list">
        <div class="uk-head">
          <span class="col-name">{{ t('userKeys.colName') }}</span>
          <span class="col-type">{{ t('userKeys.colType') }}</span>
          <span class="col-bits">{{ t('userKeys.colBits') }}</span>
          <span class="col-comment">{{ t('userKeys.colComment') }}</span>
        </div>
        <div class="uk-rows">
          <div v-if="loading" class="uk-empty">{{ t('sftp.loading') }}</div>
          <div v-else-if="!keys.length" class="uk-empty">{{ t('userKeys.empty') }}</div>
          <div
            v-for="k in keys"
            :key="k.name"
            class="uk-row"
            :class="{ selected: k.name === selected }"
            :title="k.publicPath"
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
        <FButton @click="pendingFeature">{{ t('userKeys.generate') }}</FButton>
        <FButton :disabled="!current" @click="openProps">{{ t('userKeys.props') }}</FButton>
        <FButton variant="danger" :disabled="!current" @click="removeKey">
          {{ confirmingDelete ? t('userKeys.confirmDelete') : t('common.delete') }}
        </FButton>
        <FButton @click="pendingFeature">{{ t('userKeys.import') }}</FButton>
        <FButton :disabled="!current" @click="pendingFeature">{{ t('userKeys.export') }}</FButton>
      </div>
    </div>

    <template #footer>
      <FButton variant="accent" @click="emit('update:open', false)">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>

  <!-- 密钥属性 -->
  <FDialog :open="propsOpen" :title="t('userKeys.propsTitle')" :width="520" @update:open="(v: boolean) => (propsOpen = v)">
    <div v-if="current" class="uk-props">
      <span>{{ t('userKeys.colName') }}</span><span class="mono">{{ current.name }}</span>
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
  grid-template-columns: 150px 110px 60px 1fr;
  gap: 8px;
  align-items: center;
  padding: 0 10px;
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
.break {
  word-break: break-all;
}
</style>
