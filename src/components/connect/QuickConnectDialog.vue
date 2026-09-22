<script setup lang="ts">
/**
 * 快速连接对话框：解析 ssh://user@host:port 后弹出，选择认证方式即可连接。
 * 与「新建连接」的区别：配置为临时（temporary），不保存到会话管理器/列表。
 */
import { computed, reactive, ref, watch } from 'vue'
import { FButton, FComboBox, FDialog, FTextBox, toast } from '../../ui'
import { useSessionsStore } from '../../stores/sessions'
import { defaultProfile, type AuthKind, type SshProfile } from '../../api/types'
import { isTauri } from '../../api/adapter'
import { profileSave, userKeyList } from '../../api/backend'
import { isValidHost, isValidPort } from '../../utils/validate'
import { t } from '../../i18n'

const sessions = useSessionsStore()

const props = defineProps<{
  open: boolean
  initial: { username: string; host: string; port: number } | null
}>()
const emit = defineEmits<{ (e: 'update:open', v: boolean): void }>()

const form = reactive({
  host: '',
  port: '22',
  username: 'root',
  password: '',
  privateKeyPath: '',
  privateKeyPassphrase: '',
  userKeyName: '',
})
const authKind = ref<AuthKind>('password')
const userKeyNames = ref<string[]>([])
const busy = ref(false)
const showErr = ref(false)

const errs = computed(() => {
  const e: Record<string, string> = {}
  if (!form.host.trim()) e.host = t('validate.hostRequired')
  else if (!isValidHost(form.host)) e.host = t('validate.hostInvalid')
  if (!isValidPort(form.port)) e.port = t('validate.portInvalid')
  return e
})

function validateForm(): boolean {
  showErr.value = true
  if (Object.keys(errs.value).length) {
    toast(Object.values(errs.value)[0]!, 'error')
    return false
  }
  return true
}

const authOptions = computed(() => [
  { kind: 'password' as AuthKind, label: t('connect.auth.password') },
  { kind: 'publicKey' as AuthKind, label: t('connect.auth.privateKey') },
  { kind: 'userKey' as AuthKind, label: t('connect.auth.userKey') },
])
const authLabel = computed({
  get: () => authOptions.value.find((o) => o.kind === authKind.value)?.label ?? '',
  set: (label: string) => {
    const o = authOptions.value.find((x) => x.label === label)
    if (o) authKind.value = o.kind
  },
})

async function loadUserKeys() {
  if (!isTauri) {
    userKeyNames.value = []
    return
  }
  try {
    userKeyNames.value = (await userKeyList()).map((k) => k.name)
  } catch {
    userKeyNames.value = []
  }
  if (userKeyNames.value.length && !userKeyNames.value.includes(form.userKeyName)) {
    form.userKeyName = userKeyNames.value[0]!
  }
}

watch(
  () => props.open,
  (v) => {
    if (!v) return
    const info = props.initial
    form.host = info?.host ?? ''
    form.port = String(info?.port ?? 22)
    form.username = info?.username ?? 'root'
    form.password = ''
    form.privateKeyPath = ''
    form.privateKeyPassphrase = ''
    form.userKeyName = ''
    authKind.value = 'password'
    showErr.value = false
  },
)
watch(authKind, (k) => {
  if (k === 'userKey') void loadUserKeys()
})

async function pickPrivateKey() {
  if (!isTauri) {
    toast(t('connect.browserMockToast'))
    return
  }
  const { open } = await import('@tauri-apps/plugin-dialog')
  const file = await open({ multiple: false, title: t('connect.privateKeyFile') })
  if (typeof file === 'string') form.privateKeyPath = file
}

function reset() {
  form.host = ''
  form.port = '22'
  form.username = 'root'
  form.password = ''
  form.privateKeyPath = ''
  form.privateKeyPassphrase = ''
  form.userKeyName = ''
  authKind.value = 'password'
}

function validate(): boolean {
  if (!form.host.trim()) {
    toast(t('connect.hostRequired'), 'error')
    return false
  }
  if (authKind.value === 'publicKey' && !form.privateKeyPath.trim()) {
    toast(t('connect.privateKeyRequired'), 'error')
    return false
  }
  if (authKind.value === 'userKey' && !form.userKeyName) {
    toast(t('connect.userKeyRequired'), 'error')
    return false
  }
  return true
}

function buildProfile(): SshProfile {
  const base = defaultProfile({
    name: form.host.trim(),
    host: form.host.trim(),
    port: Number(form.port) || 22,
    username: form.username.trim() || 'root',
  })
  const auth = authKind.value
  return {
    ...base,
    auth,
    temporary: true,
    privateKeyPath: auth === 'publicKey' ? form.privateKeyPath.trim() : '',
    userKeyName: auth === 'userKey' ? form.userKeyName : '',
  }
}

function secrets(): { password?: string; passphrase?: string } | undefined {
  const s: { password?: string; passphrase?: string } = {}
  if (authKind.value === 'password' && form.password) s.password = form.password
  if (form.privateKeyPassphrase) s.passphrase = form.privateKeyPassphrase
  return Object.keys(s).length ? s : undefined
}

async function connectNow() {
  if (busy.value) return
  if (!validateForm()) return
  if (!validate()) return
  busy.value = true
  try {
    // 临时配置照常入库（后端连接按 id 读取凭据），temporary 使其不出现在任何列表
    const saved = await profileSave(buildProfile(), secrets())
    sessions.createSession(JSON.parse(JSON.stringify(saved)))
    emit('update:open', false)
    reset()
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <FDialog
    :open="open"
    :title="t('quickconnect.title')"
    :width="530"
    @update:open="(v: boolean) => !v && emit('update:open', false)"
  >
    <div class="qc-body">
      <div class="qc-grid">
      <span class="lbl">{{ t('connect.host') }}</span>
      <FTextBox v-model="form.host" spellcheck="false" :placeholder="t('connect.hostPlaceholder')" :class="{ 'cd-invalid': showErr && errs.host }" />
      <span class="lbl">{{ t('connect.port') }}</span>
      <FTextBox v-model="form.port" spellcheck="false" placeholder="1–65535" :class="{ 'cd-invalid': showErr && errs.port }" />
      <span class="lbl">{{ t('connect.username') }}</span>
      <FTextBox v-model="form.username" spellcheck="false" />
    </div>

    <div class="qc-auth-title">{{ t('connect.authSection') }}</div>
    <div class="qc-grid">
      <span class="lbl">{{ t('connect.authMethod') }}</span>
      <FComboBox v-model="authLabel" :options="authOptions.map((o) => o.label)" />
      <template v-if="authKind === 'password'">
        <span class="lbl">{{ t('connect.password') }}</span>
        <FTextBox v-model="form.password" type="password" :placeholder="t('connect.passwordPlaceholder')" />
      </template>
      <template v-else-if="authKind === 'publicKey'">
        <span class="lbl">{{ t('connect.privateKeyFile') }}</span>
        <div class="qc-row">
          <FTextBox v-model="form.privateKeyPath" :placeholder="t('connect.privateKeyPlaceholder')" />
          <FButton @click="pickPrivateKey">{{ t('common.browse') }}</FButton>
        </div>
        <span class="lbl">{{ t('connect.passphrase') }}</span>
        <FTextBox v-model="form.privateKeyPassphrase" type="password" :placeholder="t('connect.passphrasePlaceholder')" />
      </template>
      <template v-else>
        <span class="lbl">{{ t('connect.userKeyLabel') }}</span>
        <FComboBox v-if="userKeyNames.length" v-model="form.userKeyName" :options="userKeyNames" />
        <span v-else class="qc-hint">{{ t('connect.noUserKeys') }}</span>
        <span class="lbl">{{ t('connect.passphrase') }}</span>
        <FTextBox v-model="form.privateKeyPassphrase" type="password" :placeholder="t('connect.passphrasePlaceholder')" />
      </template>
    </div>
      <p class="qc-note">{{ t('quickconnect.note') }}</p>
    </div>

    <template #footer>
      <FButton @click="emit('update:open', false)">{{ t('common.cancel') }}</FButton>
      <FButton variant="accent" :disabled="busy" @click="connectNow">{{ t('common.connect') }}</FButton>
    </template>
  </FDialog>
</template>

<style scoped>
.qc-body {
  /* 高度在原基础上加 100px */
  min-height: 380px;
}
.qc-grid {
  display: grid;
  grid-template-columns: 92px 1fr;
  gap: 10px 14px;
  font-size: 12.5px;
  align-items: center;
}
.qc-grid .lbl {
  color: var(--text-secondary);
  text-align: right;
}
.qc-grid :deep(.f-textbox),
.qc-grid :deep(.f-combo) {
  width: 100%;
}
.qc-row {
  display: flex;
  gap: 8px;
}
.qc-row :deep(.f-textbox) {
  flex: 1;
}
.qc-auth-title {
  font-weight: 600;
  font-size: 13px;
  margin: 14px 0 10px;
  color: var(--text-primary);
}
.qc-hint {
  color: var(--text-tertiary);
  font-size: 12px;
}
.qc-note {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
}
</style>

.cd-invalid {
  border-color: var(--danger) !important;
}

