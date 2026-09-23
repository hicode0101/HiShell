#!/usr/bin/env python3
"""E2E：统一认证对话框（authRetry）
 1. 错误密码连接 → 弹统一认证框：标题只出现一次、错误红字、认证方式默认=配置
 2. 再输错密码 → 确认 → 对话框再次弹出（循环）
 3. 切换认证方式为「用户密钥」→ 确认 → 公钥认证被拒后再次弹出（方式切换生效）
 4. 取消 → 连接中止（用户取消）
 5. 清理
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


def wait_auth_prompt(timeout_s=15):
    return ev("""(async () => {
      for (let i = 0; i < %T%; i++) {
        await new Promise(r2 => setTimeout(r2, 500))
        const cur = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('sessions').prompts[0]
        if (cur && cur.payload.kind === 'authRetry') {
          const ov = [...document.querySelectorAll('.f-dialog-overlay')].find(o => o.getBoundingClientRect().width > 0)
          return JSON.stringify({ error: cur.payload.error, method: cur.payload.method,
            keys: cur.payload.userKeys, visible: !!ov,
            titleCount: ov ? (ov.innerText.match(/root@149\\.104\\.22\\.80/g) || []).length : 0 })
        }
      }
      return JSON.stringify({ timeout: true })
    })()""".replace("%T%", str(timeout_s * 2)))


def set_password_and_confirm(pwd):
    return ev("""(async () => {
      const input = document.querySelector('.f-dialog-overlay input[type=password]')
      if (!input) return 'no input'
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
      setter.call(input, '%PWD%')
      input.dispatchEvent(new Event('input', { bubbles: true }))
      await new Promise(r2 => setTimeout(r2, 150))
      const ok = [...document.querySelectorAll('.f-dialog-overlay button')]
        .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '确定').pop()
      if (!ok) return 'no ok'
      ok.click()
      return 'submitted'
    })()""".replace("%PWD%", pwd))


# 1) 复制配置并写入错误密码
r = ev("""(async () => {
  const profiles = %P%._s.get('profiles')
  await profiles.init()
  const src = profiles.profiles.find(p => p.host === '149.104.22.80' && p.auth === 'password')
  if (!src) return 'no src'
  const saved = await window.__TAURI_INTERNALS__.invoke('profile_copy', { sourceId: src.id, newName: 'auth-e2e-temp' })
  await profiles.upsert(JSON.parse(JSON.stringify(saved)), { password: 'definitely-wrong-password' })
  return JSON.stringify({ id: saved.id })
})()""".replace("%P%", P))
print("setup:", r)
pid = json.loads(r)["id"]

# 2) 连接 → 统一认证框
r = ev("""(async () => {
  const profiles = %P%._s.get('profiles')
  const s = %P%._s.get('sessions')
  const p = profiles.byId('%PID%')
  const tab = s.createSession(JSON.parse(JSON.stringify(p)))
  await new Promise(r2 => setTimeout(r2, 100))
  return JSON.stringify({ tabId: tab.id })
})()""".replace("%P%", P).replace("%PID%", pid))
tab_id = json.loads(r)["tabId"]
r = wait_auth_prompt()
print("prompt1:", r)
d = json.loads(r)
report["dialogAppears"] = not d.get("timeout")
report["errorText"] = "密码认证被服务器拒绝" in str(d.get("error"))
report["defaultMethod"] = d.get("method") == "password"
report["singleTitle"] = d.get("titleCount") == 1

# 3) 再输错 → 循环
print("submit2:", set_password_and_confirm("wrong-again"))
time.sleep(1)
r = wait_auth_prompt()
print("prompt2:", r)
report["loopsAgain"] = "密码认证被服务器拒绝" in str(json.loads(r).get("error"))

# 4) 切换认证方式为「用户密钥」→ 确认 → 公钥被拒
r = ev("""(async () => {
  const combo = document.querySelector('.f-dialog-overlay .f-combo-face')
  if (!combo) return 'no combo'
  combo.click()
  await new Promise(r2 => setTimeout(r2, 300))
  const item = [...document.querySelectorAll('.f-combo-item')].find(i => i.textContent.includes('用户密钥'))
  if (!item) return 'no userKey option'
  item.click()
  await new Promise(r2 => setTimeout(r2, 300))
  const ok = [...document.querySelectorAll('.f-dialog-overlay button')]
    .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '确定').pop()
  if (!ok) return 'no ok'
  ok.click()
  return 'switched'
})()""")
print("switch:", r)
r = wait_auth_prompt()
print("prompt3:", r)
report["methodSwitchWorks"] = str(json.loads(r).get("error", "")).startswith("公钥")

# 5) 取消
r = ev("""(async () => {
  const cancel = [...document.querySelectorAll('.f-dialog-overlay button')]
    .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '取消').pop()
  cancel.click()
  await new Promise(r2 => setTimeout(r2, 1500))
  const s = %P%._s.get('sessions')
  const tab = s.tabs.find(t => t.id === '%TAB%')
  return JSON.stringify({ status: tab.status, msg: tab.statusMessage || '' })
})()""".replace("%P%", P).replace("%TAB%", tab_id))
print("cancel:", r)
d = json.loads(r)
report["cancelAborts"] = d.get("status") == "disconnected"

# 6) 清理
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  const profiles = %P%._s.get('profiles')
  for (const t of [...s.tabs]) if (t.kind === 'session') s.closeTab(t.id)
  try { await profiles.remove('%PID%') } catch (e) {}
  await profiles.init()
  return JSON.stringify({ temp: profiles.profiles.filter(p => p.name.includes('auth-e2e')).length })
})()""".replace("%P%", P).replace("%PID%", pid))
print("cleanup:", r)
report["cleaned"] = json.loads(r).get("temp") == 0

print("\n==== RESULT ====")
print(json.dumps(report, ensure_ascii=False))
print("PASS" if all(report.values()) else f"FAIL: {[k for k, v in report.items() if not v]}")
