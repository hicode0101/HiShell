#!/usr/bin/env python3
"""验证：连续导航后状态条计数与当前目录一致 + 单位显示"""
import json, time, urllib.request, websocket

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=90)
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
                return {"__error__": (ex.get("description") or ex.get("value") or "?")[:200]}
            return res.get("result", {}).get("value")

PINIA = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"

def strip_text():
    return ev("""(() => {
      const s = document.querySelector('.sftp-status')
      return s ? s.textContent.trim().replace(/\\s+/g, ' ') : 'no strip'
    })()""")

def dblclick(label):
    return ev("""(async () => {
  const panes = [...document.querySelectorAll('.file-pane')]
  const remote = panes.find(p => p.classList.contains('is-remote')) || panes[panes.length - 1]
  const row = [...remote.querySelectorAll('.ft-row')].find(r => r.textContent.includes('%LBL%'))
  if (!row) return 'not found: %LBL%'
  row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  await new Promise(r => setTimeout(r, 1500))
  return 'navigated'
})()""".replace('%LBL%', label))

def click_row(label):
    return ev("""(async () => {
  const panes = [...document.querySelectorAll('.file-pane')]
  const remote = panes.find(p => p.classList.contains('is-remote')) || panes[panes.length - 1]
  const row = [...remote.querySelectorAll('.ft-row')].find(r => r.textContent.includes('%LBL%'))
  if (!row) return 'not found: %LBL%'
  row.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await new Promise(r => setTimeout(r, 400))
  return 'clicked'
})()""".replace('%LBL%', label))

print(ev(f"""(async () => {{
  const pinia = {PINIA}
  const profiles = pinia._s.get('profiles')
  await profiles.init()
  const sessions = pinia._s.get('sessions')
  const p = profiles.profiles.find(x => x.host === '149.104.22.80')
  const tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 40; i++) {{
    await new Promise(r => setTimeout(r, 500))
    if (tab.status === 'connected' && tab.sid !== undefined) break
  }}
  sessions.setViewMode(tab.id, 'sftp')
  await new Promise(r => setTimeout(r, 2000))
  return 'ready'
}})()"""))

print("1 /root:", strip_text())
print(dblclick('cert'))
print("2 /root/cert:", strip_text())
print(dblclick('v.hicode.top'))
print("3 /root/cert/v.hicode.top:", strip_text())
print(click_row('fullchain.pem'))
print("4 选中 fullchain.pem:", strip_text())
