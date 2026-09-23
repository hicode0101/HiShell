#!/usr/bin/env python3
"""E2E：工具栏用户密钥按钮 + 会话管理器连接后自动关闭"""
import json
import time
import urllib.request

import websocket

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=120)
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
                return {"__error__": r["exceptionDetails"].get("exception", {}).get("description", "?")}
            return r.get("result", {}).get("value")


PINIA = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"
report = {}
tabs_before = ev(f"{PINIA}._s.get('sessions').tabs.length")
print("tabs before:", tabs_before)

# ---- 1. 工具栏钥匙按钮 ----
r = ev("""(() => {
  const btns = [...document.querySelectorAll('.tb-btn')]
  const keyBtn = btns.find(b => (b.title || '').includes('用户密钥'))
  if (!keyBtn) return JSON.stringify({ found: false })
  const mgrIdx = btns.findIndex(b => (b.title || '').includes('会话管理'))
  const keyIdx = btns.indexOf(keyBtn)
  return JSON.stringify({ found: true, afterManager: keyIdx === mgrIdx + 1, tip: keyBtn.title })
})()""")
print("key button:", r)
d = json.loads(r)
report["keyBtnAfterManager"] = d.get("found") and d.get("afterManager")
report["keyBtnTip"] = d.get("tip") == "用户密钥管理"

# 点击 → 用户密钥管理对话框打开（fixed 覆盖层内 offsetParent 恒为 null，用覆盖层文本判断）
r = ev("""(async () => {
  const btn = [...document.querySelectorAll('.tb-btn')].find(b => (b.title || '').includes('用户密钥'))
  btn.click()
  await new Promise(r2 => setTimeout(r2, 500))
  const vis = [...document.querySelectorAll('.f-dialog-overlay')].filter(o => o.getBoundingClientRect().width > 0)
  const opened = vis.some(o => o.innerText.includes('用户密钥'))
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 400))
  return JSON.stringify({ dialogVisible: opened })
})()""")
print("click key btn:", r)
report["keyDialogOpens"] = json.loads(r).get("dialogVisible")
# 关闭
ev("""(async () => {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 300))
  const btn = [...document.querySelectorAll('button')].filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '关闭').pop()
  if (btn) btn.click()
  await new Promise(r2 => setTimeout(r2, 300))
  return 'closed'
})()""")

# ---- 2. 会话管理器 → 编辑 → 连接 → 管理器应自动关闭 ----
r = ev("""(async () => {
  const s = %PINIA%._s.get('sessions')
  const mgrBtn = [...document.querySelectorAll('.tb-btn')].find(b => (b.title || '').includes('会话管理'))
  mgrBtn.click()
  await new Promise(r2 => setTimeout(r2, 500))
  const mgrVisible = [...document.querySelectorAll('button')].some(b => b.getBoundingClientRect().width > 0)
  // 找编辑（铅笔）按钮：title 为 编辑 的图标按钮
  const editBtn = [...document.querySelectorAll('button')].find(b => b.title === '编辑' && b.getBoundingClientRect().width > 0)
  if (!editBtn) return JSON.stringify({ mgrVisible, editBtn: false })
  editBtn.click()
  await new Promise(r2 => setTimeout(r2, 500))
  const dlgVisible = [...document.querySelectorAll('h2, [class*=title]')].some(
    e => e.offsetParent && (e.textContent.includes('编辑连接') || e.textContent.includes('新建连接')))
  return JSON.stringify({ mgrVisible, editBtn: true, dlgVisible })
})()""".replace("%PINIA%", PINIA))
print("manager -> edit:", r)
d = json.loads(r)
report["managerOpens"] = d.get("mgrVisible")
report["editDialogOpens"] = d.get("editBtn") and d.get("dlgVisible")

# 点「连接」
r = ev("""(async () => {
  const connectBtn = [...document.querySelectorAll('button')]
    .find(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '连接')
  if (!connectBtn) return JSON.stringify({ connectBtn: false })
  connectBtn.click()
  for (let i = 0; i < 30; i++) {
    await new Promise(r2 => setTimeout(r2, 500))
    const s = %PINIA%._s.get('sessions')
    if (!s.connectDialogOpen) break
  }
  await new Promise(r2 => setTimeout(r2, 600))
  const s = %PINIA%._s.get('sessions')
  const dlgGone = !s.connectDialogOpen
  // 管理器是否关闭：无可见的会话管理列表行
  const mgrGone = ![...document.querySelectorAll('.sm-row')].some(x => x.getBoundingClientRect().width > 0)
  const tabs = s.tabs.length
  return JSON.stringify({ connectBtn: true, dlgGone, mgrGone, tabs })
})()""".replace("%PINIA%", PINIA))
print("connect:", r)
d = json.loads(r)
report["dialogCloses"] = d.get("dlgGone")
report["managerAutoClosed"] = d.get("mgrGone")
report["tabCreated"] = d.get("tabs", 0) > tabs_before

# ---- 清理：关闭测试创建的连接标签 ----
r = ev(f"""(async () => {{
  const s = {PINIA}._s.get('sessions')
  for (const t of [...s.tabs]) if (t.kind === 'session') s.closeTab(t.id)
  return s.tabs.length
}})()""")
print("tabs after cleanup:", r)

print("\n==== RESULT ====")
fails = [k for k, v in report.items() if not v]
print(json.dumps(report, indent=1, ensure_ascii=False))
print("PASS" if not fails else f"FAIL: {fails}")
