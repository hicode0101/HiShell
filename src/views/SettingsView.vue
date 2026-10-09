<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useSettingsStore, ACCENT_PRESETS, type TerminalColorScheme } from '../stores/settings'
import { FButton, FComboBox, FSwitch, FTextBox, FConfirmDialog, toast } from '../ui'
import { t, type LangPref } from '../i18n'
import { isTauri } from '../api/adapter'
import { mcpConfigGet, mcpConfigSave, mcpTokenRegen, type McpConfigInfo } from '../api/backend'
import { useMcpServerStore } from '../stores/mcpServer'

const settings = useSettingsStore()
const mcpServerStore = useMcpServerStore()

const nav = computed(() => [
  { id: 'appearance', label: t('settings.appearance'), icon: '★' },
  { id: 'terminal', label: t('settings.terminalNav'), icon: '>' },
  { id: 'connection', label: t('settings.connection'), icon: '⇄' },
  { id: 'mcp', label: t('settings.mcpNav'), icon: '⚡' },
  { id: 'update', label: t('settings.update'), icon: '↓' },
])
const activeNav = computed({
  get: () => settings.settingsNav,
  set: (id: string) => settings.setSettingsNav(id),
})

/* 语言（跟随系统 / English / 简体中文） */
const LANG_OPTIONS: { pref: LangPref; fixedLabel: string }[] = [
  { pref: 'system', fixedLabel: '' }, // fixedLabel 留空 → 使用本地化标签
  { pref: 'en-US', fixedLabel: 'English' },
  { pref: 'zh-CN', fixedLabel: '简体中文' },
]
const langLabels = computed(() =>
  LANG_OPTIONS.map((o) => ({ pref: o.pref, label: o.fixedLabel || (o.pref === 'system' ? t('settings.followSystem') : o.pref) })),
)
const langModel = computed({
  get: () => {
    const cur = langLabels.value.find((o) => o.pref === settings.s.language)
    return cur?.label ?? t('settings.followSystem')
  },
  set: (label: string) => {
    const o = langLabels.value.find((x) => x.label === label)
    if (o) settings.patch({ language: o.pref })
  },
})
const langLabelList = computed(() => langLabels.value.map((o) => o.label))

const themeModel = computed({
  get: () => (settings.theme === 'dark' ? t('settings.dark') : t('settings.light')),
  set: (v: string) => settings.patch({ theme: v === t('settings.dark') ? 'dark' : 'light' }),
})

const TERMINAL_FONTS = [
  { label: 'Cascadia Mono', value: "'Cascadia Mono', 'Cascadia Code', Consolas, monospace" },
  { label: 'Consolas', value: 'Consolas, monospace' },
  { label: 'Courier New', value: "'Courier New', monospace" },
  { label: 'Sarasa Mono SC', value: "'Sarasa Mono SC', Consolas, monospace" },
]
const fontModel = computed({
  get: () => TERMINAL_FONTS.find((f) => f.value === settings.s.terminalFontFamily)?.label ?? TERMINAL_FONTS[0]!.label,
  set: (label: string) => {
    const f = TERMINAL_FONTS.find((x) => x.label === label)
    if (f) settings.patch({ terminalFontFamily: f.value })
  },
})
const fontLabels = TERMINAL_FONTS.map((f) => f.label)
const fontSizes = ['12', '13', '14', '15', '16', '18', '20']
const fontSizeModel = computed({
  get: () => String(settings.s.terminalFontSize),
  set: (v: string) => settings.patch({ terminalFontSize: Number(v) || 14 }),
})

/* 终端配色方案 */
const SCHEMES: { value: TerminalColorScheme; label: () => string }[] = [
  { value: 'dark', label: () => t('settings.termSchemeDark') },
  { value: 'light', label: () => t('settings.termSchemeLight') },
  { value: 'followApp', label: () => t('settings.termSchemeFollow') },
]
const schemeLabels = computed(() => SCHEMES.map((s) => s.label()))
const schemeModel = computed({
  get: () => SCHEMES.find((s) => s.value === settings.s.terminalColorScheme)?.label() ?? SCHEMES[0]!.label(),
  set: (label: string) => {
    const s = SCHEMES.find((x) => x.label() === label)
    if (s) settings.patch({ terminalColorScheme: s.value })
  },
})

/* ==================== MCP Server ==================== */

const mcp = reactive({
  loaded: false,
  enabled: false,
  listenHost: '127.0.0.1',
  port: '9500',
  token: '',
  running: false,
  error: '',
})

function syncMcp(c: McpConfigInfo) {
  mcp.loaded = true
  mcp.enabled = c.enabled
  mcp.listenHost = c.listenHost
  mcp.port = String(c.port)
  mcp.token = c.token
  mcp.running = c.running
  mcp.error = c.error ?? ''
  mcpServerStore.sync(c) // 同步给状态栏指示灯
}

onMounted(async () => {
  if (!isTauri) return
  try {
    syncMcp(await mcpConfigGet())
  } catch {
    /* 后端未就绪等场景忽略，保留默认展示 */
  }
})

/** 开关 / 网络配置统一走这里：保存并按需启停（enabled=true 时按新地址启动或重启） */
async function mcpApply(enabled: boolean) {
  try {
    const c = await mcpConfigSave(enabled, mcp.listenHost.trim(), Number(mcp.port) || 0)
    syncMcp(c)
    toast(enabled ? t('settings.mcp.startedToast') : t('settings.mcp.stoppedToast'), 'success')
  } catch (e) {
    toast(String(e), 'error')
    // 回显真实状态（端口占用等失败场景后端会返回当前实际配置）
    try {
      syncMcp(await mcpConfigGet())
    } catch {
      /* ignore */
    }
  }
}

/* 恢复默认参数（二次确认）：服务关闭 + 127.0.0.1:9500，访问密钥保持不变 */
const mcpResetOpen = ref(false)
async function mcpReset() {
  try {
    syncMcp(await mcpConfigSave(false, '127.0.0.1', 9500))
    toast(t('settings.mcp.resetToast'), 'success')
  } catch (e) {
    toast(String(e), 'error')
  }
}

/* 重新生成密钥（二次确认：旧密钥即刻失效） */
const regenOpen = ref(false)
async function mcpRegen() {
  try {
    syncMcp(await mcpTokenRegen())
    toast(t('settings.mcp.tokenRegenToast'), 'success')
  } catch (e) {
    toast(String(e), 'error')
  }
}

/* 客户端配置片段（Claude Code / Codex JSON+TOML，ZCode / Trae JSON），标签页按此顺序展示 */
const MCP_CLIENTS = ['claudeCode', 'codex', 'zcode', 'trae'] as const
type McpClientKey = (typeof MCP_CLIENTS)[number]
const mcpClient = ref<McpClientKey>('claudeCode')

const mcpUrl = computed(() => `http://${mcp.listenHost}:${mcp.port}/mcp`)
const mcpSnippets = computed<Record<McpClientKey, { label: string; content: string }>>(() => {
  const auth = { Authorization: `Bearer ${mcp.token}` }
  return {
    claudeCode: {
      label: 'Claude Code',
      content: JSON.stringify({ mcpServers: { hishell: { type: 'http', url: mcpUrl.value, headers: auth } } }, null, 2),
    },
    zcode: {
      label: 'ZCode',
      // ZCode「MCP 设置 → JSON 编辑」直接可粘贴的格式：{ mcpServers: {...} }
      content: JSON.stringify({ mcpServers: { hishell: { type: 'http', url: mcpUrl.value, headers: auth } } }, null, 2),
    },
    trae: {
      label: 'Trae',
      content: JSON.stringify({ mcpServers: { hishell: { url: mcpUrl.value, headers: auth } } }, null, 2),
    },
    codex: {
      label: 'Codex',
      content: `[mcp_servers.hishell]\nurl = "${mcpUrl.value}"\nhttp_headers = { Authorization = "Bearer ${mcp.token}" }`,
    },
  }
})

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    ta.remove()
  }
  toast(t('settings.mcp.copiedToast'), 'success')
}
</script>

<template>
  <div class="settings-view">
    <div class="sv-nav">
      <div class="sv-nav-title">{{ t('settings.title') }}</div>
      <div
        v-for="item in nav"
        :key="item.id"
        class="sv-nav-item"
        :class="{ on: activeNav === item.id }"
        @click="activeNav = item.id"
      >
        <span class="sv-nav-icon">{{ item.icon }}</span>
        {{ item.label }}
      </div>
    </div>

    <div class="sv-content">
      <template v-if="activeNav === 'appearance'">
        <div class="sv-title">{{ t('settings.appearance') }}</div>

        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.language') }}</div>
          <FComboBox v-model="langModel" :options="langLabelList" class="lang-combo" />
        </div>

        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.appTheme') }}</div>
          <div class="theme-cards">
            <div class="theme-card" :class="{ on: settings.theme === 'light' }" @click="settings.patch({ theme: 'light' })">
              <div class="preview light">
                <div class="pv-bar" /><div class="pv-body" />
              </div>
              <span>{{ t('settings.light') }}</span>
            </div>
            <div class="theme-card" :class="{ on: settings.theme === 'dark' }" @click="settings.patch({ theme: 'dark' })">
              <div class="preview dark">
                <div class="pv-bar" /><div class="pv-body" />
              </div>
              <span>{{ t('settings.dark') }}</span>
            </div>
          </div>
        </div>

        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.accentColor') }}</div>
          <div class="accent-row">
            <button
              v-for="c in ACCENT_PRESETS"
              :key="c"
              class="accent-dot"
              :class="{ on: settings.accent === c }"
              :style="{ background: c }"
              @click="settings.patch({ accent: c })"
            />
          </div>
        </div>
      </template>

      <template v-else-if="activeNav === 'terminal'">
        <div class="sv-title">{{ t('settings.terminalNav') }}</div>
        <div class="sv-block">
          <div class="sv-grid">
            <span class="sv-label">{{ t('settings.font') }}</span>
            <FComboBox v-model="fontModel" :options="fontLabels" />
            <span class="sv-label">{{ t('settings.fontSize') }}</span>
            <FComboBox v-model="fontSizeModel" :options="fontSizes" />
            <span class="sv-label">{{ t('settings.colorScheme') }}</span>
            <FComboBox v-model="schemeModel" :options="schemeLabels" />
          </div>
        </div>
        <div class="sv-block">
          <div class="sv-hint">{{ t('settings.terminalHint') }}</div>
        </div>
      </template>

      <template v-else-if="activeNav === 'connection'">
        <div class="sv-title">{{ t('settings.connection') }}</div>
        <div class="sv-block">
          <div class="sv-hint">{{ t('settings.connectionHint') }}</div>
        </div>
      </template>

      <template v-else-if="activeNav === 'mcp'">
        <div class="sv-title">{{ t('settings.mcpNav') }}</div>

        <!-- 服务开关 -->
        <div class="sv-block mcp-service">
          <div class="mcp-service-text">
            <div class="sv-row-label">{{ t('settings.mcp.service') }}</div>
            <div class="sv-hint">{{ t('settings.mcp.serviceHint') }}</div>
          </div>
          <div class="mcp-service-ctl">
            <span class="mcp-badge" :class="{ on: mcp.running }">
              {{ mcp.running ? t('settings.mcp.running') : t('settings.mcp.stopped') }}
            </span>
            <FSwitch :model-value="mcp.enabled" @update:model-value="(v: boolean) => mcpApply(v)" />
          </div>
        </div>
        <div v-if="mcp.error" class="mcp-error">{{ mcp.error }}</div>

        <!-- 实时显示 AI 操作 -->
        <div class="sv-block mcp-service">
          <div class="mcp-service-text">
            <div class="sv-row-label">{{ t('settings.mcp.showActivity') }}</div>
            <div class="sv-hint">{{ t('settings.mcp.showActivityHint') }}</div>
          </div>
          <div class="mcp-service-ctl">
            <FSwitch :model-value="settings.s.mcpShowActivity" @update:model-value="(v: boolean) => settings.patch({ mcpShowActivity: v })" />
          </div>
        </div>

        <!-- 监听配置 -->
        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.mcp.network') }}</div>
          <div class="sv-grid">
            <span class="sv-label">{{ t('settings.mcp.listenHost') }}</span>
            <FTextBox v-model="mcp.listenHost" placeholder="127.0.0.1" />
            <span class="sv-label">{{ t('settings.mcp.port') }}</span>
            <FTextBox v-model="mcp.port" placeholder="9500" class="mcp-port" />
          </div>
          <div class="sv-hint mcp-hint">{{ t('settings.mcp.networkHint') }}</div>
          <FButton @click="mcpApply(mcp.enabled)">{{ t('settings.mcp.apply') }}</FButton>
        </div>

        <!-- 访问密钥 -->
        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.mcp.token') }}</div>
          <div class="mcp-token">{{ mcp.token || '—' }}</div>
          <div class="mcp-btn-row">
            <FButton :disabled="!mcp.token" @click="copyText(mcp.token)">{{ t('settings.mcp.copyToken') }}</FButton>
            <FButton variant="danger" :disabled="!mcp.token" @click="regenOpen = true">{{ t('settings.mcp.regenToken') }}</FButton>
          </div>
          <div class="sv-hint mcp-hint">{{ t('settings.mcp.tokenHint') }}</div>
        </div>

        <!-- 客户端配置一键复制 -->
        <div class="sv-block">
          <div class="sv-row-label">{{ t('settings.mcp.clients') }}</div>
          <div class="mcp-tabs">
            <button
              v-for="key in MCP_CLIENTS"
              :key="key"
              class="mcp-tab"
              :class="{ on: mcpClient === key }"
              @click="mcpClient = key"
            >
              {{ mcpSnippets[key].label }}
            </button>
          </div>
          <pre class="mcp-code">{{ mcpSnippets[mcpClient].content }}</pre>
          <div class="mcp-btn-row">
            <FButton variant="accent" @click="copyText(mcpSnippets[mcpClient].content)">{{ t('settings.mcp.copyConfig') }}</FButton>
          </div>
          <div class="sv-hint mcp-hint">{{ t('settings.mcp.clientsHint') }}</div>
        </div>

        <!-- 安全提示 -->
        <div class="sv-block">
          <div class="sv-hint mcp-security">{{ t('settings.mcp.securityHint') }}</div>
        </div>

        <!-- 恢复默认 -->
        <div class="sv-block mcp-reset-row">
          <FButton variant="danger" @click="mcpResetOpen = true">{{ t('settings.mcp.reset') }}</FButton>
        </div>

        <FConfirmDialog
          v-model:open="regenOpen"
          :title="t('settings.mcp.regenToken')"
          :message="t('settings.mcp.regenConfirm')"
          :confirm-text="t('settings.mcp.regenToken')"
          @confirm="mcpRegen"
        />
        <FConfirmDialog
          v-model:open="mcpResetOpen"
          :title="t('settings.mcp.reset')"
          :message="t('settings.mcp.resetConfirm')"
          :confirm-text="t('settings.mcp.reset')"
          @confirm="mcpReset"
        />
      </template>

      <template v-else-if="activeNav === 'update'">
        <div class="sv-title">{{ t('settings.update') }}</div>
        <div class="sv-block">
          <div class="sv-hint">{{ t('settings.updateHint') }}</div>
          <FButton @click="toast(t('settings.versionToast'))">{{ t('settings.checkUpdate') }}</FButton>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  display: flex;
  height: 100%;
  min-height: 0;
}
.sv-nav {
  width: 180px;
  flex: none;
  padding: 12px 8px;
  border-right: 1px solid var(--stroke-divider);
  overflow: auto;
}
.sv-nav-title {
  font-size: 15px;
  font-weight: 600;
  padding: 4px 10px 12px;
}
.sv-nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--text-primary);
  font-size: 13px;
}
.sv-nav-item:hover {
  background: var(--bg-alt-hover);
}
.sv-nav-item.on {
  background: var(--bg-selected);
  font-weight: 600;
}
.sv-nav-icon {
  width: 16px;
  text-align: center;
  color: var(--accent);
}
.sv-content {
  flex: 1;
  overflow: auto;
  padding: 20px 28px;
}
.sv-title {
  font-size: 20px;
  font-weight: 600;
  margin-bottom: 18px;
}
.sv-block {
  margin-bottom: 24px;
}
.sv-row-label {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 10px;
}
.sv-grid {
  display: grid;
  grid-template-columns: 90px 260px;
  gap: 12px;
  align-items: center;
}
.sv-label {
  color: var(--text-secondary);
  text-align: right;
  font-size: 12.5px;
}
.sv-hint {
  color: var(--text-tertiary);
  font-size: 12.5px;
  line-height: 1.6;
  margin-bottom: 12px;
}
.lang-combo {
  width: 260px;
}
.theme-cards {
  display: flex;
  gap: 12px;
}
.theme-card {
  cursor: pointer;
  text-align: center;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.theme-card.on span {
  color: var(--accent);
  font-weight: 600;
}
.preview {
  width: 120px;
  height: 76px;
  border-radius: var(--radius-md);
  border: 2px solid transparent;
  overflow: hidden;
  margin-bottom: 6px;
}
.theme-card.on .preview {
  border-color: var(--accent);
}
.preview.light {
  background: #f3f3f3;
}
.preview.light .pv-bar {
  height: 18px;
  background: #ffffff;
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
}
.preview.light .pv-body {
  margin: 8px;
  height: calc(100% - 34px);
  background: #ffffff;
  border-radius: 3px;
}
.preview.dark {
  background: #202020;
}
.preview.dark .pv-bar {
  height: 18px;
  background: #2d2d2d;
}
.preview.dark .pv-body {
  margin: 8px;
  height: calc(100% - 34px);
  background: #333;
  border-radius: 3px;
}
.accent-row {
  display: flex;
  gap: 10px;
}
.accent-dot {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
  outline: 2px solid transparent;
  outline-offset: 2px;
}
.accent-dot.on {
  outline-color: var(--accent);
}

/* ==================== MCP Server ==================== */
.mcp-service {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
}
.mcp-service-text {
  flex: 1;
  min-width: 0;
}
.mcp-service-ctl {
  display: flex;
  align-items: center;
  gap: 12px;
  flex: none;
  padding-top: 2px;
}
.mcp-badge {
  font-size: 12px;
  padding: 2px 10px;
  border-radius: 999px;
  border: 1px solid var(--stroke-divider);
  color: var(--text-tertiary);
  white-space: nowrap;
}
.mcp-badge.on {
  color: var(--accent-text-on);
  background: var(--accent);
  border-color: var(--accent);
  font-weight: 600;
}
.mcp-error {
  color: var(--danger);
  font-size: 12.5px;
  margin: -12px 0 20px;
}
.mcp-port {
  width: 140px;
}
.mcp-hint {
  margin: 10px 0 12px;
}
.mcp-token {
  font-family: 'Cascadia Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  background: var(--bg-alt-hover);
  border: 1px solid var(--stroke-divider);
  border-radius: var(--radius-sm);
  padding: 10px 12px;
  word-break: break-all;
  user-select: all;
  margin-bottom: 12px;
}
.mcp-btn-row {
  display: flex;
  gap: 10px;
}
.mcp-tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.mcp-tab {
  border: 1px solid var(--stroke-divider);
  background: var(--bg-card);
  color: var(--text-secondary);
  font-size: 12.5px;
  padding: 5px 16px;
  border-radius: 999px;
  cursor: pointer;
}
.mcp-tab:hover {
  background: var(--bg-alt-hover);
}
.mcp-tab.on {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text-on);
  font-weight: 600;
}
.mcp-code {
  font-family: 'Cascadia Mono', 'Cascadia Code', Consolas, monospace;
  font-size: 12px;
  line-height: 1.6;
  background: var(--bg-alt-hover);
  border: 1px solid var(--stroke-divider);
  border-radius: var(--radius-sm);
  padding: 12px 14px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 280px;
  overflow: auto;
  user-select: all;
  margin: 0 0 12px;
}
.mcp-security {
  border: 1px solid var(--stroke-divider);
  border-left: 3px solid var(--accent);
  border-radius: var(--radius-sm);
  padding: 10px 14px;
  background: var(--bg-alt-hover);
}
.mcp-reset-row {
  display: flex;
  justify-content: flex-end;
  padding-top: 4px;
  border-top: 1px dashed var(--stroke-divider);
}
</style>
