#!/usr/bin/env python3
"""冒烟：更改权限对话框（解析/联动/应用/递归）走真实服务器"""
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

print("connect:", ev(f"""(async () => {{
  const profiles = {PINIA}._s.get('profiles')
  await profiles.init()
  const sessions = {PINIA}._s.get('sessions')
  const p = profiles.profiles.find(x => x.host === '192.168.209.130')
  if (!p) return 'no profile'
  sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 24; i++) {{
    await new Promise(r => setTimeout(r, 500))
    const tab = sessions.tabs.find(t => t.status === 'connected' && t.sid !== undefined)
    if (tab) return 'connected sid=' + tab.sid
  }}
  return 'not connected'
}})()"""))

print("sftp view:", ev(f"""(async () => {{
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  sessions.setViewMode(tab.id, 'sftp')
  await new Promise(r => setTimeout(r, 1500))
  return document.querySelector('.file-pane.is-remote') ? 'ok' : 'no pane'
}})()"""))

print("== 1. 打开权限对话框：.bashrc 应解析出 644 ==")
print(ev(f"""(async () => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  const entry = st.entries.find(e => e.name === '.bashrc')
  if (!entry) return 'no .bashrc'
  await st.openChmod(JSON.parse(JSON.stringify(entry)))
  return JSON.stringify({{ open: st.permDialog, octal: st.permOctal, name: st.permName, isDir: st.permIsDir }})
}})()"""))

print("== 2. 复选框联动：勾选所有者执行 → 744 ==")
print(ev(f"""(() => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  st.setPermBit(0, 2, true)
  const after1 = st.permOctal
  // 再取消组读 → 644? no: 744-004=740
  st.setPermBit(1, 0, false)
  return JSON.stringify({{ ownerExec: after1, groupReadOff: st.permOctal,
    checkedOwnerExec: st.permChecked(0, 2), checkedGroupRead: st.permChecked(1, 0) }})
}})()"""))

print("== 3. 应用 740 到 .bashrc，服务器端校验 ==")
print(ev(f"""(async () => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  st.permOctal = '740'
  await st.applyChmod()
  await new Promise(r => setTimeout(r, 800))
  const inv = window.__TAURI_INTERNALS__.invoke
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  const stat = await inv('sftp_stat', {{ sid: tab.sid, path: '/root/.bashrc' }})
  return JSON.stringify({{ dialogOpen: st.permDialog, attrs: stat.attrs }})
}})()"""))

print("== 4. 递归权限 ==")
print(ev(f"""(async () => {{
  const inv = window.__TAURI_INTERNALS__.invoke
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  await inv('sftp_mkdir', {{ sid: tab.sid, path: '/root/hishell-chmod-test' }})
  // 造一个子文件：用 sftp_list 不行，直接用 set_metadata 不行——用上传跳过；改用 read_dir 空目录递归只改目录本身
  const r = await inv('sftp_chmod', {{ sid: tab.sid, path: '/root/hishell-chmod-test', mode: 0o700, recursive: true }})
  const stat = await inv('sftp_stat', {{ sid: tab.sid, path: '/root/hishell-chmod-test' }})
  return JSON.stringify({{ changed: r, attrs: stat.attrs }})
}})()"""))

print("== 5. 还原 + 清理 ==")
print(ev(f"""(async () => {{
  const inv = window.__TAURI_INTERNALS__.invoke
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  await inv('sftp_chmod', {{ sid: tab.sid, path: '/root/.bashrc', mode: 0o644, recursive: false }})
  await inv('sftp_rm', {{ sid: tab.sid, path: '/root/hishell-chmod-test' }})
  const stat = await inv('sftp_stat', {{ sid: tab.sid, path: '/root/.bashrc' }})
  return 'restored .bashrc -> ' + stat.attrs
}})()"""))
