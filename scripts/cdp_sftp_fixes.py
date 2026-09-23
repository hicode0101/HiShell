#!/usr/bin/env python3
"""E2E：本轮 4 项修复回归
 1. 传输队列行布局（名称/状态同行，grid 生效）
 2. 上传完成后远程栏自动刷新
 3. 面板间鼠标拖拽（远程 → 本地下载）
 4. Delete 键删除选中文件（确认对话框）
"""
import json
import os
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
        msg = json.loads(ws.recv())
        if msg.get("id") == mid:
            res = msg.get("result", {})
            if "exceptionDetails" in res:
                ex = res["exceptionDetails"].get("exception", {})
                return {"__error__": ex.get("description") or ex.get("value") or "?"}
            return res.get("result", {}).get("value")


PINIA = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"
TEMP = os.environ.get("TEMP", r"C:\Users\QWERT\AppData\Local\Temp")
LOCAL_FILE = os.path.join(TEMP, "hishell_fix_e2e.txt")
LOCAL_DIR = os.path.join(TEMP, "hishell_fix_dir")
NAME = "hishell_fix_e2e.txt"
with open(LOCAL_FILE, "wb") as f:
    f.write(b"hishell fix regression\n" * 3000)
os.makedirs(LOCAL_DIR, exist_ok=True)
SIZE = os.path.getsize(LOCAL_FILE)

report = {}

# ---- 连接 + 文件视图 ----
r = ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  let tab = sessions.tabs.find(t => t.status === 'connected')
  if (!tab) {
    const profiles = %PINIA%._s.get('profiles')
    await profiles.init()
    for (const host of ['149.104.22.80', '192.168.209.130', '39.98.212.72']) {
      const p = profiles.profiles.find(x => x.host === host)
      if (!p) continue
      tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
      for (let i = 0; i < 40; i++) {
        await new Promise(r2 => setTimeout(r2, 500))
        if (tab.status !== 'connecting') break
      }
      if (tab.status === 'connected' && tab.sid !== undefined) break
      sessions.closeTab(tab.id)
      tab = null
    }
  }
  if (!tab || tab.status !== 'connected') return 'not connected'
  tab.viewMode = 'sftp'
  await new Promise(r2 => setTimeout(r2, 800))
  return JSON.stringify({ sid: tab.sid })
})()""".replace("%PINIA%", PINIA))
print("connect:", r)
if not (isinstance(r, str) and r.startswith("{")):
    raise SystemExit("no session")

# ---- 1. 拖放上传到远程栏 ----
r = ev("""(async () => {
  const pane = document.querySelector('.file-pane.is-remote')
  const r0 = pane.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  const px = Math.round((r0.left + r0.width / 2) * dpr)
  const py = Math.round((r0.top + r0.height / 2) * dpr)
  await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'tauri://drag-drop',
    payload: { paths: ['%FILE%'], position: { x: px, y: py } } })
  return 'dropped'
})()""".replace("%FILE%", LOCAL_FILE.replace("\\", "\\\\")))
print("drop:", r)

# 等上传完成
done = False
for _ in range(30):
    r = ev("""(() => {
      const tr = %PINIA%._s.get('transfers')
      const it = tr.items.find(i => i.name === '%NAME%' && i.direction === 'upload')
      return it ? it.status : 'missing'
    })()""".replace("%PINIA%", PINIA).replace("%NAME%", NAME))
    if r in ("done", "failed"):
        done = r == "done"
        break
    time.sleep(0.5)
report["uploadDone"] = done
time.sleep(1.5)

# ---- 2a. 远程栏自动刷新（未手动刷新） ----
r = ev("JSON.stringify([...document.querySelectorAll('.file-pane.is-remote .ft-row .col-name')].map(e => e.textContent.trim()))")
names = json.loads(r) if isinstance(r, str) else []
report["remoteAutoRefresh"] = NAME in names
print("remote rows:", names[:6])

# ---- 2b. 传输行布局（名称与状态同一行） ----
r = ev("""(() => {
  const row = document.querySelector('.tq-row')
  if (!row) return JSON.stringify({ row: false })
  const a = row.querySelector('.col-name').getBoundingClientRect()
  const b = row.querySelector('.col-status').getBoundingClientRect()
  const c = row.querySelector('.col-local').getBoundingClientRect()
  return JSON.stringify({ row: true, sameLine: a.top === b.top && b.top === c.top,
    grid: getComputedStyle(row).gridTemplateColumns.split(' ').length })
})()""")
print("row layout:", r)
try:
    d = json.loads(r)
    report["queueRowLayout"] = d.get("row") and d.get("sameLine") and d.get("grid", 0) >= 11
except Exception:
    report["queueRowLayout"] = False

# ---- 3. 面板间拖拽：远程行 → 本地栏（下载） ----
# 先把本地栏导航到测试目录（拖目录到本地栏）
r = ev("""(async () => {
  const pane = document.querySelector('.file-pane:not(.is-remote)')
  const r0 = pane.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  const px = Math.round((r0.left + r0.width / 2) * dpr)
  const py = Math.round((r0.top + r0.height / 2) * dpr)
  await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'tauri://drag-drop',
    payload: { paths: ['%DIR%'], position: { x: px, y: py } } })
  await new Promise(r2 => setTimeout(r2, 1000))
  return document.querySelector('.file-pane:not(.is-remote) .pb-input').value
})()""".replace("%DIR%", LOCAL_DIR.replace("\\", "\\\\")))
print("local nav:", r)

r = ev("""(async () => {
  const row = [...document.querySelectorAll('.file-pane.is-remote .ft-row')]
    .find(x => x.querySelector('.col-name').textContent.trim() === '%NAME%')
  if (!row) return 'row not found'
  const a = row.getBoundingClientRect()
  const dst = document.querySelector('.file-pane:not(.is-remote) .file-table-wrap').getBoundingClientRect()
  const sx = a.left + a.width / 2, sy = a.top + a.height / 2
  const dx = dst.left + dst.width / 2, dy = dst.top + dst.height / 2
  row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: sx, clientY: sy }))
  for (let i = 1; i <= 8; i++) {
    document.dispatchEvent(new MouseEvent('mousemove', { bubbles: true,
      clientX: sx + ((dx - sx) * i) / 8, clientY: sy + ((dy - sy) * i) / 8 }))
    await new Promise(r2 => setTimeout(r2, 40))
  }
  document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientX: dx, clientY: dy }))
  await new Promise(r2 => setTimeout(r2, 300))
  const tr = %PINIA%._s.get('transfers')
  const it = tr.items.find(i => i.name === '%NAME%' && i.direction === 'download')
  return JSON.stringify({ queued: !!it, localPath: it ? it.localPath : null })
})()""".replace("%PINIA%", PINIA).replace("%NAME%", NAME))
print("inner drag:", r)
try:
    d = json.loads(r)
    report["innerDragQueued"] = bool(d.get("queued"))
except Exception:
    report["innerDragQueued"] = False

# 等下载完成 + 本地栏自动刷新
dl_done = False
for _ in range(30):
    r = ev("""(() => {
      const tr = %PINIA%._s.get('transfers')
      const it = tr.items.find(i => i.name === '%NAME%' && i.direction === 'download')
      return it ? it.status : 'missing'
    })()""".replace("%PINIA%", PINIA).replace("%NAME%", NAME))
    if r in ("done", "failed"):
        dl_done = r == "done"
        break
    time.sleep(0.5)
report["downloadDone"] = dl_done
time.sleep(1.5)
r = ev("JSON.stringify([...document.querySelectorAll('.file-pane:not(.is-remote) .ft-row .col-name')].map(e => e.textContent.trim()))")
names = json.loads(r) if isinstance(r, str) else []
report["localAutoRefresh"] = NAME in names
print("local rows:", names[:6])

# ---- 4. Delete 键删除远程文件（确认对话框） ----
r = ev("""(async () => {
  const pane = document.querySelector('.file-pane.is-remote')
  const row = [...pane.querySelectorAll('.ft-row')]
    .find(x => x.querySelector('.col-name').textContent.trim() === '%NAME%')
  if (!row) return 'row not found'
  row.click()
  row.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Delete' }))
  await new Promise(r2 => setTimeout(r2, 400))
  const btn = [...document.querySelectorAll('button')]
    .filter(b => b.getBoundingClientRect().width > 0 && b.textContent.trim() === '删除').pop()
  if (!btn) return JSON.stringify({ dialog: false })
  btn.click()
  await new Promise(r2 => setTimeout(r2, 1000))
  const left = [...pane.querySelectorAll('.ft-row .col-name')].map(e => e.textContent.trim())
  return JSON.stringify({ dialog: true, gone: !left.includes('%NAME%') })
})()""".replace("%NAME%", NAME))
print("delete key:", r)
try:
    d = json.loads(r)
    report["deleteKeyDialog"] = d.get("dialog")
    report["deleteKeyRemoved"] = d.get("gone")
except Exception:
    report["deleteKeyDialog"] = False
    report["deleteKeyRemoved"] = False

# ---- 清理 ----
r = ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  let out = {}
  try {
    await window.__TAURI_INTERNALS__.invoke('sftp_rm', { sid: tab.sid, path: '/root/%NAME%' })
  } catch (e) { out.rm = String(e) }
  const tr = %PINIA%._s.get('transfers')
  tr.clearFinished()
  return JSON.stringify(out)
})()""".replace("%PINIA%", PINIA).replace("%NAME%", NAME))
print("cleanup remote:", r)
for p in (LOCAL_FILE, os.path.join(LOCAL_DIR, NAME)):
    try:
        os.remove(p)
    except OSError:
        pass
try:
    os.rmdir(LOCAL_DIR)
except OSError:
    pass

print("\n==== RESULT ====")
fails = [k for k, v in report.items() if not v]
print(json.dumps(report, indent=1, ensure_ascii=False))
print("PASS" if not fails else f"FAIL: {fails}")
