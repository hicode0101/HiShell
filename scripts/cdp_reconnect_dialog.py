#!/usr/bin/env python3
"""E2E：快速连接重连 + 认证对话框宽度
 1. 地址栏快速连接 root@192.168.209.130（故意错误密码）→ 统一认证框弹出，宽度应为 620
 2. 取消 → 标签断开
 3. 点「重新连接」→ 认证框应再次弹出（回归：此前被 profiles.byId 门禁静默拦截），宽度 620
 4. 再取消 → 右下角应出现「重新连接失败」toast（失败可见）
 5. 清理：删除本次新增的临时配置、关闭测试标签
"""
import json
import os
import time
import urllib.request

import websocket

PJ = os.path.join(os.environ["APPDATA"], "hishell", "profiles.json")


def temp_ids():
    return {x["id"] for x in json.load(open(PJ, encoding="utf-8")) if isinstance(x, dict) and x.get("temporary")}


BEFORE = temp_ids()

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


def shot(name):
    global mid
    mid += 1
    ws.send(json.dumps({"id": mid, "method": "Page.captureScreenshot", "params": {"format": "png"}}))
    while True:
        m = json.loads(ws.recv())
        if m.get("id") == mid:
            open(os.path.join(os.path.dirname(__file__), name), "wb").write(__import__("base64").b64decode(m["result"]["data"]))
            return name


P = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"
report = {}
HOST = "192.168.209.130"


def wait_auth_prompt(timeout_s=20):
    return ev("""(async () => {
      for (let i = 0; i < %T%; i++) {
        await new Promise(r2 => setTimeout(r2, 500))
        const s = %P%._s.get('sessions')
        const cur = s.prompts[0]
        if (cur && cur.payload.kind === 'authRetry') {
          const dlg = [...document.querySelectorAll('.f-dialog')].find(d => d.getBoundingClientRect().width > 0)
          const rect = dlg ? dlg.getBoundingClientRect() : { width: 0, height: 0 }
          return JSON.stringify({ error: String(cur.payload.error ?? ''), width: Math.round(rect.width), height: Math.round(rect.height) })
        }
      }
      return JSON.stringify({ timeout: true })
    })()""".replace("%T%", str(timeout_s * 2)).replace("%P%", P))


def click_dialog_button(text):
    return ev("""(async () => {
      const btn = [...document.querySelectorAll('.f-dialog-overlay button')]
        .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '%TXT%').pop()
      if (!btn) return 'no button %TXT%'
      btn.click()
      return 'clicked %TXT%'
    })()""".replace("%TXT%", text))


def toast_visible(text, timeout_s=10):
    return ev("""(async () => {
      for (let i = 0; i < %T%; i++) {
        const toasts = [...document.querySelectorAll('.toast')].map(x => x.textContent)
        if (toasts.some(x => x.includes('%TXT%'))) return JSON.stringify({ found: true, toasts })
        await new Promise(r2 => setTimeout(r2, 400))
      }
      return JSON.stringify({ found: false })
    })()""".replace("%T%", str(timeout_s * 2)).replace("%TXT%", text))


# 1) 地址栏快速连接（错误密码在认证框弹出后再无输入→直接取消，所以这里先填错误密码）
r = ev("""(async () => {
  const input = document.querySelector('.addr-input')
  if (!input) return 'no addr input'
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
  setter.call(input, 'ssh://root@%HOST%')
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 150))
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 400))
  const dlg = [...document.querySelectorAll('.f-dialog')].find(d => d.getBoundingClientRect().width > 0)
  return JSON.stringify({ quickDialog: !!dlg, host: dlg ? dlg.querySelector('input')?.value : '' })
})()""".replace("%HOST%", HOST))
print("quick-open:", r)
report["quickDialogOpened"] = json.loads(r).get("quickDialog") is True

# 2) 填错误密码并点「连接」
r = ev("""(async () => {
  const input = document.querySelector('.f-dialog-overlay input[type=password]')
  if (!input) return 'no pwd input'
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set
  setter.call(input, 'e2e-wrong-password')
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 150))
  const btn = [...document.querySelectorAll('.f-dialog-overlay button')]
    .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '连接').pop()
  if (!btn) return 'no connect btn'
  btn.click()
  return 'connecting'
})()""")
print("connect:", r)

# 3) 统一认证框弹出 + 宽度断言
r = wait_auth_prompt()
print("prompt1:", r)
d = json.loads(r)
report["authDialogOnFirstConnect"] = not d.get("timeout")
report["dialogWidth620"] = d.get("width") == 620
report["authFailedAsExpected"] = "拒绝" in d.get("error", "") or "失败" in d.get("error", "") or "denied" in d.get("error", "").lower()
shot("cdp_auth_width_620.png")

# 4) 取消 → 标签应断开
print("cancel1:", click_dialog_button("取消"))
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  for (let i = 0; i < 20; i++) {
    await new Promise(r2 => setTimeout(r2, 500))
    const tab = s.tabs.find(t => t.address.includes('%HOST%'))
    if (tab && tab.status === 'disconnected') return JSON.stringify({ disconnected: true, statusMessage: tab.statusMessage })
  }
  return JSON.stringify({ disconnected: false })
})()""".replace("%P%", P).replace("%HOST%", HOST))
print("after-cancel:", r)
report["tabDisconnectedAfterCancel"] = json.loads(r).get("disconnected") is True

# 5) 点「重新连接」→ 认证框应再次弹出（回归核心）+ 重连 toast
r = ev("""(async () => {
  const btn = [...document.querySelectorAll('button[title="重新连接"]')]
    .filter(b => b.getBoundingClientRect().width > 0 && !b.disabled).pop()
  if (!btn) return 'no reconnect btn'
  btn.click()
  return 'reconnect clicked'
})()""")
print("reconnect:", r)
early_toast = toast_visible("正在重新连接", 4)
print("toast-reconnecting:", early_toast)
report["reconnectingToast"] = json.loads(early_toast).get("found") is True

r = wait_auth_prompt()
print("prompt2:", r)
d2 = json.loads(r)
report["authDialogOnReconnect"] = not d2.get("timeout")
report["dialogWidth620Again"] = d2.get("width") == 620
shot("cdp_reconnect_dialog.png")

# 6) 再取消 → 「重新连接失败」toast 可见
print("cancel2:", click_dialog_button("取消"))
r = toast_visible("重新连接失败", 12)
print("toast-failed:", r)
report["reconnectFailedToast"] = json.loads(r).get("found") is True

# 7) 清理：关闭测试标签 + 删除新增临时配置
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  const tab = s.tabs.find(t => t.address.includes('%HOST%'))
  if (tab) s.closeTab(tab.id)
  return 'tab closed'
})()""".replace("%P%", P).replace("%HOST%", HOST))
print("cleanup-tab:", r)

after = temp_ids()
new_ids = [i for i in after if i not in BEFORE]
for i in new_ids:
    print("delete temp:", i, ev(f"window.__TAURI_INTERNALS__.invoke('profile_delete', {{ id: '{i}' }})"))
report["tempProfilesCleaned"] = len(new_ids)

print("\nRESULT:", json.dumps(report, ensure_ascii=False))
ok = (report["quickDialogOpened"] and report["authDialogOnFirstConnect"] and report["dialogWidth620"]
      and report["tabDisconnectedAfterCancel"] and report["authDialogOnReconnect"]
      and report["dialogWidth620Again"] and report["reconnectFailedToast"])
print("ALL PASS" if ok else "SOME FAILED")
