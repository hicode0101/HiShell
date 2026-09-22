#!/usr/bin/env python3
"""E2E：ECDSA 384 走 UI 向导；ECDSA 521 / RSA 8192 走后端直调并计时"""
import json, time, websocket, urllib.request

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
                return {"__error__": res["exceptionDetails"].get("exception", {}).get("description", "?")[:300]}
            return res.get("result", {}).get("value")

def click_text(text):
    return ev(f"""(async () => {{
      const wizEl = document.querySelector('.wiz')
      const root = wizEl ? wizEl.closest('.f-dialog') : document
      const els = [...root.querySelectorAll('button, .f-combo-item, .menu-item')]
      const el = els.find(e => e.textContent.trim().includes('{text}'))
      if (!el) return 'not found: {text}'
      el.click()
      await new Promise(r => setTimeout(r, 350))
      return 'clicked'
    }})()""")

def combo_pick(face_text, item_text):
    return ev(f"""(async () => {{
      const wiz = document.querySelector('.wiz')
      const faces = [...wiz.querySelectorAll('.f-combo-face')]
      const face = faces.find(f => f.textContent.includes('{face_text}'))
      if (!face) return 'combo not found: {face_text}'
      face.click()
      await new Promise(r => setTimeout(r, 250))
      const items = [...document.querySelectorAll('.f-combo-popup .f-combo-item')]
      const item = items.find(i => i.textContent.trim() === '{item_text}')
      if (!item) return 'item not found: {item_text}'
      item.click()
      await new Promise(r => setTimeout(r, 250))
      return 'picked'
    }})()""")

print("== 1. ECDSA 384 完整 UI 向导 ==")
print(ev("""(async () => {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: 'user-keys' }))
  await new Promise(r => setTimeout(r, 500))
  const btn = [...document.querySelectorAll('.uk-ops button')].find(b => b.textContent.includes('生成'))
  btn.click()
  await new Promise(r => setTimeout(r, 500))
  return document.querySelector('.wiz') ? 'wizard open' : 'fail'
})()"""))
print(combo_pick("RSA", "ECDSA"))
print(combo_pick("256", "384"))
print(click_text("下一步"))
for i in range(40):
    done = ev("""(() => document.querySelector('.wiz')?.textContent.includes('公钥对已成功生成') || false)()""")
    if done: break
    time.sleep(0.5)
print("genDone:", done)
print(click_text("下一步"))
time.sleep(0.3)
print(click_text("下一步"))
time.sleep(0.3)
print(ev("""(() => {
  const v = document.querySelector('.wiz .pub-area')?.value || ''
  return JSON.stringify({ isEcdsa384: v.startsWith('ecdsa-sha2-nistp384 ') })
})()"""))
print(click_text("完成"))
time.sleep(1.2)
print(ev("""(() => JSON.stringify([...document.querySelectorAll('.uk-row .col-name')].map(e => e.textContent.trim())))()"""))

print("== 2. ECDSA 521 后端直调 ==")
print(ev("""(async () => {
  const gid = crypto.randomUUID()
  const info = await window.__TAURI_INTERNALS__.invoke('user_key_generate_start', { genId: gid, keyType: 'ECDSA', bits: 521 })
  const paths = await window.__TAURI_INTERNALS__.invoke('user_key_generate_finish', { req: { genId: gid, name: 'id_ecdsa_521', comment: 'ecdsa 521-test', passphrase: '' } })
  return JSON.stringify({ blob: info.publicBlob.slice(0, 22), paths })
})()"""))

print("== 3. RSA 8192 后端直调（计时）==")
print(ev("""(async () => {
  const gid = crypto.randomUUID()
  const t0 = Date.now()
  try {
    const info = await window.__TAURI_INTERNALS__.invoke('user_key_generate_start', { genId: gid, keyType: 'RSA', bits: 8192 })
    const genMs = Date.now() - t0
    const paths = await window.__TAURI_INTERNALS__.invoke('user_key_generate_finish', { req: { genId: gid, name: 'id_rsa_8192', comment: 'rsa 8192-test', passphrase: '' } })
    return JSON.stringify({ blob: info.publicBlob.slice(0, 9), genSeconds: (genMs / 1000).toFixed(1), written: !!paths.privateKeyPath })
  } catch (e) { return 'ERR: ' + String(e) }
})()"""))

print("== 4. 最终列表 ==")
print(ev("""(async () => {
  await new Promise(r => setTimeout(r, 400))
  return JSON.stringify([...document.querySelectorAll('.uk-row .col-name')].map(e => e.textContent.trim()))
})()"""))
