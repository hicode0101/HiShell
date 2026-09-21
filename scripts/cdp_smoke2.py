#!/usr/bin/env python3
"""冒烟测试：本地终端 / 终端配色 / 会话导入 / 断点续传"""
import json, time, websocket, urllib.request

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
                return {"__error__": res["exceptionDetails"].get("exception", {}).get("description", "?")}
            return res.get("result", {}).get("value")

PINIA = """document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"""

print("=== 1. 本地终端 ===")
print("open:", ev(f"""(async () => {{
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.createLocalTab()
  for (let i = 0; i < 20; i++) {{
    await new Promise(r => setTimeout(r, 300))
    if (tab.lid !== undefined) return 'lid=' + tab.lid
    if (tab.status === 'disconnected') return 'failed: ' + tab.statusMessage
  }}
  return 'timeout, status=' + tab.status
}})()"""))

print("shell prompt:", ev(f"""(async () => {{
  const reg = window.__termReg
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.kind === 'local')
  const entry = reg.get(tab.id)
  if (!entry) return 'no entry'
  await new Promise(r => setTimeout(r, 1500))
  let text = ''
  for (let i = 0; i < entry.term.buffer.active.length; i++) {{
    const line = entry.term.buffer.active.getLine(i)
    if (line) text += line.translateToString() + '\\n'
  }}
  return text.slice(-400).replace(/\\n+/g, ' | ')
}})()"""))

print("echo roundtrip:", ev(f"""(async () => {{
  const reg = window.__termReg
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.kind === 'local')
  const entry = reg.get(tab.id)
  entry.term.input('echo hishell-smoke-777\\r')
  await new Promise(r => setTimeout(r, 1200))
  let text = ''
  for (let i = 0; i < entry.term.buffer.active.length; i++) {{
    const line = entry.term.buffer.active.getLine(i)
    if (line) text += line.translateToString() + '\\n'
  }}
  return text.includes('hishell-smoke-777') ? 'ECHO OK' : 'ECHO MISSING: ' + text.slice(-300)
}})()"""))

print("=== 2. 终端配色方案 ===")
print("switch light:", ev(f"""(async () => {{
  const settings = {PINIA}._s.get('settings')
  settings.patch({{ terminalColorScheme: 'light' }})
  await new Promise(r => setTimeout(r, 300))
  const reg = window.__termReg
  const bgs = [...reg.values()].map(e => e.term.options.theme.background)
  settings.patch({{ terminalColorScheme: 'dark' }})
  await new Promise(r => setTimeout(r, 200))
  const bgs2 = [...reg.values()].map(e => e.term.options.theme.background)
  return JSON.stringify({{ light: bgs[0], dark: bgs2[0] }})
}})()"""))

print("=== 3. 会话导入 ===")
print("scan:", ev("""(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke
  const found = await inv('import_scan')
  return 'scan found: ' + found.length + ' -> ' + JSON.stringify(found.slice(0, 3))
})()"""))

print("apply+cleanup:", ev("""(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke
  const r = await inv('import_apply', { candidates: [{
    source: 'xshell', name: 'smoke-import-test', host: '203.0.113.99', port: 2222,
    username: 'smokeuser', auth: 'password', privateKeyPath: '', originPath: 'C:\\\\fake\\\\t.xsh' }] })
  const profiles = await inv('profile_list')
  const added = profiles.find(p => p.host === '203.0.113.99')
  const dup = await inv('import_apply', { candidates: [{
    source: 'xshell', name: 'smoke-import-test', host: '203.0.113.99', port: 2222,
    username: 'smokeuser', auth: 'password', privateKeyPath: '', originPath: 'C:\\\\fake\\\\t.xsh' }] })
  if (added) await inv('profile_delete', { id: added.id })
  return JSON.stringify({ first: r, profileCreated: !!added, dedup: dup })
})()"""))

print("=== 4. 断点续传（真实服务器） ===")
print("connect:", ev(f"""(async () => {{
  const profiles = {PINIA}._s.get('profiles')
  await profiles.init()
  const sessions = {PINIA}._s.get('sessions')
  const p = profiles.profiles.find(x => x.host === '192.168.209.130')
  if (!p) return 'no profile'
  sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 24; i++) {{
    await new Promise(r => setTimeout(r, 500))
    const tab = sessions.tabs.find(t => t.status === 'connected')
    if (tab) return 'connected sid=' + tab.sid
  }}
  const cur = sessions.tabs.map(t => t.status + ':' + (t.statusMessage || '')).join(',')
  return 'not connected: ' + cur
}})()"""))

print("upload + cancel mid-way:", ev(f"""(async () => {{
  const transfers = {PINIA}._s.get('transfers')
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected' && t.kind === 'session')
  if (!tab) return 'no connected session tab'
  await transfers.submit({{
    sid: tab.sid, name: 'test-resume.bin', isDir: false, direction: 'upload',
    localPath: 'D:\\\\AiWork\\\\work-2\\\\HiShell\\\\test-resume.bin',
    remotePath: '/root/test-resume.bin', total: 8388608,
  }})
  await new Promise(r => setTimeout(r, 600))
  const it = transfers.items.find(i => i.name === 'test-resume.bin')
  if (!it) return 'item missing'
  const atCancel = it.transferred
  transfers.cancel(it.id)
  await new Promise(r => setTimeout(r, 800))
  return JSON.stringify({{ id: it.id, status: it.status, transferredAtCancel: atCancel }})
}})()"""))

print("retry with resume:", ev(f"""(async () => {{
  const transfers = {PINIA}._s.get('transfers')
  const it = transfers.items.find(i => i.name === 'test-resume.bin')
  if (!it) return 'item missing'
  const oldId = it.id, oldStatus = it.status
  await transfers.retry(oldId)
  await new Promise(r => setTimeout(r, 100))
  return JSON.stringify({{ oldId, oldStatus, newId: it.id, newStatus: it.status }})
}})()"""))

for i in range(20):
    time.sleep(1)
    s = ev(f"""(() => {{
      const st = {PINIA}.state.value
      const it = st.transfers.items.find(i => i.name === 'test-resume.bin')
      return it ? JSON.stringify({{ status: it.status, transferred: it.transferred, total: it.total }}) : 'gone'
    }})()""")
    if '"done"' in str(s) or 'failed' in str(s) or 'gone' in str(s):
        break
print("resume progress:", s)

print("remote size check:", ev("""(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke
  const app = document.querySelector('#app').__vue_app__
  const st = app.config.globalProperties.$pinia.state.value
  const tab = st.sessions.tabs.find(t => t.status === 'connected' && t.kind === 'session')
  const stat = await inv('sftp_stat', { sid: tab.sid, path: '/root/test-resume.bin' })
  return 'remote size: ' + JSON.stringify(stat)
})()"""))

print("cleanup remote:", ev("""(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke
  const st = document.querySelector('#app').__vue_app__.config.globalProperties.$pinia.state.value
  const tab = st.sessions.tabs.find(t => t.status === 'connected' && t.kind === 'session')
  await inv('sftp_rm', { sid: tab.sid, path: '/root/test-resume.bin' })
  return 'removed'
})()"""))
