<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { FButton, FTextBox, FComboBox, FRadioGroup, FDialog, FCheckbox, toast } from '../../ui'
import { useProfilesStore } from '../../stores/profiles'
import { useSessionsStore } from '../../stores/sessions'
import { defaultProfile, type AuthKind, type ProxyConfig, type SshProfile } from '../../api/types'
import { isTauri } from '../../api/adapter'
import { isValidHost, isValidInt, isValidPort } from '../../utils/validate'
import { sshTest, userKeyList } from '../../api/backend'
import { t } from '../../i18n'

const sessions = useSessionsStore()
const profilesStore = useProfilesStore()

const emit = defineEmits<{ (e: 'connected'): void }>()

const open = computed({
  get: () => sessions.connectDialogOpen,
  set: (v) => (sessions.connectDialogOpen = v),
})

const form = reactive<SshProfile>(defaultProfile())
const authKind = ref<AuthKind>('password')
const proxyKind = ref<'none' | 'socks5' | 'http' | 'jump' | 'proxyCommand'>('none')
const socks = reactive({ host: '127.0.0.1', port: 1080, username: '', password: '' })
const http = reactive({ host: '127.0.0.1', port: 8080, username: '', password: '' })
const proxyCmd = reactive({ command: '' })
const jumpProfileId = ref('')
const testing = ref(false)
const showErr = ref(false)

/** 实时校验：返回 { 字段: 错误文案 }（仅统计当前选择的代理类型相关字段） */
const errs = computed(() => {
  const e: Record<string, string> = {}
  if (!form.host.trim()) e.host = t('validate.hostRequired')
  else if (!isValidHost(form.host)) e.host = t('validate.hostInvalid')
  if (!isValidPort(form.port)) e.port = t('validate.portInvalid')
  if (!isValidInt(form.keepaliveInterval, 1000, 3600000)) e.keepalive = t('validate.keepaliveInvalid')
  if (!isValidInt(form.connectTimeout, 1000, 600000)) e.timeout = t('validate.timeoutInvalid')
  if (proxyKind.value === 'socks5' || proxyKind.value === 'http') {
    const hs = proxyKind.value === 'socks5' ? socks.host : http.host
    const pt = proxyKind.value === 'socks5' ? socks.port : http.port
    if (!isValidHost(hs)) e.proxyHost = t('validate.proxyHostInvalid')
    if (!isValidPort(pt)) e.proxyPort = t('validate.proxyPortInvalid')
  }
  if (proxyKind.value === 'proxyCommand' && !proxyCmd.command.trim()) {
    e.proxyCmd = t('validate.proxyCmdRequired')
  }
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

/** 校验通过后把数字字段统一转为数值（后端 serde 要求） */
function coerceNumbers() {
  form.port = Number(form.port)
  form.keepaliveInterval = Number(form.keepaliveInterval)
  form.connectTimeout = Number(form.connectTimeout)
}

// 切换密钥时清空口令输入（口令随密钥走，避免旧口令误导）
watch(
  () => [form.userKeyName, form.privateKeyPath],
  () => {
    form.privateKeyPassphrase = ''
  },
)

watch(open, (v) => {
  if (!v) return
  const editId = sessions.connectDialogProfileId
  if (editId) {
    const p = profilesStore.byId(editId)
    if (p) Object.assign(form, defaultProfile(), JSON.parse(JSON.stringify(p)))
  } else {
    Object.assign(form, defaultProfile())
  }
  fillFormForEdit()
  showErr.value = false
})

/* 认证：稳定值 + 本地化标签 */
const authOptions = computed(() => [
  { kind: 'password' as AuthKind, label: t('connect.auth.password') },
  { kind: 'priFileKey' as AuthKind, label: t('connect.auth.privateKey') },
  { kind: 'userKey' as AuthKind, label: t('connect.auth.userKey') },
  { kind: 'keyboardInteractive' as AuthKind, label: t('connect.auth.ki') },
  { kind: 'agent' as AuthKind, label: t('connect.auth.agent') },
])

/* 用户密钥列表（auth = userKey 时供选择） */
const userKeyNames = ref<string[]>([])
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
  // 未选定时自动选第一个可用密钥
  if (userKeyNames.value.length && !userKeyNames.value.includes(form.userKeyName)) {
    form.userKeyName = userKeyNames.value[0]!
  }
}
watch([open, authKind], ([o, k]) => {
  if (o && k === 'userKey') void loadUserKeys()
})
const authLabel = computed({
  get: () => authOptions.value.find((o) => o.kind === authKind.value)?.label ?? '',
  set: (label: string) => {
    const o = authOptions.value.find((x) => x.label === label)
    if (o) authKind.value = o.kind
  },
})

const proxyOptions = computed(() => [
  { label: t('connect.proxyNone'), value: 'none' },
  { label: t('connect.proxySocks5'), value: 'socks5' },
  { label: t('connect.proxyHttp'), value: 'http' },
  { label: t('connect.proxyJump'), value: 'jump' },
  { label: t('connect.proxyCommand'), value: 'proxyCommand' },
])

const jumpOptions = computed(() =>
  profilesStore.jumpCandidates
    .filter((p) => p.id !== form.id)
    .map((p) => `${p.name}（${p.username}@${p.host}:${p.port}）`)
    .concat([t('connect.jumpEmpty')]),
)

function buildProxy(): ProxyConfig {
  switch (proxyKind.value) {
    case 'socks5':
      return {
        kind: 'socks5',
        host: socks.host,
        port: Number(socks.port) || 1080,
        username: socks.username || undefined,
        password: socks.password || undefined,
      }
    case 'http':
      return {
        kind: 'http',
        host: http.host,
        port: Number(http.port) || 8080,
        username: http.username || undefined,
        password: http.password || undefined,
      }
    case 'jump': {
      const target = profilesStore.jumpCandidates.find(
        (p) => `${p.name}（${p.username}@${p.host}:${p.port}）` === jumpProfileId.value,
      )
      return { kind: 'jump', profileId: target?.id ?? '' }
    }
    case 'proxyCommand':
      return { kind: 'proxyCommand', command: proxyCmd.command }
    default:
      return { kind: 'none' }
  }
}

function fillFormForEdit() {
  // 已存储凭据标志 → 决定密码/口令字段是否显示“清除已存”扫把图标
  storedPassword.value = !!profilesStore.byId(sessions.connectDialogProfileId)?.hasStoredPassword
  storedPassphrase.value = !!profilesStore.byId(sessions.connectDialogProfileId)?.hasStoredPassphrase
  clearPasswordIntent.value = false
  clearPassphraseIntent.value = false
  const p = form.proxy
  if (p.kind === 'socks5') {
    proxyKind.value = 'socks5'
    Object.assign(socks, { host: p.host, port: p.port, username: p.username ?? '', password: p.password ?? '' })
  } else if (p.kind === 'http') {
    proxyKind.value = 'http'
    Object.assign(http, { host: p.host, port: p.port, username: p.username ?? '', password: p.password ?? '' })
  } else if (p.kind === 'jump') {
    proxyKind.value = 'jump'
    const target = profilesStore.byId(p.profileId)
    jumpProfileId.value = target ? `${target.name}（${target.username}@${target.host}:${target.port}）` : ''
  } else if (p.kind === 'proxyCommand') {
    proxyKind.value = 'proxyCommand'
    proxyCmd.command = p.command ?? ''
  } else {
    proxyKind.value = 'none'
  }
  authKind.value = form.auth
}

watch(proxyKind, (k) => {
  if (k === 'jump' && !jumpProfileId.value && jumpOptions.value.length) {
    jumpProfileId.value = jumpOptions.value[0]!
  }
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

async function testConnection() {
  if (!form.host) {
    toast(t('connect.hostRequired'), 'error')
    return
  }
  if (authKind.value === 'userKey' && !form.userKeyName) {
    toast(t('connect.userKeyRequired'), 'error')
    return
  }
  if (!isTauri) {
    toast(t('connect.testOk'))
    return
  }
  if (!validateForm()) return
  coerceNumbers()
  resetOtherAuthFields()
  testing.value = true
  try {
    form.auth = authKind.value
    form.proxy = buildProxy()
    const r = await sshTest(JSON.parse(JSON.stringify(form)), {
      password: form.password || undefined,
      passphrase: form.privateKeyPassphrase || undefined,
    })
    toast(r.message, r.ok ? 'success' : 'error')
  } catch (e) {
    toast(String(e), 'error')
  } finally {
    testing.value = false
  }
}

/** 已存储凭据标志（编辑已有配置时由 hasStoredPassword/hasStoredPassphrase 驱动清除图标） */
const storedPassword = ref(false)
const storedPassphrase = ref(false)
/** 用户点击扫把图标后，保存时显式清除对应已存凭据 */
const clearPasswordIntent = ref(false)
const clearPassphraseIntent = ref(false)

function clearStored(kind: 'password' | 'passphrase') {
  if (kind === 'password') {
    clearPasswordIntent.value = true
    form.password = ''
  } else {
    clearPassphraseIntent.value = true
    form.privateKeyPassphrase = ''
  }
}

/** 表单中已输入的明文凭据 → 传给后端存入系统凭据管理器（留空 = 保持已存值不变；
 *  点击过扫把图标的字段 = 显式清除已存值） */
function secretsFromForm() {
  const secrets: {
    password?: string
    passphrase?: string
    clearPassword?: boolean
    clearPassphrase?: boolean
  } = {}
  if (clearPasswordIntent.value) secrets.clearPassword = true
  if (clearPassphraseIntent.value) secrets.clearPassphrase = true
  if (form.password) secrets.password = form.password
  if (form.privateKeyPassphrase) secrets.passphrase = form.privateKeyPassphrase
  return Object.keys(secrets).length ? secrets : undefined
}

/** 切换认证方式后保存/连接时，重置其它认证方式的配置项（口令为两种密钥方式共用，保留） */
function resetOtherAuthFields() {
  if (authKind.value !== 'password') form.password = ''
  if (authKind.value === 'priFileKey') form.userKeyName = ''
  if (authKind.value === 'userKey') form.privateKeyPath = ''
  if (authKind.value !== 'priFileKey' && authKind.value !== 'userKey') form.privateKeyPassphrase = ''
}

/** MCP 授权（SshProfile 中为可选字段，转成确定布尔值驱动勾选框） */
const mcpAuthorizedModel = computed({
  get: () => !!form.mcpAuthorized,
  set: (v: boolean) => (form.mcpAuthorized = v),
})

async function saveProfile() {
  form.auth = authKind.value
  form.proxy = buildProxy()
  if (!validateForm()) return
  if (authKind.value === 'userKey' && !form.userKeyName) {
    toast(t('connect.userKeyRequired'), 'error')
    return
  }
  coerceNumbers()
  resetOtherAuthFields()
  if (!form.name) form.name = form.host
  const saved = await profilesStore.upsert(JSON.parse(JSON.stringify(form)), secretsFromForm())
  storedPassword.value = !!saved.hasStoredPassword
  storedPassphrase.value = !!saved.hasStoredPassphrase
  clearPasswordIntent.value = false
  clearPassphraseIntent.value = false
  toast(t('connect.savedToast', { name: form.name }), 'success')
  open.value = false
}

async function connectNow() {
  form.auth = authKind.value
  form.proxy = buildProxy()
  if (!validateForm()) return
  if (authKind.value === 'userKey' && !form.userKeyName) {
    toast(t('connect.userKeyRequired'), 'error')
    return
  }
  coerceNumbers()
  resetOtherAuthFields()
  if (!form.name) form.name = form.host
  // 先持久化（含凭据），再发起连接——避免后端尚未读到该配置的竞态
  const saved = await profilesStore.upsert(JSON.parse(JSON.stringify(form)), secretsFromForm())
  sessions.createSession(JSON.parse(JSON.stringify(saved)))
  open.value = false
  // 通知外层：若会话管理器打开着（从列表新建/编辑进入），一并关闭
  emit('connected')
}
</script>

<template>
  <FDialog v-model:open="open" :title="sessions.connectDialogProfileId ? t('connect.editTitle') : t('connect.newTitle')" :width="640">
    <div class="cd-section">{{ t('connect.general') }}</div>
    <div class="cd-grid">
      <span class="cd-label">{{ t('connect.name') }}</span>
      <FTextBox v-model="form.name" :placeholder="t('connect.namePlaceholder')" />
      <span class="cd-label">{{ t('connect.host') }}</span>
      <FTextBox v-model="form.host" :placeholder="t('connect.hostPlaceholder')" :class="{ 'cd-invalid': showErr && errs.host }" />
      <span class="cd-label">{{ t('connect.port') }}</span>
      <FTextBox v-model="form.port" placeholder="1–65535" :class="{ 'cd-invalid': showErr && errs.port }" />
      <span class="cd-label">{{ t('connect.username') }}</span>
      <FTextBox v-model="form.username" placeholder="root" />
    </div>

    <div class="cd-section">{{ t('connect.sectionAuth') }}</div>
    <div class="cd-grid">
      <span class="cd-label">{{ t('connect.authMethod') }}</span>
      <FComboBox v-model="authLabel" :options="authOptions.map((o) => o.label)" />
      <template v-if="authKind === 'password'">
        <span class="cd-label">{{ t('connect.password') }}</span>
        <FTextBox
          v-model="form.password"
          type="password"
          :placeholder="form.hasStoredPassword ? t('prompt.savedHint') : t('connect.passwordPlaceholder')"
          :stored-clearable="storedPassword"
          @clear-stored="clearStored('password')"
        />
      </template>
      <template v-else-if="authKind === 'priFileKey'">
        <span class="cd-label">{{ t('connect.privateKeyFile') }}</span>
        <div class="cd-row">
          <FTextBox v-model="form.privateKeyPath" :placeholder="t('connect.privateKeyPlaceholder')" />
          <FButton @click="pickPrivateKey">{{ t('common.browse') }}</FButton>
        </div>
        <span class="cd-label">{{ t('connect.passphrase') }}</span>
        <FTextBox
          v-model="form.privateKeyPassphrase"
          type="password"
          :placeholder="t('connect.passphrasePlaceholder')"
          :stored-clearable="storedPassphrase"
          @clear-stored="clearStored('passphrase')"
        />
      </template>
      <template v-else-if="authKind === 'userKey'">
        <span class="cd-label">{{ t('connect.userKeyLabel') }}</span>
        <FComboBox v-if="userKeyNames.length" v-model="form.userKeyName" :options="userKeyNames" />
        <span v-else class="cd-hint">{{ t('connect.noUserKeys') }}</span>
        <span class="cd-label">{{ t('connect.passphrase') }}</span>
        <FTextBox
          v-model="form.privateKeyPassphrase"
          type="password"
          :placeholder="t('connect.passphrasePlaceholder')"
          :stored-clearable="storedPassphrase"
          @clear-stored="clearStored('passphrase')"
        />
      </template>
      <template v-else-if="authKind === 'keyboardInteractive'">
        <span class="cd-label">{{ t('connect.hint') }}</span>
        <span class="cd-hint">{{ t('connect.kiHint') }}</span>
      </template>
      <template v-else>
        <span class="cd-label">{{ t('connect.hint') }}</span>
        <span class="cd-hint">{{ t('connect.agentHint') }}</span>
      </template>
    </div>

    <div class="cd-section">{{ t('connect.proxy') }}</div>
    <div class="cd-proxy">
      <FRadioGroup v-model="proxyKind" :options="proxyOptions" />
      <div v-if="proxyKind === 'socks5'" class="cd-grid cd-proxy-fields">
        <span class="cd-label">{{ t('connect.socksAddr') }}</span>
        <div class="cd-row">
          <FTextBox v-model="socks.host" :placeholder="t('connect.proxyServer')" :class="{ 'cd-invalid': showErr && errs.proxyHost }" />
          <FTextBox v-model="socks.port" :placeholder="t('connect.proxyPort')" class="cd-port" :class="{ 'cd-invalid': showErr && errs.proxyPort }" />
        </div>
        <span class="cd-label">{{ t('connect.proxyUser') }}</span>
        <FTextBox v-model="socks.username" :placeholder="t('connect.proxyUserOptional')" />
        <span class="cd-label">{{ t('connect.proxyPass') }}</span>
        <FTextBox v-model="socks.password" type="password" :placeholder="t('connect.proxyUserPlain')" />
      </div>
      <div v-else-if="proxyKind === 'http'" class="cd-grid cd-proxy-fields">
        <span class="cd-label">{{ t('connect.httpAddr') }}</span>
        <div class="cd-row">
          <FTextBox v-model="http.host" :placeholder="t('connect.proxyServer')" :class="{ 'cd-invalid': showErr && errs.proxyHost }" />
          <FTextBox v-model="http.port" :placeholder="t('connect.proxyPort')" class="cd-port" :class="{ 'cd-invalid': showErr && errs.proxyPort }" />
        </div>
        <span class="cd-label">{{ t('connect.proxyUser') }}</span>
        <FTextBox v-model="http.username" :placeholder="t('connect.proxyUserPlain')" />
        <span class="cd-label">{{ t('connect.proxyPass') }}</span>
        <FTextBox v-model="http.password" type="password" :placeholder="t('connect.proxyUserPlain')" />
      </div>
      <div v-else-if="proxyKind === 'jump'" class="cd-grid cd-proxy-fields">
        <span class="cd-label">{{ t('connect.jumpHost') }}</span>
        <FComboBox v-model="jumpProfileId" :options="jumpOptions" :placeholder="t('connect.jumpPlaceholder')" />
        <span class="cd-label">{{ t('connect.hint') }}</span>
        <span class="cd-hint">{{ t('connect.jumpHint') }}</span>
      </div>
      <div v-else-if="proxyKind === 'proxyCommand'" class="cd-grid cd-proxy-fields">
        <span class="cd-label">{{ t('connect.proxyCommand') }}</span>
        <FTextBox v-model="proxyCmd.command" :placeholder="t('connect.proxyCommandPlaceholder')" :class="{ 'cd-invalid': showErr && errs.proxyCmd }" />
        <span class="cd-label">{{ t('connect.hint') }}</span>
        <span class="cd-hint">{{ t('connect.proxyCommandHint') }}</span>
      </div>
    </div>

    <div class="cd-section">{{ t('connect.terminal') }}</div>
    <div class="cd-grid">
      <span class="cd-label">{{ t('connect.termType') }}</span>
      <FComboBox v-model="form.term" :options="['xterm-256color', 'xterm', 'vt100', 'linux']" />
      <span class="cd-label">{{ t('connect.keepalive') }}</span>
      <div class="cd-row">
        <FTextBox v-model="form.keepaliveInterval" placeholder="5000（1000–3600000 ms）" class="cd-port" :class="{ 'cd-invalid': showErr && errs.keepalive }" />
        <span class="cd-hint">{{ t('connect.keepaliveUnit') }}</span>
      </div>
      <span class="cd-label">{{ t('connect.connectTimeout') }}</span>
      <FTextBox v-model="form.connectTimeout" placeholder="10000（1000–600000 ms）" class="cd-port" :class="{ 'cd-invalid': showErr && errs.timeout }" />
    </div>

    <div class="cd-section">{{ t('connect.mcpSection') }}</div>
    <div class="cd-grid">
      <span class="cd-label">{{ t('connect.mcpAuthLabel') }}</span>
      <div>
        <FCheckbox v-model="mcpAuthorizedModel" :label="t('connect.mcpAuthCheckbox')" />
      </div>
    </div>
    <div class="cd-hint cd-mcp-hint">{{ t('connect.mcpHint') }}</div>

    <template #footer>
      <FButton variant="subtle" :disabled="testing" @click="testConnection">{{ testing ? t('common.testing') : t('common.testConnection') }}</FButton>
      <div class="footer-spacer" />
      <FButton variant="subtle" @click="open = false">{{ t('common.cancel') }}</FButton>
      <FButton @click="saveProfile">{{ t('common.save') }}</FButton>
      <FButton variant="accent" @click="connectNow">{{ t('common.connect') }}</FButton>
    </template>
  </FDialog>
</template>

<style scoped>
.cd-section {
  font-weight: 600;
  font-size: 13px;
  margin: 16px 0 10px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--stroke-divider);
}
.cd-section:first-child {
  margin-top: 0;
}
.cd-grid {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr);
  gap: 10px 12px;
  align-items: center;
}
.cd-label {
  color: var(--text-secondary);
  text-align: right;
  font-size: 12.5px;
}
.cd-hint {
  color: var(--text-tertiary);
  font-size: 12.5px;
  line-height: 1.5;
}
.cd-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.cd-row > :first-child {
  flex: 1;
}
.cd-port {
  width: 96px;
  flex: none;
}
.cd-proxy-fields {
  margin-top: 12px;
  padding-left: 8px;
}
.cd-mcp-hint {
  margin: 8px 0 0 108px;
}
.footer-spacer {
  flex: 1;
}
</style>

.cd-invalid :deep(input),
.cd-invalid {
  border-color: var(--danger) !important;
}

