<script setup lang="ts">
/**
 * 会话管理器：列出全部连接配置，支持 连接 / 编辑 / 删除 / 新建。
 * 删除通过确认对话框二次确认，避免误删。
 */
import { computed, ref } from 'vue'
import { FButton, FConfirmDialog, FDialog, toast } from '../ui'
import { useProfilesStore } from '../stores/profiles'
import { useSessionsStore } from '../stores/sessions'
import type { SshProfile } from '../api/types'
import { t } from '../i18n'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const profiles = useProfilesStore()
const sessions = useSessionsStore()
const deleteTarget = ref<SshProfile | null>(null)

const rows = computed(() =>
  profiles.profiles.map((p) => ({
    profile: p,
    addr: `${p.username}@${p.host}:${p.port}`,
    proxy: proxyLabel(p),
  })),
)

function proxyLabel(p: SshProfile): string {
  switch (p.proxy.kind) {
    case 'socks5':
      return `SOCKS5 ${p.proxy.host}:${p.proxy.port}`
    case 'http':
      return `HTTP ${p.proxy.host}:${p.proxy.port}`
    case 'jump':
      return t('sessionToolbar.jump', { name: profiles.byId(p.proxy.profileId)?.name ?? '?' })
    case 'proxyCommand':
      return t('sessionToolbar.proxyCommand')
    default:
      return ''
  }
}

function connect(p: SshProfile) {
  sessions.createSession(JSON.parse(JSON.stringify(p)))
  emit('update:open', false)
}

function edit(p: SshProfile) {
  sessions.openConnectDialog(p.id)
  emit('update:open', false)
}

function remove(p: SshProfile) {
  deleteTarget.value = p
}

async function confirmRemove() {
  const p = deleteTarget.value
  if (!p) return
  await profiles.remove(p.id)
  toast(t('sessions.deletedToast', { name: p.name }), 'success')
  deleteTarget.value = null
}
</script>

<template>
  <FDialog :open="open" :title="t('sessions.title')" :width="620" @update:open="emit('update:open', $event)">
    <div v-if="!rows.length" class="sm-empty">{{ t('sessions.empty') }}</div>
    <div v-else class="sm-list">
      <div v-for="row in rows" :key="row.profile.id" class="sm-row">
        <div class="sm-main">
          <div class="sm-name">
            {{ row.profile.name }}
            <span v-if="row.proxy" class="sm-proxy">{{ row.proxy }}</span>
          </div>
          <div class="sm-addr">{{ row.addr }} · {{ t(`connect.auth.${row.profile.auth === 'keyboardInteractive' ? 'ki' : row.profile.auth === 'publicKey' ? 'privateKey' : row.profile.auth === 'agent' ? 'agent' : 'password'}`) }}</div>
        </div>
        <div class="sm-ops">
          <FButton variant="accent" @click="connect(row.profile)">{{ t('common.connect') }}</FButton>
          <FButton variant="subtle" @click="edit(row.profile)">{{ t('common.edit') }}</FButton>
          <FButton variant="danger" @click="remove(row.profile)">
            {{ t('common.delete') }}
          </FButton>
        </div>
      </div>
    </div>
    <template #footer>
      <FButton @click="sessions.openConnectDialog(); emit('update:open', false)">
        {{ t('common.newConnection') }}
      </FButton>
      <div class="sm-spacer" />
      <FButton variant="subtle" @click="emit('update:open', false)">{{ t('common.close') }}</FButton>
    </template>
  </FDialog>

  <!-- 删除连接确认对话框 -->
  <FConfirmDialog
    :open="!!deleteTarget"
    :title="t('common.delete')"
    :message="t('sessions.confirmDelete', { name: deleteTarget?.name ?? '' })"
    @update:open="(v: boolean) => !v && (deleteTarget = null)"
    @confirm="confirmRemove"
  />
</template>

<style scoped>
.sm-empty {
  padding: 30px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
}
.sm-list {
  max-height: 380px;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.sm-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 10px;
  border-radius: var(--radius-md);
}
.sm-row:hover {
  background: var(--bg-alt-hover);
}
.sm-main {
  flex: 1;
  min-width: 0;
}
.sm-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 8px;
}
.sm-proxy {
  font-size: 10.5px;
  font-weight: 500;
  color: var(--accent);
  background: var(--accent-light);
  border-radius: 9px;
  padding: 1px 8px;
}
.sm-addr {
  font-size: 12px;
  color: var(--text-tertiary);
  font-family: 'Cascadia Mono', Consolas, monospace;
  margin-top: 3px;
}
.sm-ops {
  display: flex;
  gap: 6px;
  flex: none;
}
.sm-spacer {
  flex: 1;
}
</style>
