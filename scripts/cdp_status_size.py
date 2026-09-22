#!/usr/bin/env python3
"""验证：进入 .ssh 后单选 known_hosts → 状态栏显示字节 + 过滤后的计数"""
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
  const certRow = [...remote.querySelectorAll('.ft-row')].find(r2 => r2.textContent.includes('cert'))
  if (!certRow) return 'no cert row'
  certRow.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  await new Promise(r => setTimeout(r, 1500))
  const vrow = [...remote.querySelectorAll('.ft-row')].find(r2 => r2.textContent.includes('v.hicode.top'))
  if (!vrow) return 'no v.hicode.top row'
  vrow.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  await new Promise(r => setTimeout(r, 1500))
  const row = [...remote.querySelectorAll('.ft-row')].find(r2 => r2.textContent.includes('fullchain.pem'))
  if (!row) return 'no fullchain.pem row'
  row.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await new Promise(r => setTimeout(r, 400))
  const strip = document.querySelector('.sftp-status')
  const text = strip.textContent.replace(/\\s+/g, ' ').trim()
  const counts = text.match(/文件: \\d+ 文件夹: \\d+/)
  return JSON.stringify({ size: strip.querySelector('.ss-size')?.textContent.trim(), counts: counts && counts[0] })
})()""".replace("%PINIA%", PINIA)

print(ev(js))
