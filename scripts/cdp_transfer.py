#!/usr/bin/env python3
"""直调 transfers.submit 验证上传/下载后端链路"""
import json, time, websocket, urllib.request

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=30)
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

# 先连接（e2e-real profile = 192.168.209.130，密码已在 keyring）
print("connect:", ev("""(async () => {
  const app = document.querySelector('#app').__vue_app__
  const pinia = app.config.globalProperties.$pinia
  await pinia._s.get('profiles').init()
  const profiles = pinia._s.get('profiles')
  const sessions = pinia._s.get('sessions')
  const p = profiles.profiles.find(x => x.host === '192.168.209.130')
  if (!p) return 'no profile'
  sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 20; i++) {
    await new Promise(r => setTimeout(r, 500))
    const tab = sessions.tabs.find(t => t.status === 'connected')
    if (tab) return 'connected sid=' + tab.sid
  }
  const cur = sessions.tabs.map(t => t.status).join(',')
  return 'not connected: ' + cur
})()"""))

# 上传：本机 hishell.log → /root/hishell-upload-test.txt
print("upload:", ev("""(async () => {
  const app = document.querySelector('#app').__vue_app__
  const pinia = app.config.globalProperties.$pinia
  const sessions = pinia._s.get('sessions')
  const transfers = pinia._s.get('transfers')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  if (!tab) return 'no connected tab'
  await transfers.submit({
    sid: tab.sid, name: 'hishell-upload-test.txt', isDir: false,
    direction: 'upload',
    localPath: 'C:\\\\Users\\\\QWERT\\\\AppData\\\\Roaming\\\\hishell\\\\hishell.log',
    remotePath: '/root/hishell-upload-test.txt',
    total: 1000,
  })
  return JSON.stringify(transfers.items.map(i => ({ id: i.id, name: i.name, status: i.status, transferred: i.transferred, total: i.total })))
})()"""))

# 下载：/root/.bash_logout → 桌面临时目录
print("download:", ev("""(async () => {
  const app = document.querySelector('#app').__vue_app__
  const pinia = app.config.globalProperties.$pinia
  const sessions = pinia._s.get('sessions')
  const transfers = pinia._s.get('transfers')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  await transfers.submit({
    sid: tab.sid, name: '.bash_logout', isDir: false,
    direction: 'download',
    localPath: 'C:\\\\Users\\\\QWERT\\\\AppData\\\\Roaming\\\\hishell\\\\test-download\\\\.bash_logout',
    remotePath: '/root/.bash_logout',
    total: 220,
  })
  await new Promise(r => setTimeout(r, 1500))
  return JSON.stringify(transfers.items.slice(0, 3).map(i => ({ id: i.id, name: i.name, status: i.status, transferred: i.transferred, total: i.total })))
})()"""))

time.sleep(2)
print("final queue:", ev("""(() => {
  const app = document.querySelector('#app').__vue_app__
  const st = app.config.globalProperties.$pinia.state.value
  return JSON.stringify(st.transfers.items.map(i => ({ name: i.name, status: i.status, transferred: i.transferred, total: i.total })))
})()"""))

# 验证远端文件确实存在
print("remote check:", ev("""(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke
  const app = document.querySelector('#app').__vue_app__
  const st = app.config.globalProperties.$pinia.state.value
  const tab = st.sessions.tabs.find(t => t.status === 'connected')
  const exists = await inv('sftp_exists', { sid: tab.sid, path: '/root/hishell-upload-test.txt' })
  return 'remote file exists: ' + exists
})()"""))
