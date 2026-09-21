#!/usr/bin/env python3
"""冒烟：FilePane 真实操作（新建/重命名/删除/属性/打开）走真实服务器 + 本地文件操作"""
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
    const tab = sessions.tabs.find(t => t.status === 'connected')
    if (tab && tab.sid !== undefined) return 'connected sid=' + tab.sid
  }}
  return 'not connected'
}})()"""))

print("sftp view:", ev(f"""(async () => {{
  const sessions = {PINIA}._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  sessions.setViewMode(tab.id, 'sftp')
  await new Promise(r => setTimeout(r, 1500))
  const pane = document.querySelector('.file-pane.is-remote')
  return pane ? 'remote pane rendered' : 'no remote pane'
}})()"""))

def remote_state():
    return ev(f"""(() => {{
      const pane = document.querySelector('.file-pane.is-remote')
      const st = pane.__vueParentComponent.setupState
      return JSON.stringify({{ path: st.path, names: st.entries.map(e => e.name).slice(0, 30) }})
    }})()""")

print("== 新建文件夹 ==")
print(ev(f"""(async () => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  st.dialog = 'newFolder'
  st.dialogName = 'hishell-ui-test'
  await st.applyNewFolder()
  await new Promise(r => setTimeout(r, 800))
  return st.entries.map(e => e.name).includes('hishell-ui-test') ? 'MKDIR OK' : 'MISSING: ' + st.entries.map(e=>e.name).join(',')
}})()"""))

print("== 重命名 ==")
print(ev(f"""(async () => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  const entry = st.entries.find(e => e.name === 'hishell-ui-test')
  st.dialogTargets = [JSON.parse(JSON.stringify(entry))]
  st.dialogName = 'hishell-ui-test-renamed'
  await st.applyRename()
  await new Promise(r => setTimeout(r, 800))
  const names = st.entries.map(e => e.name)
  return names.includes('hishell-ui-test-renamed') && !names.includes('hishell-ui-test') ? 'RENAME OK' : 'FAIL: ' + names.join(',')
}})()"""))

print("== 属性 ==")
print(ev(f"""(async () => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  const entry = st.entries.find(e => e.name === '.bashrc')
  await st.openProps(JSON.parse(JSON.stringify(entry)))
  await new Promise(r => setTimeout(r, 600))
  return JSON.stringify(st.propsData)
}})()"""))

print("== 删除（递归目录） ==")
print(ev(f"""(async () => {{
  const pane = document.querySelector('.file-pane.is-remote')
  const st = pane.__vueParentComponent.setupState
  const entry = st.entries.find(e => e.name === 'hishell-ui-test-renamed')
  st.dialogTargets = [JSON.parse(JSON.stringify(entry))]
  await st.applyDelete()
  await new Promise(r => setTimeout(r, 800))
  return !st.entries.map(e => e.name).includes('hishell-ui-test-renamed') ? 'DELETE OK' : 'FAIL'
}})()"""))

print("== 本地窗格操作 ==")
print(ev(f"""(async () => {{
  const inv = window.__TAURI_INTERNALS__.invoke
  const base = await inv('fs_special_dir', {{ kind: 'temp' }})
  const dir = base.replace(/[\\\\/]+$/, '') + '\\\\hishell-local-test'
  await inv('fs_mkdir', {{ path: dir }})
  await inv('fs_rename', {{ oldPath: dir, newPath: dir + '-2' }})
  const stat = await inv('fs_stat', {{ path: dir + '-2' }})
  await inv('fs_delete', {{ path: dir + '-2' }})
  return JSON.stringify({{ kind: stat.kind, readonly: stat.readonly, deleted: true }})
}})()"""))

print("== 会话管理器 / 快捷键对话框渲染 ==")
print(ev(f"""(async () => {{
  window.dispatchEvent(new CustomEvent('hishell-command', {{ detail: 'session-manager' }}))
  await new Promise(r => setTimeout(r, 500))
  const rows = document.querySelectorAll('.sm-row').length
  document.querySelector('.sm-spacer')?.parentElement?.querySelectorAll('button')[1]?.click()
  window.dispatchEvent(new CustomEvent('hishell-command', {{ detail: 'help-hotkeys' }}))
  await new Promise(r => setTimeout(r, 400))
  const hk = document.querySelectorAll('.hotkey-row').length
  return JSON.stringify({{ sessionRows: rows, hotkeyRows: hk }})
}})()"""))
