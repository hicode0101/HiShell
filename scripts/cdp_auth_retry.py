#!/usr/bin/env python3
"""E2E：密码认证严格按配置 + 错误密码弹临时认证对话框
 1. 复制一个密码认证配置，写入错误密码
 2. 连接 → 应弹出密码对话框（带「密码验证失败」提示），终端错误不得出现 agent
 3. 再输错一次 → 对话框再次弹出（循环）
 4. 取消 → 连接中止，statusMessage 只提「密码」不提 agent
 5. 清理临时配置
"""
import json
import time
import urllib.request

import websocket

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=180)
mid = 0


def ev(expr):
    global mid
    mid += 1
    ws.send(json.dumps({"id": mid, "method": "Runtime.evaluate", "params": {
        "expression": expr, "returnByValue": True, "awaitPromise": True}}))
    while True:
        m = json.loads(ws.recv())
        if m.get("id") == mid:
            r = m.get("result", {})
            if "exceptionDetails" in r:
                return "ERR: " + str(r["exceptionDetails"].get("exception", {}).get("description", "?"))
            return r.get("result", {}).get("value")


P = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"
report = {}

# 1) 用 profile_copy 复制配置（真实 id，含原凭据），再覆盖为错误密码
r = ev("""(async () => {
  const profiles = %P%._s.get('profiles')
  await profiles.init()
  const src = profiles.profiles.find(p => p.host === '149.104.22.80' && p.auth === 'password')
  if (!src) return 'no password profile'
  const saved = await window.__TAURI_INTERNALS__.invoke('profile_copy', {
    sourceId: src.id, newName: 'auth-e2e-temp' })
  await profiles.upsert(JSON.parse(JSON.stringify(saved)), { password: 'definitely-wrong-password' })
  return JSON.stringify({ id: saved.id, auth: saved.auth })
})()""".replace("%P%", P))
print("setup profile:", r)
d = json.loads(r)
pid = d.get("id")

# 2) 连接 → 密码对话框应弹出并带重试提示
r = ev("""(async () => {
  const profiles = %P%._s.get('profiles')
  const s = %P%._s.get('sessions')
  const p = profiles.byId('%PID%')
  const tab = s.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 20; i++) {
    await new Promise(r2 => setTimeout(r2, 500))
    const cur = %P%._s.get('sessions').prompts[0]
    if (cur && cur.payload.kind === 'password') break
  }
  const cur = %P%._s.get('sessions').prompts[0]
  return JSON.stringify({ sid: tab.sid, promptKind: cur ? cur.payload.kind : null,
    retry: !!(cur && cur.payload.retry), tabId: tab.id })
})()""".replace("%P%", P).replace("%PID%", pid))
print("prompt:", r)
d = json.loads(r)
report["promptAppears"] = d.get("promptKind") == "password"
report["retryFlag"] = d.get("retry") is True
tab_id = d.get("tabId")

# 3) 再输错一次 → 对话框应再次弹出
r = ev("""(async () => {
  const input = document.querySelector('.f-dialog-overlay input[type=password]')
  if (!input) return 'no input'
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
  setter.call(input, 'wrong-again')
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 200))
  const ok = [...document.querySelectorAll('.f-dialog-overlay button')]
    .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '确认').pop()
  if (!ok) return 'no ok btn'
  ok.click()
  for (let i = 0; i < 16; i++) {
    await new Promise(r2 => setTimeout(r2, 500))
    const cur = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('sessions').prompts[0]
    if (cur && cur.payload.kind === 'password' && cur.payload.retry) break
  }
  const cur = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('sessions').prompts[0]
  return JSON.stringify({ rePrompted: !!(cur && cur.payload.kind === 'password') })
})()""")
print("second wrong:", r)
report["rePromptsAgain"] = json.loads(r).get("rePromotedAgain", json.loads(r).get("rePrompted"))
# 对话框中应有重试提示文本
r = ev("""(() => {
  const ov = [...document.querySelectorAll('.f-dialog-overlay')].find(o => o.getBoundingClientRect().width > 0)
  return JSON.stringify({ hasRetryHint: !!ov && ov.innerText.includes('密码验证失败') })
})()""")
report["retryHintShown"] = json.loads(r).get("hasRetryHint")

# 4) 取消 → 连接中止；错误信息只提「密码」
r = ev("""(async () => {
  const cancel = [...document.querySelectorAll('.f-dialog-overlay button')]
    .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '取消').pop()
  if (cancel) cancel.click()
  await new Promise(r2 => setTimeout(r2, 1500))
  const s = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('sessions')
  const tab = s.tabs.find(t => t.id === '%TAB%')
  return JSON.stringify({ status: tab.status, msg: tab.statusMessage || '' })
})()""".replace("%TAB%", tab_id))
print("cancel:", r)
d = json.loads(r)
report["cancelled"] = d.get("status") == "disconnected"
msg = d.get("msg", "")
report["msgMentionsPasswordOnly"] = ("密码" in msg or "取消" in msg) and "agent" not in msg
print("statusMessage:", msg)

# 5) 清理：关标签 + 删除临时配置
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  const profiles = %P%._s.get('profiles')
  for (const t of [...s.tabs]) if (t.kind === 'session') s.closeTab(t.id)
  try { await profiles.remove('%PID%') } catch (e) {}
  return 'cleaned'
})()""".replace("%P%", P).replace("%PID%", pid))
print("cleanup:", r)

print("\n==== RESULT ====")
print(json.dumps(report, ensure_ascii=False))
print("PASS" if all(report.values()) else f"FAIL: {[k for k, v in report.items() if not v]}")
