#!/usr/bin/env python3
"""E2E：userKey 认证（安装探针公钥到服务器 → ssh_test 验证 userKey 链路）"""
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

print("== 3. 用密码会话把公钥装到服务器 ==")
install = ev("""(async () => {
  try {
    const profiles = %PINIA%._s.get('profiles')
    await profiles.init()
    const p = profiles.profiles.find(x => x.host === '149.104.22.80')
    if (!p) return 'no server profile'
    const sessions = %PINIA%._s.get('sessions')
    const tab = sessions.createSession(JSON.parse(JSON.stringify(p)))
    for (let i = 0; i < 40; i++) {
      await new Promise(r => setTimeout(r, 500))
      if (tab.status === 'connected' && tab.sid !== undefined) break
    }
    if (tab.status !== 'connected') return 'connect failed: ' + tab.status
    const ks = await window.__TAURI_INTERNALS__.invoke('user_key_list')
    const k = ks.find(x => x.name === 'server_probe')
    const pubText = (await window.__TAURI_INTERNALS__.invoke('user_key_public_content', { publicPath: k.publicPath })).trim()
    const base = 'mkdir -p ~/.ssh; grep -qF "' + pubText + '" ~/.ssh/authorized_keys 2>/dev/null || echo ' + pubText + ' >> ~/.ssh/authorized_keys; chmod 700 ~/.ssh; chmod 600 ~/.ssh/authorized_keys; echo INSTALLED'
    const bytes = new TextEncoder().encode(base + '\\r')
    let bin = ''
    bytes.forEach(b => bin += String.fromCharCode(b))
    const b64 = btoa(bin)
    await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: b64 })
    await new Promise(r => setTimeout(r, 1500))
    return 'installed, sid=' + tab.sid
  } catch (e) { return 'ERR: ' + String(e) }
})()""".replace("%PINIA%", PINIA))
print(install)

print("== 4. ssh_test 验证 userKey 认证链路 ==")
print(ev("""(async () => {
  try {
    const profiles = %PINIA%._s.get('profiles')
    const p = profiles.profiles.find(x => x.host === '149.104.22.80')
    const test = JSON.parse(JSON.stringify(p))
    test.auth = 'userKey'
    test.userKeyName = 'server_probe'
    test.privateKeyPath = ''
    test.id = 'test-userkey-' + Date.now()
    const r = await window.__TAURI_INTERNALS__.invoke('ssh_test', { profile: test, password: null, passphrase: null })
    return JSON.stringify(r)
  } catch (e) { return 'ERR: ' + String(e) }
})()""".replace("%PINIA%", PINIA)))

print("== 5. 清理服务器上的探针公钥 ==")
print(ev("""(async () => {
  try {
    const sessions = %PINIA%._s.get('sessions')
    const tab = sessions.tabs.find(t => t.status === 'connected' && t.sid !== undefined)
    if (!tab) return 'no session (skip, will clean manually)'
    const bytes = new TextEncoder().encode('sed -i "/userkey-e2e/d" ~/.ssh/authorized_keys; echo CLEANED\\r')
    let bin = ''
    bytes.forEach(b => bin += String.fromCharCode(b))
    const b64 = btoa(bin)
    await window.__TAURI_INTERNALS__.invoke('ssh_write', { sid: tab.sid, data: b64 })
    await new Promise(r => setTimeout(r, 1200))
    sessions.disconnectById(tab.id)
    return 'cleaned'
  } catch (e) { return 'ERR: ' + String(e) }
})()""".replace("%PINIA%", PINIA)))
