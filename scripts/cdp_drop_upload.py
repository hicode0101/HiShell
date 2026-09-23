#!/usr/bin/env python3
"""E2E：系统拖放上传（Tauri drag-drop 真实路径）+ 传输队列列宽拖拽验证

流程：
 1. 连接 192.168.209.130 会话（profile + keyring 密码）
 2. 切到「文件」视图
 3. 本地建测试文件 → 模拟 tauri://drag-drop（物理坐标命中远程栏）
 4. 校验：高亮、队列条目 localPath == 真实路径、状态 done、sftp_stat 文件存在
 5. 校验本地栏拖放单目录 → 左栏跳转
 6. 传输队列列宽拖拽 +40px、灰分割线
 7. 清理远程与本地测试文件
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
LOCAL_FILE = os.path.join(os.environ.get("TEMP", r"C:\Users\QWERT\AppData\Local\Temp"),
                          "hishell_drop_e2e.txt")
LOCAL_DIR = os.path.join(os.environ.get("TEMP", ""), "hishell_drop_dir")
REMOTE_NAME = "hishell_drop_e2e.txt"
CONTENT = b"hishell drop-upload e2e\n" * 4000  # ~88 KB

with open(LOCAL_FILE, "wb") as f:
    f.write(CONTENT)
os.makedirs(LOCAL_DIR, exist_ok=True)
LOCAL_SIZE = os.path.getsize(LOCAL_FILE)
print("local test file:", LOCAL_FILE, LOCAL_SIZE, "bytes")

report = {}

# ---- 1. 连接会话（遍历常用主机直到连上） ----
r = ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const profiles = %PINIA%._s.get('profiles')
  await profiles.init()
  let tab = sessions.tabs.find(t => t.status === 'connected')
  if (!tab) {
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
  if (!tab || tab.status !== 'connected') return 'not connected: ' + (tab ? tab.status : 'no tab')
  tab.viewMode = 'sftp'
  await new Promise(r2 => setTimeout(r2, 800))
  return JSON.stringify({ sid: tab.sid, viewMode: tab.viewMode })
})()""".replace("%PINIA%", PINIA))
print("connect:", r)
if not (isinstance(r, str) and r.startswith("{")):
    raise SystemExit("session unavailable")

# ---- 2. 拖放命中远程栏：先 enter 高亮，再 drop ----
r = ev("""(async () => {
  const pane = document.querySelector('.file-pane.is-remote')
  if (!pane) return 'no remote pane'
  const r0 = pane.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  const px = Math.round((r0.left + r0.width / 2) * dpr)
  const py = Math.round((r0.top + r0.height / 2) * dpr)
  const invoke = window.__TAURI_INTERNALS__.invoke
  await invoke('plugin:event|emit', { event: 'tauri://drag-enter',
    payload: { paths: ['%FILE%'], position: { x: px, y: py } } })
  await new Promise(r2 => setTimeout(r2, 300))
  const hl = pane.classList.contains('drag-over')
  await invoke('plugin:event|emit', { event: 'tauri://drag-drop',
    payload: { paths: ['%FILE%'], position: { x: px, y: py } } })
  await new Promise(r2 => setTimeout(r2, 400))
  const hlAfterDrop = pane.classList.contains('drag-over')
  return JSON.stringify({ hl, hlAfterDrop })
})()""".replace("%FILE%", LOCAL_FILE.replace("\\", "\\\\")))
print("drop-on-remote:", r)
report["hoverHighlight"] = isinstance(r, str) and '"hl":true' in r
report["highlightClearedAfterDrop"] = isinstance(r, str) and '"hlAfterDrop":false' in r

# ---- 3. 队列条目与真实路径 ----
ok_item = False
for _ in range(30):
    r = ev("""(async () => {
      const tr = %PINIA%._s.get('transfers')
      const it = [...tr.items].reverse().find(i => i.name === '%NAME%')
      if (!it) return JSON.stringify({ found: false })
      return JSON.stringify({ found: true, localPath: it.localPath, remotePath: it.remotePath,
        total: it.total, transferred: it.transferred, status: it.status })
    })()""".replace("%PINIA%", PINIA).replace("%NAME%", REMOTE_NAME))
    d = json.loads(r) if isinstance(r, str) and r.startswith("{") else {}
    if d.get("status") in ("done", "failed"):
        ok_item = d
        break
    time.sleep(0.5)
print("queue item:", ok_item)
if isinstance(ok_item, dict):
    report["realLocalPath"] = ok_item.get("localPath", "").lower() == LOCAL_FILE.lower()
    report["realSize"] = ok_item.get("total") == LOCAL_SIZE
    report["uploadDone"] = ok_item.get("status") == "done"

# ---- 4. 远程文件确实存在 ----
r = ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  try {
    const st = await window.__TAURI_INTERNALS__.invoke('sftp_stat', { sid: tab.sid, path: '/root/%NAME%' })
    return JSON.stringify({ exists: true, size: st.size, kind: st.kind })
  } catch (e) { return JSON.stringify({ exists: false, err: String(e) }) }
})()""".replace("%PINIA%", PINIA).replace("%NAME%", REMOTE_NAME))
print("remote stat:", r)
report["remoteFileExists"] = isinstance(r, str) and '"exists":true' in r

# ---- 5. 拖到本地栏：单目录 → 左栏定位 ----
r = ev("""(async () => {
  const pane = document.querySelector('.file-pane:not(.is-remote)')
  const r0 = pane.getBoundingClientRect()
  const dpr = window.devicePixelRatio || 1
  const px = Math.round((r0.left + r0.width / 2) * dpr)
  const py = Math.round((r0.top + r0.height / 2) * dpr)
  const invoke = window.__TAURI_INTERNALS__.invoke
  await invoke('plugin:event|emit', { event: 'tauri://drag-drop',
    payload: { paths: ['%DIR%'], position: { x: px, y: py } } })
  await new Promise(r2 => setTimeout(r2, 1200))
  const inp = pane.querySelector('.pb-input')
  return JSON.stringify({ path: inp ? inp.value : null })
})()""".replace("%DIR%", LOCAL_DIR.replace("\\", "\\\\")))
print("drop-dir-on-local:", r)
report["localDirNavigate"] = isinstance(r, str) and "hishell_drop_dir" in r

# ---- 6. 传输队列列宽拖拽 + 灰分割线 ----
r = ev("""(async () => {
  const grip = document.querySelector('.tq-grip')
  const head = document.querySelector('.tq-head')
  if (!grip || !head) return 'no transfer table'
  const before = head.style.gridTemplateColumns
  const r0 = grip.getBoundingClientRect()
  const x0 = r0.left + r0.width / 2, y0 = r0.top + r0.height / 2
  grip.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, clientX: x0, clientY: y0 }))
  document.dispatchEvent(new MouseEvent('mousemove', { bubbles: true, clientX: x0 + 40, clientY: y0 }))
  document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, clientX: x0 + 40, clientY: y0 }))
  await new Promise(r2 => setTimeout(r2, 200))
  const after = head.style.gridTemplateColumns
  const cell = document.querySelector('.tq-head > span')
  const border = getComputedStyle(cell).borderRightColor
  return JSON.stringify({ before: before.split(' ').slice(0, 3), after: after.split(' ').slice(0, 3), border })
})()""")
print("col-resize:", r)
ok = False
try:
    d = json.loads(r)
    b0 = float(d["before"][0].replace("px", ""))
    a0 = float(d["after"][0].replace("px", ""))
    ok = abs((a0 - b0) - 40) < 3
    report["dividerGray"] = d["border"] not in ("rgba(0, 0, 0, 0)", "none")
except Exception as e:
    print("parse err", e)
report["queueColResize"] = ok

# ---- 7. 清理 ----
r = ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  let rm = 'ok'
  try { await window.__TAURI_INTERNALS__.invoke('sftp_rm', { sid: tab.sid, path: '/root/%NAME%' }) }
  catch (e) { rm = String(e) }
  const tr = %PINIA%._s.get('transfers')
  tr.clearFinished()
  return rm
})()""".replace("%PINIA%", PINIA).replace("%NAME%", REMOTE_NAME))
try:
    os.remove(LOCAL_FILE)
    os.rmdir(LOCAL_DIR)
except OSError:
    pass
print("cleanup remote:", r)

print("\n==== RESULT ====")
fails = [k for k, v in report.items() if not v]
print(json.dumps(report, indent=1, ensure_ascii=False))
print("PASS" if not fails else f"FAIL: {fails}")
