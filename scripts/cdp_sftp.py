#!/usr/bin/env python3
"""驱动：连接 192.168.209.130 → 切到文件视图 → 读取远程窗格真实内容"""
import json, time, websocket, urllib.request

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=20)
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
                return {"__error__": res["exceptionDetails"].get("exception", {}).get("description", "?")}
            return res.get("result", {}).get("value")

def state():
    return ev("""(() => {
      const app = document.querySelector('#app').__vue_app__
      const st = app.config.globalProperties.$pinia.state.value
      return {
        tabs: st.sessions.tabs.map(t => ({ id: t.id, title: t.title, sid: t.sid, status: t.status })),
        prompts: st.sessions.prompts.map(p => ({ sid: p.sid, kind: p.payload && p.payload.kind })),
      }
    })()""")

# 连接 e2e profile（192.168.209.130，凭据已在 keyring）
ev("""(() => {
  const app = document.querySelector('#app').__vue_app__
  const st = app.config.globalProperties.$pinia.state.value
  const sessions = st ? null : null
  return 1
})()""")

ev("""(async () => {
  const app = document.querySelector('#app').__vue_app__
  const pinia = app.config.globalProperties.$pinia
  await pinia._s.get('profiles').init()
  const sessions = pinia._s.get('sessions')
  const profiles = pinia._s.get('profiles')
  const p = profiles.profiles.find(x => x.host === '192.168.209.130')
  if (!p) { window.__d = 'no profile for 192.168.209.130'; return }
  sessions.createSession(JSON.parse(JSON.stringify(p)))
  window.__d = 'created: ' + p.id
})()""")
print("create:", ev("window.__d"))

# 等待 connected，并自动应答提示（密码已在 keyring 时不会弹）
for i in range(15):
    time.sleep(1)
    st = state()
    tab = next((t for t in st["tabs"] if t.get("status") == "connected"), st["tabs"][-1] if st["tabs"] else None)
    print(f"[{i+1}s]", tab)
    if tab and tab["status"] == "connected":
        break
    # 若弹出密码提示：输入密码并确认（fallback：从 keyring 已存则不会弹）
    if st["prompts"]:
        kind = st["prompts"][0]["kind"]
        sid = st["prompts"][0]["sid"]
        print(f"  -> prompt {kind}, 尝试应答")
        if kind == "password":
            ev(f"""(async () => {{
              const inv = window.__TAURI_INTERNALS__.invoke
              await inv('ssh_prompt_response', {{ sid: {sid}, answer: {{ key: 'password', value: {{ password: 'Wxhq@123456', remember: true }} }} }})
            }})()""")

import sys
if not (tab and tab["status"] == "connected"):
    print("未能连接，退出")
    sys.exit(1)

# 切到文件视图
ev("""(() => {
  const app = document.querySelector('#app').__vue_app__
  const st = app.config.globalProperties.$pinia.state.value
  st.sessions.setViewMode(st.sessions.activeTabId, 'sftp')
})()""")
time.sleep(2)

# 读取远程窗格内容
out = ev("""(() => {
  const panes = document.querySelectorAll('.file-pane')
  const remote = panes[panes.length - 1]
  const rows = [...remote.querySelectorAll('.ft-row')].map(r => {
    const cols = [...r.children].map(c => c.textContent.trim())
    return cols.join(' | ')
  })
  return { rows: rows.slice(0, 15) }
})()""")
print(json.dumps(out, ensure_ascii=False, indent=1))
