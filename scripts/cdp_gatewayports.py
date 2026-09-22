#!/usr/bin/env python3
"""检查服务器 sshd GatewayPorts 设置（经 HiShell 会话）"""
import json, time, urllib.request, websocket

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=120)
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

# 会话已在（上一步连接过）；确认并等待输出
r = ev("""(async () => {
  try {
    const sessions = %PINIA%._s.get('sessions')
    let tab = sessions.tabs.find(t => t.status === 'connected')
    if (!tab) {
      const profiles = %PINIA%._s.get('profiles')
      await profiles.init()
      const p = profiles.profiles.find(x => x.host === '149.104.22.80')
      tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
      for (let i = 0; i < 40; i++) {
        await new Promise(r2 => setTimeout(r2, 500))
        if (tab.status === 'connected' && tab.sid !== undefined) break
      }
    }
    if (tab.status !== 'connected') return 'not connected: ' + tab.status

    window.__out = ''
    const { listen } = window.__TAURI__.event
    if (!window.__unGP) window.__unGP = await listen('ssh-data', (e) => {
      const b64 = e.payload && e.payload.data
      if (b64) { window.__out = (window.__out + atob(b64)).slice(-3000) }
    })
    const cmd = 'sshd -T 2>/dev/null | grep -i gatewayports; grep -i gatewayports /etc/ssh/sshd_config; echo DONE_MARK\\r'
    const bytes = new TextEncoder().encode(cmd)
    let bin = ''
    bytes.forEach(b => bin += String.fromCharCode(b))
    const b64 = btoa(bin)
    await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: b64 })
    await new Promise(r2 => setTimeout(r2, 2500))
    const lines = window.__out.split('\n').filter(l => /gatewayport|DONE_MARK/i.test(l))
    return JSON.stringify(lines)
  } catch (e) { return 'ERR: ' + String(e) }
})()""".replace("%PINIA%", PINIA))
print(r)
