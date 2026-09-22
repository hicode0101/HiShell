#!/usr/bin/env python3
"""E2E：用户密钥生成向导（ED25519 无口令 + RSA2048 带口令）+ 保存公钥文件"""
import json, time, websocket, urllib.request

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=90)
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

def click_text(text, scope=".wiz"):
    return ev(f"""(async () => {{
      const wizEl = document.querySelector('.wiz')
      const root = wizEl ? wizEl.closest('.f-dialog') : document
      const els = [...root.querySelectorAll('button, .f-combo-item, .menu-item')]
      const el = els.find(e => e.textContent.trim().includes('{text}'))
      if (!el) return 'not found: {text}'
      el.click()
      await new Promise(r => setTimeout(r, 350))
      return 'clicked: {text}'
    }})()""")

def combo_pick(face_text, item_text, step_scope=".wiz"):
    return ev(f"""(async () => {{
      const wiz = document.querySelector('{step_scope}')
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
      return 'picked: {item_text}'
    }})()""")

def set_input(index, value, scope=".wiz"):
    return ev(f"""(async () => {{
      const wiz = document.querySelector('.wiz')
      const inputs = [...wiz.querySelectorAll('input')]
      const inp = inputs[{index}]
      if (!inp) return 'no input ' + {index}
      inp.focus()
      inp.value = '{value}'
      inp.dispatchEvent(new Event('input', {{ bubbles: true }}))
      await new Promise(r => setTimeout(r, 150))
      return 'set input ' + {index}
    }})()""")

def step_text():
    return ev("""(() => {
      const t = document.querySelector('.wiz-step-title')
      return t ? t.textContent.trim() : 'no wizard'
    })()""")

print("== 0. 打开用户密钥管理 ==")
print(ev("""(async () => {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: 'user-keys' }))
  await new Promise(r => setTimeout(r, 600))
  return document.querySelector('.uk-body') ? 'dialog open' : 'fail'
})()"""))

print("== 1. 打开生成向导（ED25519）==")
print(click_text("生成…"))
print("step:", step_text())
print(combo_pick("RSA", "ED25519"))
print(click_text("下一步"))

print("== 2. 等待生成完成（ED25519 应瞬时）==")
for i in range(40):
    done = ev("""(() => document.querySelector('.wiz')?.textContent.includes('公钥对已成功生成') || false)()""")
    if done: break
    time.sleep(0.5)
print("genDone:", done)

print("== 3. 第 3 步默认名称 id_ed25519_256 ==")
print(click_text("下一步"))
time.sleep(0.4)
print(ev("""(() => {
  const wiz = document.querySelector('.wiz')
  const name = wiz.querySelectorAll('input')[0]?.value
  const comment = wiz.querySelectorAll('input')[1]?.value
  return JSON.stringify({ name, comment })
})()"""))

print("== 4. 第 4 步公钥格式（IETF SECSH 转换）==")
print(click_text("下一步"))
time.sleep(0.4)
print(ev("""(() => {
  const ta = document.querySelector('.wiz .pub-area')
  const v = ta?.value || ''
  return JSON.stringify({ starts: v.slice(0, 30), isIetf: v.includes('---- BEGIN SSH2 PUBLIC KEY ----') })
})()"""))
print(combo_pick("SECSH", "SSH2 - OpenSSH"))
print(ev("""(() => {
  const ta = document.querySelector('.wiz .pub-area')
  const v = ta?.value || ''
  return JSON.stringify({ starts: v.slice(0, 26), oneLine: !v.includes('\\n\\n') })
})()"""))

print("== 5. 完成 → 列表应出现 id_ed25519_256 ==")
print(click_text("完成"))
time.sleep(1.2)
print(ev("""(() => {
  const rows = [...document.querySelectorAll('.uk-row .col-name')].map(e => e.textContent.trim())
  return JSON.stringify(rows)
})()"""))

print("== 6. RSA 2048 + 口令向导 ==")
print(click_text("生成…"))
time.sleep(0.5)
print(combo_pick("ED25519", "RSA"))
print(ev("""(() => {
  const wiz = document.querySelector('.wiz')
  const bitsFace = wiz.querySelectorAll('.f-combo-face')[1]
  return bitsFace ? bitsFace.textContent.trim() : 'no bits combo'
})()"""))
print(click_text("下一步"))
for i in range(120):
    done = ev("""(() => document.querySelector('.wiz')?.textContent.includes('公钥对已成功生成') || false)()""")
    if done: break
    time.sleep(0.5)
print("genDone:", done)
print(click_text("下一步"))
time.sleep(0.4)
print(set_input(2, "Test123!"))
print(set_input(3, "Test123!"))
print(click_text("下一步"))
time.sleep(0.4)
print(ev("""(() => {
  const v = document.querySelector('.wiz .pub-area')?.value || ''
  return JSON.stringify({ isRsa: v.startsWith('ssh-rsa '), hasComment: v.includes('rsa 2048-') })
})()"""))
print(click_text("完成"))
time.sleep(1.5)
print(ev("""(() => {
  const rows = [...document.querySelectorAll('.uk-row .col-name')].map(e => e.textContent.trim())
  return JSON.stringify(rows)
})()"""))

print("== 7. 保存公钥为文件（写下载目录）==")
print(ev("""(async () => {
  const p = await window.__TAURI_INTERNALS__.invoke('user_key_save_public', { name: 'id_ed25519_256', content: 'ssh-ed25519 AAAA test-line' })
  return p
})()"""))

print("== 8. 向导打开时取消（Esc 等价于点取消）→ 无残留 ==")
print(click_text("生成…"))
time.sleep(0.4)
print(combo_pick("RSA", "ECDSA"))
print(click_text("下一步"))
time.sleep(0.8)
print(click_text("取消"))
time.sleep(0.5)
print(ev("""(() => {
  const btns = [...document.querySelectorAll('.uk-ops button')]
  return btns.length ? 'back to key manager' : '??'
})()"""))
