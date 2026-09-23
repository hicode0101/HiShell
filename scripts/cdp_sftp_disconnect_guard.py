#!/usr/bin/env python3
"""E2E：断开后远程栏操作全部拦截并提示，重连后自动恢复"""
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
MSG = "连接已断开"


def toast_text():
    return ev("[...document.querySelectorAll('.toast')].map(t => t.textContent.trim()).join('|')")


def try_op(name, js):
    r = ev(js)
    t = toast_text()
    ok = MSG in str(t)
    print(f"{name}: toast={'Y' if ok else 'N(' + str(t) + ')'} -> {r}")
    return ok


# 1) 连接 + 进入文件视图
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  const profiles = %P%._s.get('profiles')
  await profiles.init()
  const p = profiles.profiles.find(x => x.host === '149.104.22.80')
  const tab = s.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 60; i++) {
    await new Promise(r2 => setTimeout(r2, 500))
    if (tab.status === 'connected') break
  }
  if (tab.status !== 'connected') return 'not connected'
  tab.viewMode = 'sftp'
  await new Promise(r2 => setTimeout(r2, 800))
  return JSON.stringify({ sid: tab.sid })
})()""".replace("%P%", P))
print("connect:", r)
if not str(r).startswith("{"):
    raise SystemExit("no session")

# 2) 断开（保持文件视图）
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  const tab = s.activeTab
  s.disconnectById(tab.id)
  await new Promise(r2 => setTimeout(r2, 600))
  return JSON.stringify({ status: tab.status, viewMode: tab.viewMode })
})()""".replace("%P%", P))
print("disconnect:", r)

# 3a) 刷新
report["refreshBlocked"] = try_op("refresh", """(async () => {
  const pane = document.querySelector('.file-pane.is-remote')
  const btns = pane.querySelectorAll('.pb-path .pb-btn')
  btns[btns.length - 1].click()
  await new Promise(r2 => setTimeout(r2, 300))
  return pane.querySelector('.pb-input').value
})()""")

# 3b) 双击目录
report["dblclickBlocked"] = try_op("dblclick dir", """(async () => {
  const pane = document.querySelector('.file-pane.is-remote')
  const row = [...pane.querySelectorAll('.ft-row')].find(x => {
    const n = x.querySelector('.col-name').textContent.trim()
    return n !== '..' && x.querySelector('.dir-ic')
  })
  if (!row) return 'no dir row'
  row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 400))
  return pane.querySelector('.pb-input').value
})()""")

# 3c) 右键删除（不应出现确认对话框）
report["deleteBlocked"] = try_op("context delete", """(async () => {
  const pane = document.querySelector('.file-pane.is-remote')
  const row = [...pane.querySelectorAll('.ft-row')].find(x => x.querySelector('.col-name').textContent.trim() !== '..')
  row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 700, clientY: 400 }))
  await new Promise(r2 => setTimeout(r2, 300))
  const del = [...document.querySelectorAll('.f-ctx-item')].find(i => i.textContent.includes('删除'))
  if (del) del.click()
  await new Promise(r2 => setTimeout(r2, 300))
  return 'clicked'
})()""")
r = ev("""(() => {
  const dlgOpen = [...document.querySelectorAll('.f-dialog-overlay')]
    .some(o => o.getBoundingClientRect().width > 0 && o.innerText.includes('删除'))
  return !dlgOpen
})()""")
report["deleteNoDialog"] = bool(r)

# 3d) 拖拽远程行到本地（不应入队下载）
before = ev(f"{P}._s.get('transfers').items.length")
report["dragBlocked"] = try_op("drag remote->local", """(async () => {
  const row = [...document.querySelectorAll('.file-pane.is-remote .ft-row')]
    .find(x => x.querySelector('.col-name').textContent.trim() !== '..')
  const dst = document.querySelector('.file-pane:not(.is-remote) .file-table-wrap').getBoundingClientRect()
  const a = row.getBoundingClientRect()
  const sx = a.left + a.width / 2, sy = a.top + a.height / 2
  const dx = dst.left + dst.width / 2, dy = dst.top + dst.height / 2
  row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: sx, clientY: sy }))
  for (let i = 1; i <= 6; i++) {
    document.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: sx + (dx - sx) * i / 6, clientY: sy + (dy - sy) * i / 6 }))
    await new Promise(r2 => setTimeout(r2, 30))
  }
  document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientX: dx, clientY: dy }))
  await new Promise(r2 => setTimeout(r2, 400))
  return 'dragged'
})()""")
after = ev(f"{P}._s.get('transfers').items.length")
report["dragNoQueue"] = after == before
print("transfer items before/after:", before, after)

# 4) 重连 → 守卫解除，目录可正常进入
r = ev("""(async () => {
  const s = %P%._s.get('sessions')
  const tab = s.activeTab
  const profiles = %P%._s.get('profiles')
  const p = profiles.byId(tab.profileId)
  s.reconnect(tab.id)
  for (let i = 0; i < 60; i++) {
    await new Promise(r2 => setTimeout(r2, 500))
    if (tab.status === 'connected') break
  }
  if (tab.status !== 'connected') return JSON.stringify({ reconnected: false })
  await new Promise(r2 => setTimeout(r2, 800))
  const pane = document.querySelector('.file-pane.is-remote')
  const row = [...pane.querySelectorAll('.ft-row')].find(x => {
    const n = x.querySelector('.col-name').textContent.trim()
    return n !== '..' && x.querySelector('.dir-ic')
  })
  if (!row) return JSON.stringify({ reconnected: true, dirRows: 0 })
  row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  await new Promise(r2 => setTimeout(r2, 600))
  return JSON.stringify({ reconnected: true, path: pane.querySelector('.pb-input').value })
})()""".replace("%P%", P))
print("reconnect:", r)
d4 = json.loads(r)
report["recovered"] = d4.get("reconnected") and str(d4.get("path", "")).startswith("/root/")

# 清理
ev("""(async () => {
  const s = %P%._s.get('sessions')
  s.setViewMode(s.activeTab.id, 'terminal')
  for (const t of [...s.tabs]) if (t.kind === 'session') s.closeTab(t.id)
  return s.tabs.length
})()""".replace("%P%", P))

print("\n==== RESULT ====")
print(json.dumps(report, ensure_ascii=False))
print("PASS" if all(report.values()) else f"FAIL: {[k for k, v in report.items() if not v]}")
