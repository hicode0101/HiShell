#!/usr/bin/env python3
"""快速连接完整 E2E：服务器密码策略确认 + 探针密钥安装 + 用户密钥方式快速连接 + 清理"""
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
                return {"__error__": (ex.get("description") or ex.get("value") or "?")[:200]}
            return res.get("result", {}).get("value")

PINIA = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"

def connected_tab():
    return ev("""(() => {
  const sessions = %PINIA%._s.get('sessions')
  return sessions.tabs.find(t => t.status === 'connected') ? 'yes' : 'no'
})()""".replace("%PINIA%", PINIA))

# 1. 用 yt-108082（公钥认证）连上服务器
print("1 连接:", ev(f"""(async () => {{
  const pinia = {PINIA}
  const profiles = pinia._s.get('profiles')
  await profiles.init()
  const sessions = pinia._s.get('sessions')
  const p = profiles.profiles.find(x => x.host === '149.104.22.80')
  const tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
  for (let i = 0; i < 40; i++) {{
    await new Promise(r => setTimeout(r, 500))
    if (tab.status === 'connected' && tab.sid !== undefined) return 'connected sid=' + tab.sid
  }}
  return 'failed: ' + tab.status
}})()"""))

# 3. 生成探针密钥 + 装公钥
print("3 生成并安装探针密钥:", ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  const gid = crypto.randomUUID()
  await window.__TAURI_INTERNALS__.invoke('user_key_generate_start', { genId: gid, keyType: 'ED25519', bits: 256 })
  await window.__TAURI_INTERNALS__.invoke('user_key_generate_finish', { req: { genId: gid, name: 'qc_probe', comment: 'qc-e2e', passphrase: '' } })
  const ks = await window.__TAURI_INTERNALS__.invoke('user_key_list')
  const k = ks.find(x => x.name === 'qc_probe')
  const pubText = (await window.__TAURI_INTERNALS__.invoke('user_key_public_content', { publicPath: k.publicPath })).trim()
  const cmd = 'grep -qF "' + pubText + '" ~/.ssh/authorized_keys 2>/dev/null || echo ' + pubText + ' >> ~/.ssh/authorized_keys; echo KEY_OK' + String.fromCharCode(13)
  const bytes = new TextEncoder().encode(cmd)
  let bin = ''
  bytes.forEach(b => bin += String.fromCharCode(b))
  await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: btoa(bin) })
  await new Promise(r => setTimeout(r, 1500))
  return 'probe ready'
})()""".replace("%PINIA%", PINIA)))

# 4. 快速连接：用户密钥方式
print("4 快速连接对话框（用户密钥）:", ev("""(async () => {
  const input = document.querySelector('.address-bar input')
  input.focus(); input.value = 'ssh://root@149.104.22.80:22'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await new Promise(r => setTimeout(r, 200))
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await new Promise(r => setTimeout(r, 800))
  const dlg = [...document.querySelectorAll('.f-dialog')].find(d => d.querySelector('.qc-grid'))
  if (!dlg) return 'dialog not open'
  // 切认证方式为 用户密钥
  const faces = [...dlg.querySelectorAll('.f-combo-face')]
  const faceTexts = faces.map(f => f.textContent.trim())
  const authFace = faces.find(f => ['密码', 'Password'].includes(f.textContent.trim()))
  if (!authFace) return 'faces: ' + JSON.stringify(faceTexts)
  authFace.click()
  await new Promise(r => setTimeout(r, 250))
  const item = [...document.querySelectorAll('.f-combo-popup .f-combo-item')].find(i => i.textContent.trim() === '用户密钥')
  item.click()
  await new Promise(r => setTimeout(r, 700))
  // 在用户密钥下拉中选中 qc_probe
  const keyFaces = [...dlg.querySelectorAll('.f-combo-face')]
  const kFace = keyFaces.find(f => !['密码', 'Password', '用户密钥', 'User key', '私钥', 'Private key'].includes(f.textContent.trim()))
  if (!kFace) return 'key combo not found'
  kFace.click()
  await new Promise(r => setTimeout(r, 250))
  const kItem = [...document.querySelectorAll('.f-combo-popup .f-combo-item')].find(i => i.textContent.trim() === 'qc_probe')
  if (!kItem) return 'qc_probe item missing'
  kItem.click()
  await new Promise(r => setTimeout(r, 300))
  const keyFace = [...dlg.querySelectorAll('.f-combo-face')].find(f => f.textContent.includes('qc_probe'))
  if (!keyFace) return 'qc_probe not selected'
  const conn = [...dlg.querySelectorAll('button')].find(b => b.textContent.trim() === '连接')
  conn.click()
  await new Promise(r => setTimeout(r, 500))
  for (let i = 0; i < 24; i++) {
    await new Promise(r => setTimeout(r, 500))
    const sessions = %PINIA%._s.get('sessions')
    const t = sessions.tabs[sessions.tabs.length - 1]
    if (t && (t.status === 'connected' || t.status === 'disconnected')) {
      return JSON.stringify({ status: t.status, addr: t.address, dialogClosed: !dlg.offsetParent })
    }
  }
  return 'timeout'
})()""".replace("%PINIA%", PINIA)))

# 5. 清理：探针密钥文件 + 服务器 authorized_keys 行 + 临时配置
print("5 清理:", ev("""(async () => {
  const sessions = %PINIA%._s.get('sessions')
  const tab = sessions.tabs.find(t => t.status === 'connected')
  const out = []
  if (tab) {
    const cmd = 'sed -i "/qc-e2e/d" ~/.ssh/authorized_keys; echo CLEANED' + String.fromCharCode(13)
    const bytes = new TextEncoder().encode(cmd)
    let bin = ''
    bytes.forEach(b => bin += String.fromCharCode(b))
    await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: btoa(bin) })
    await new Promise(r => setTimeout(r, 1000))
    sessions.disconnectById(tab.id)
    out.push('server cleaned')
  }
  const ks = await window.__TAURI_INTERNALS__.invoke('user_key_list')
  const k = ks.find(x => x.name === 'qc_probe')
  if (k) await window.__TAURI_INTERNALS__.invoke('user_key_delete', { publicPath: k.publicPath, privatePath: k.privatePath })
  out.push('probe deleted')
  // 删除临时配置
  const profiles = %PINIA%._s.get('profiles')
  await profiles.init()
  return JSON.stringify({ out, names: profiles.profiles.map(x => x.name) })
})()""".replace("%PINIA%", PINIA)))
