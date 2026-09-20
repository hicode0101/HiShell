<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { FButton, FTextBox, FComboBox, FRadioGroup, FDialog, toast } from '../../ui'
import { useProfilesStore } from '../../stores/profiles'
import { useSessionsStore } from '../../stores/sessions'
import { defaultProfile, type AuthKind, type ProxyConfig, type SshProfile } from '../../api/types'
import { isTauri } from '../../api/adapter'
import { t } from '../../i18n'

const sessions = useSessionsStore()
const profilesStore = useProfilesStore()

const open = computed({
  get: () => sessions.connectDialogOpen,
  set: (v) => (sessions.connectDialogOpen = v),
})

const form = reactive<SshProfile>(defaultProfile())
const authKind = ref<AuthKind>('password')
const proxyKind = ref<'none' | 'socks5' | 'http' | 'jump' | 'proxyCommand'>('none')
const socks = reactive({ host: '127.0.0.1', port: 1080, username: '', password: '' })
const http = reactive({ host: '127.0.0.1', port: 8080, username: '', password: '' })
const jumpProfileId = ref('')
const testing = ref(false)

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
})

/* 认证：稳定值 + 本地化标签 */
const authOptions = computed(() => [
  { kind: 'password' as AuthKind, label: t('connect.auth.password') },
  { kind: 'publicKey' as AuthKind, label: t('connect.auth.privateKey') },
  { kind: 'keyboardInteractive' as AuthKind, label: t('connect.auth.ki') },
  { kind: 'agent' as AuthKind, label: t('connect.auth.agent') },
])
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
  { label: t('connect.proxyCommand'), value: 'proxyCommand', disabled: true },
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
    default:
      return { kind: 'none' }
  }
}

function fillFormForEdit() {
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
  testing.value = true
  await new Promise((r) => setTimeout(r, 900))
  testing.value = false
  toast(t('connect.testOk'), 'success')
}

function saveProfile() {
  form.auth = authKind.value
  form.proxy = buildProxy()
  if (!form.host) {
    toast(t('connect.hostRequired'), 'error')
    return
  }
  if (!form.name) form.name = form.host
  profilesStore.upsert(JSON.parse(JSON.stringify(form)))
  toast(t('connect.savedToast', { name: form.name }), 'success')
  open.value = false
}

function connectNow() {
  form.auth = authKind.value
  form.proxy = buildProxy()
  if (!form.host) {
    toast(t('connect.hostRequired'), 'error')
    return
  }
  if (!form.name) form.name = form.host
  profilesStore.upsert(JSON.parse(JSON.stringify(form)))
  sessions.createSession(JSON.parse(JSON.stringify(form)))
  open.value = false
}
</script>

<template>
  <FDialog v-model:open="open" :title="sessions.connectDialogProfileId ? t('connect.editTitle') : t('connect.newTitle')" :width="640">
    <div class="cd-section">{{ t('connect.general') }}</div>
    <div class="cd-grid">
      <span class="cd-label">{{ t('connect.name') }}</span>
      <FTextBox v-model="form.name" :placeholder="t('connect.namePlaceholder')" />
      <span class="cd-label">{{ t('connect.host') }}</span>
      <FTextBox v-model="form.host" :placeholder="t('connect.hostPlaceholder')" />
      <span class="cd-label">{{ t('connect.port') }}</span>
      <FTextBox v-model="form.port" placeholder="22" />
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
        />
      </template>
      <template v-else-if="authKind === 'publicKey'">
        <span class="cd-label">{{ t('connect.privateKeyFile') }}</span>
        <div class="cd-row">
          <FTextBox v-model="form.privateKeyPath" :placeholder="t('connect.privateKeyPlaceholder')" />
          <FButton @click="pickPrivateKey">{{ t('common.browse') }}</FButton>
        </div>
        <span class="cd-label">{{ t('connect.passphrase') }}</span>
        <FTextBox v-model="form.privateKeyPassphrase" type="password" :placeholder="t('connect.passphrasePlaceholder')" />
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
          <FTextBox v-model="socks.host" :placeholder="t('connect.proxyServer')" />
          <FTextBox v-model="socks.port" :placeholder="t('connect.proxyPort')" class="cd-port" />
        </div>
        <span class="cd-label">{{ t('connect.proxyUser') }}</span>
        <FTextBox v-model="socks.username" :placeholder="t('connect.proxyUserOptional')" />
        <span class="cd-label">{{ t('connect.proxyPass') }}</span>
        <FTextBox v-model="socks.password" type="password" :placeholder="t('connect.proxyUserPlain')" />
      </div>
      <div v-else-if="proxyKind === 'http'" class="cd-grid cd-proxy-fields">
        <span class="cd-label">{{ t('connect.httpAddr') }}</span>
        <div class="cd-row">
          <FTextBox v-model="http.host" :placeholder="t('connect.proxyServer')" />
          <FTextBox v-model="http.port" :placeholder="t('connect.proxyPort')" class="cd-port" />
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
    </div>

    <div class="cd-section">{{ t('connect.terminal') }}</div>
    <div class="cd-grid">
      <span class="cd-label">{{ t('connect.termType') }}</span>
      <FComboBox v-model="form.term" :options="['xterm-256color', 'xterm', 'vt100', 'linux']" />
      <span class="cd-label">{{ t('connect.keepalive') }}</span>
      <div class="cd-row">
        <FTextBox v-model="form.keepaliveInterval" placeholder="5000 (ms)" class="cd-port" />
        <span class="cd-hint">{{ t('connect.keepaliveUnit') }}</span>
      </div>
      <span class="cd-label">{{ t('connect.connectTimeout') }}</span>
      <FTextBox v-model="form.connectTimeout" placeholder="10000 (ms)" class="cd-port" />
    </div>

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
  grid-template-columns: 96px 1fr;
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
.footer-spacer {
  flex: 1;
}
</style>
