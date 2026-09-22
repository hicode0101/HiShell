#!/usr/bin/env python3
"""验证远程文件右键菜单顺序：显示隐藏文件位于更改权限之后"""
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

js = """(async () => {
  const pinia = %PINIA%
  const profiles = pinia._s.get('profiles')
  await profiles.init()
  const sessions = pinia._s.get('sessions')
  const p = profiles.profiles.find(x => x.host === '149.104.22.80')
  const tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 40; i++) {
    await new Promise(r => setTimeout(r, 500))
    if (tab.status === 'connected' && tab.sid !== undefined) break
  }
  sessions.setViewMode(tab.id, 'sftp')
  await new Promise(r => setTimeout(r, 2000))
  const panes = [...document.querySelectorAll('.file-pane')]
  const remote = panes.find(p2 => p2.classList.contains('is-remote')) || panes[panes.length - 1]
  const row = [...remote.querySelectorAll('[class*=row]')].find(r2 => r2.textContent.includes('cert') && !r2.textContent.includes('..'))
  if (!row) return 'cert row not found'
  const r = row.getBoundingClientRect()
  row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 40, clientY: r.top + 10 }))
  await new Promise(r => setTimeout(r, 500))
  const menu = document.getElementById('f-context-menu')
  if (!menu) return 'menu not open'
  const items = [...menu.querySelectorAll('.f-ctx-item')]
  return JSON.stringify(items.map(i => ({
    label: i.querySelector('.f-ctx-label')?.textContent.trim(),
    checkIcon: !!i.querySelector('.f-ctx-icon [class*=check]'),
  }))) + ' seps=' + menu.querySelectorAll('.f-ctx-sep').length
})()""".replace("%PINIA%", PINIA)

print(ev(js))
