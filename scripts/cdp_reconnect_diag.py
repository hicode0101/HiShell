#!/usr/bin/env python3
"""诊断：全新会话 vs 重连会话——终端响应 + 动态转发连通性 + 通道存活"""
import base64, json, time, urllib.request, websocket

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
                return {"__error__": "?"}
            return res.get("result", {}).get("value")

PINIA = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"

# 安装 ssh-data 事件监听（捕获最近输出，用于终端回显检查）
ev("""(() => {
  window.__lastData = ''
  const { listen } = window.__TAURI__.event
  if (!window.__unlistenData) {
    listen('ssh-data', (e) => {
      const b64 = e.payload && e.payload.data
      if (b64) window.__lastData += atob(b64)
      if (window.__lastData.length > 4000) window.__lastData = window.__lastData.slice(-2000)
    }).then(u => window.__unlistenData = u)
  }
  return 'listener ready'
})()""")

def fresh_session():
    return ev("""(async () => {
  const pinia = %PINIA%
  const profiles = pinia._s.get('profiles')
  await profiles.init()
  const sessions = pinia._s.get('sessions')
  // 关掉现有 tab
  for (const t of [...sessions.tabs]) { try { sessions.disconnectById(t.id) } catch {} }
  await new Promise(r => setTimeout(r, 1200))
  const p = profiles.profiles.find(x => x.host === '149.104.22.80')
  const tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 40; i++) {
    await new Promise(r => setTimeout(r, 500))
    if (tab.status === 'connected' && tab.sid !== undefined) return 'fresh connected sid=' + tab.sid
  }
  return 'failed: ' + tab.status
})()""".replace("%PINIA%", PINIA))

def reconnect():
    return ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  sessions.disconnectById(tab.id)
  await new Promise(r => setTimeout(r, 1500))
  sessions.reconnect(tab.id)
  for (let i = 0; i < 40; i++) {
    await new Promise(r => setTimeout(r, 500))
    if (tab.status === 'connected' && tab.sid !== undefined) return 'reconnected sid=' + tab.sid
  }
  return 'failed: ' + tab.status
})()""".replace("%PINIA%", PINIA))

def echo_test():
    return ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  if (!tab) return 'no connected tab'
  window.__lastData = ''
  const bytes = new TextEncoder().encode('echo MARKER_$$$_' + Date.now() + '\\r')
  let bin = ''
  bytes.forEach(b => bin += String.fromCharCode(b))
  const b64 = btoa(bin)
  await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: b64 })
  await new Promise(r => setTimeout(r, 1500))
  const ok = window.__lastData.includes('MARKER_$$$_')
  const status = tab.status
  return JSON.stringify({ echoReceived: ok, statusAfter: status })
})()""".replace("%PINIA%", PINIA))

def add_dyn_and_report(port):
    return ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  if (!tab) return 'no connected tab'
  const rule = { id: 'diag-' + %PORT%, type: 'dynamic', bindHost: '127.0.0.1', bindPort: %PORT%, description: 'diag' }
  try {
    await window.__TAURI_INTERNALS__.invoke('forward_add', { req: { sid: tab.sid, rule } })
    return 'added sid=' + tab.sid
  } catch (e) { return 'ERR: ' + String(e) }
})()""".replace("%PINIA%", PINIA).replace('%PORT%', str(port)))

def status_after(seconds):
    time.sleep(seconds)
    return ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs[0]
  const list = tab.sid !== undefined ? await window.__TAURI_INTERNALS__.invoke('forward_list', { sid: tab.sid }).catch(e => 'ERR ' + e) : 'no sid'
  return JSON.stringify({ status: tab.status, forwards: list })
})()""".replace("%PINIA%", PINIA))

print("### A. 全新会话 ###")
print(fresh_session())
print("echo:", echo_test())
print("dyn:", add_dyn_and_report(18090))
print("t+2s:", status_after(2))
print("t+17s:", status_after(15))

print("### B. 重连会话 ###")
print(reconnect())
print("echo:", echo_test())
print("dyn:", add_dyn_and_report(18091))
print("t+2s:", status_after(2))
print("t+17s:", status_after(15))
