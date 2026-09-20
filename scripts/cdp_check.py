#!/usr/bin/env python3
"""检查当前所有会话状态与终端缓冲内容"""
import json, sys, time
import websocket

info = json.loads(__import__("urllib.request", fromlist=["urlopen"]).urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=15)
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

out = ev("""(() => {
  const app = document.querySelector('#app').__vue_app__
  const st = app.config.globalProperties.$pinia.state.value
  const regs = window.__termReg || new Map()
  const terms = []
  for (const [id, v] of regs) {
    let text = ''
    try {
      const b = v.term.buffer.active
      for (let y = 0; y < Math.min(b.length, 15); y++) {
        const line = b.getLine(y)
        text += (line ? line.translateToString(true) : '') + '\\n'
      }
    } catch (e) { text = 'ERR ' + e.message }
    terms.push({ id, wired: v.wired, cols: v.term.cols, rows: v.term.rows, text: text.slice(0, 500) })
  }
  return {
    tabs: st.sessions.tabs.map(t => ({ id: t.id, title: t.title, sid: t.sid, status: t.status, statusMessage: t.statusMessage, viewMode: t.viewMode })),
    prompts: st.sessions.prompts.map(p => ({ sid: p.sid, kind: p.payload && p.payload.kind })),
    activeTabId: st.sessions.activeTabId,
    terms,
  }
})()""")
print(json.dumps(out, ensure_ascii=False, indent=1))
