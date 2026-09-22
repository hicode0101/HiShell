#!/usr/bin/env python3
"""E2E：密钥导入/导出（后端直调 + UI 列表刷新）"""
import json, time, websocket, urllib.request

TMP = "C:/Users/QWERT/AppData/Local/Temp/keyimp"
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
                return {"__error__": ex.get("description") or ex.get("value") or "?"}
            return res.get("result", {}).get("value")

def call(cmd, args):
    return ev(f"""(async () => {{
      try {{
        const r = await window.__TAURI_INTERNALS__.invoke('{cmd}', {json.dumps(args)})
        return JSON.stringify(r)
      }} catch (e) {{ return 'ERR: ' + String(e) }}
    }})()""")

print("== 1. inspect：明文 ed25519 ==")
print(call('user_key_inspect', {'sourcePath': f'{TMP}/imp_plain'}))
print("== 2. 导入明文密钥（无口令）==")
print(call('user_key_import', {'req': {'sourcePath': f'{TMP}/imp_plain', 'name': 'imported_plain', 'passphrase': ''}}))
print("== 3. inspect：加密 rsa ==")
print(call('user_key_inspect', {'sourcePath': f'{TMP}/imp_enc'}))
print("== 4. 导入加密密钥不给口令 → 应报错 ==")
print(call('user_key_import', {'req': {'sourcePath': f'{TMP}/imp_enc', 'name': 'imported_enc', 'passphrase': ''}}))
print("== 5. 错误口令 → 应报错 ==")
print(call('user_key_import', {'req': {'sourcePath': f'{TMP}/imp_enc', 'name': 'imported_enc', 'passphrase': 'wrong'}}))
print("== 6. 正确口令 → 导入成功 ==")
print(call('user_key_import', {'req': {'sourcePath': f'{TMP}/imp_enc', 'name': 'imported_enc', 'passphrase': 'Src123!'}}))
print("== 7. PEM 老格式 → 应明确报不支持 ==")
print(call('user_key_import', {'req': {'sourcePath': f'{TMP}/imp_legacy', 'name': 'imported_legacy', 'passphrase': ''}}))
print("== 8. 重名导入 → 应报错 ==")
print(call('user_key_import', {'req': {'sourcePath': f'{TMP}/imp_plain', 'name': 'imported_plain', 'passphrase': ''}}))

print("== 9. 导出私钥：明文 → 加密 (Exp123!) ==")
print(call('user_key_export_private_to', {'req': {
    'privatePath': f'{TMP}/exp_plain',
    'currentPassphrase': '', 'newPassphrase': 'Exp123!',
    'destPath': f'{TMP}/exp_plain_enc'}}) if False else call('user_key_export_private_to', {'req': {
    'privatePath': 'C:/Users/QWERT/.ssh/imported_plain',
    'currentPassphrase': '', 'newPassphrase': 'Exp123!',
    'destPath': f'{TMP}/exp_plain_enc'}}))
print("== 10. 导出私钥：错误当前口令 → 应报错 ==")
print(call('user_key_export_private_to', {'req': {
    'privatePath': 'C:/Users/QWERT/.ssh/imported_enc',
    'currentPassphrase': 'nope', 'newPassphrase': '', 'destPath': f'{TMP}/exp_enc_plain'}}))
print("== 11. 导出私钥：正确口令 + 去掉加密 ==")
print(call('user_key_export_private_to', {'req': {
    'privatePath': 'C:/Users/QWERT/.ssh/imported_enc',
    'currentPassphrase': 'Src123!', 'newPassphrase': '', 'destPath': f'{TMP}/exp_enc_plain'}}))
print("== 12. 公钥内容 + IETF 转换写盘 ==")
print(ev(f"""(async () => {{
  try {{
    const line = await window.__TAURI_INTERNALS__.invoke('user_key_public_content', {{ publicPath: 'C:/Users/QWERT/.ssh/imported_plain.pub' }})
    await window.__TAURI_INTERNALS__.invoke('user_key_write_text', {{ path: '{TMP}/exp_ietf.pub', content: '---- BEGIN SSH2 PUBLIC KEY ----\\nComment: "ietf-test"\\n' + line.split(/\\s+/)[1].replace(/(.{{72}})/g, '$1\\n') + '\\n---- END SSH2 PUBLIC KEY ----\\n' }})
    return 'ok: ' + line.slice(0, 20)
  }} catch (e) {{ return 'ERR: ' + String(e) }}
}})()"""))

print("== 13. UI 列表应出现两个导入的密钥 ==")
print(ev("""(async () => {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: 'user-keys' }))
  await new Promise(r => setTimeout(r, 800))
  return JSON.stringify([...document.querySelectorAll('.uk-row .col-name')].map(e => e.textContent.trim()))
})()"""))
