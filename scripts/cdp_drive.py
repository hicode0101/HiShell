#!/usr/bin/env python3
"""CDP 驱动：在真实运行的 HiShell 里复现连接流程并抓取前端状态"""
import json
import time
import sys

import websocket

WS_URL = sys.argv[1] if len(sys.argv) > 1 else None
if not WS_URL:
    info = json.loads(__import__("urllib.request", fromlist=["urlopen"]).urlopen("http://127.0.0.1:9222/json/list").read())
    WS_URL = info[0]["webSocketDebuggerUrl"]

ws = websocket.create_connection(WS_URL, timeout=15)
mid = 0


def ev(expr):
    """Runtime.evaluate，返回 JSON 结果"""
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


def js_state():
    return ev("""(() => {
      const app = document.querySelector('#app').__vue_app__
      const st = app.config.globalProperties.$pinia.state.value
      const regs = window.__termReg
      const terms = {}
      if (regs) for (const [k, v] of regs) terms[k] = { wired: v.wired }
      return {
        tabs: st.sessions.tabs.map(t => ({ id: t.id, title: t.title, sid: t.sid, status: t.status, viewMode: t.viewMode })),
        prompts: st.sessions.prompts.map(p => ({ sid: p.sid, key: p.key, kind: p.payload && p.payload.kind })),
        pendingStatus: st.sessions.pendingStatus,
        pendingDataKeys: 'n/a',
        terms,
      }
    })()""")


def click_text(text):
    return ev(f"""(() => {{
      const els = [...document.querySelectorAll('button, .tab, .f-combo-item, .f-ctx-item')]
      const el = els.find(e => e.textContent.trim().includes({json.dumps(text)}))
      if (el) {{ el.click(); return 'clicked: ' + el.textContent.trim().slice(0, 30) }}
      return 'NOT FOUND: ' + {json.dumps(text)}
    }})()""")


def type_in_password(pwd):
    return ev(f"""(() => {{
      const input = document.querySelector('.f-dialog .f-textbox-input[type=password]')
      if (!input) return 'no password input'
      const proto = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')
      proto.set.call(input, {json.dumps(pwd)})
      input.dispatchEvent(new Event('input', {{ bubbles: true }}))
      return 'typed'
    }})()""")


print("=== 1. 初始状态 ===")
print(json.dumps(js_state(), ensure_ascii=False, indent=1))

print("=== 2. 触发快速连接 192.168.209.130 ===")
print(ev("""(() => {
  window.dispatchEvent(new CustomEvent('hishell-command', { detail: JSON.stringify({
    username: 'root', host: '192.168.209.130', port: 22
  }) }))
  // quick-connect 前缀在 MainView 拼接
  return 'dispatched'
})()"""))

print("=== 3. 若未创建（命令格式不对），直接调 store ===")
st = js_state()
if not st or (isinstance(st, dict) and not st.get("tabs")) or all(t["title"] != "192.168.209.130" for t in (st.get("tabs") or [])):
    ev("""(() => {
      const app = document.querySelector('#app').__vue_app__
      const pinia = app.config.globalProperties.$pinia
      const profiles = pinia._s.get('profiles')
      const sessions = pinia._s.get('sessions')
      const p = profiles.profiles.find(x => x.host === '192.168.209.130')
        || profiles.profiles[0]
      if (!p) { window.__diag = 'no profile'; return }
      sessions.createSession(JSON.parse(JSON.stringify(p)))
      window.__diag = 'created session for ' + p.host
    })()""")
    print("store-created:", ev("window.__diag"))

for i in range(12):
    time.sleep(1)
    st = js_state()
    tabs = st.get("tabs", []) if isinstance(st, dict) else []
    cur = next((t for t in tabs if t.get("title") == "192.168.209.130"), None)
    prompts = st.get("prompts", []) if isinstance(st, dict) else []
    print(f"[{i+1}s] status={cur and cur['status']} sid={cur and cur['sid']} prompts={[(p['kind'], p['key'][:20]) for p in prompts]}")

    # 有 hostkey 提示 → 点“接受并保存”
    if prompts and prompts[0].get("kind") == "hostkeyNew":
        print("  -> 接受并保存 hostkey")
        print("   ", click_text("接受并保存"))
        time.sleep(1)
        continue
    if prompts and prompts[0].get("kind") == "password":
        print("  -> 输入密码并确认")
        print("   ", type_in_password("Wxhq@123456"))
        time.sleep(0.3)
        print("   ", click_text("确认"))
        time.sleep(1)
        continue

    if cur and cur.get("status") == "connected":
        print("=== 4. 已连接，检查终端内容 ===")
        out = ev("""(() => {
          const regs = window.__termReg
          if (!regs) return 'no reg'
          const out = []
          for (const [id, v] of regs) {
            let text = ''
            try {
              const b = v.term.buffer.active
              for (let y = 0; y < Math.min(b.length, 12); y++) {
                const line = b.getLine(y)
                text += (line ? line.translateToString(true) : '') + '\\n'
              }
            } catch (e) { text = 'ERR ' + e.message }
            out.push({ id, wired: v.wired, cols: v.term.cols, rows: v.term.rows, text: text.slice(0, 400) })
          }
          return out
        })()""")
        print(json.dumps(out, ensure_ascii=False, indent=1))
        break

print("=== done ===")
