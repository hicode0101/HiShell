#!/usr/bin/env python3
"""诊断 xterm viewport 滚动条状态"""
import json, time, urllib.request, websocket

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=60)
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
                return {"__error__": (ex.get("description") or "?")[:200]}
            return res.get("result", {}).get("value")

r = ev("""(async () => {
  const sessions = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia._s.get('sessions')
  const tab = sessions.tabs[0]
  if (!tab || tab.status !== 'connected') return 'no session: ' + (tab ? tab.status : 'no tab')
  const cmd = 'for i in $(seq 1 60); do echo LINE_$i; done' + String.fromCharCode(13)
  const bytes = new TextEncoder().encode(cmd)
  let bin = ''
  bytes.forEach(b => bin += String.fromCharCode(b))
  await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: btoa(bin) })
  await new Promise(r2 => setTimeout(r2, 2500))
  const vp = document.querySelector('.xterm-viewport')
  if (!vp) return 'no viewport'
  const cs = getComputedStyle(vp)
  return JSON.stringify({
    overflowY: cs.overflowY,
    scrollHeight: vp.scrollHeight,
    clientHeight: vp.clientHeight,
    scrollTop: Math.round(vp.scrollTop),
    bg: cs.backgroundColor,
    width: vp.offsetWidth,
  })
})()""")
print(r)
