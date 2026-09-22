#!/usr/bin/env python3
"""E2E：主机分派式校验——通过新建连接对话框逐值验证 errs.host"""
import json, time, urllib.request, websocket

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
                ex = res["exceptionDetails"].get("exception", {})
                return {"__error__": (ex.get("description") or ex.get("value") or "?")[:200]}
            return res.get("result", {}).get("value")

PINIA = "document.querySelector('#app').__vue_app__.config.globalProperties.$pinia"

print(ev(f"""(async () => {{
  const pinia = {PINIA}
  await pinia._s.get('profiles').init()
  const sessions = pinia._s.get('sessions')
  sessions.openConnectDialog()
  await new Promise(r => setTimeout(r, 700))
  return 'ready'
}})()"""))

cases = [
    ['149.104.22.80', True],      # IPv4 合法
    ['256.1.1.1', False],         # IPv4 越界
    ['1.2.3.4.5', False],         # 5 段（WHATWG 拒绝）
    ['::1', True],                # IPv6
    ['::ffff:192.168.1.1', True], # IPv6 混合
    ['gg::12', False],            # 非 hex
    ['[::1]', True],              # 方括号 IPv6
    ['example.com', True],        # 域名
    ['中文.example', True],        # 中文域名（IDNA）
    ['-bad.com', False],          # 连字符开头（标签规则）
    ['db1', True],                # 内网短主机名
    ['bad host', False],          # 空格
    ['', False],                  # 空
]

js = """(async () => {
  const dlg = [...document.querySelectorAll('.f-dialog')].find(d => d.textContent.includes('认证'))
  if (!dlg) return 'no dialog'
  const host = [...dlg.querySelectorAll('input')][1]
  const setv = (v) => { host.focus(); host.value = v; host.dispatchEvent(new Event('input', { bubbles: true })) }
  const out = []
  const cases = %CASES%
  for (const [val, expectOk] of cases) {
    setv(val)
    await new Promise(r => setTimeout(r, 120))
    // 触发一次校验展示（点亮 showErr）：点一下保存
    const save = [...dlg.querySelectorAll('button')].find(b => b.textContent.trim() === '保存')
    save.click()
    await new Promise(r => setTimeout(r, 250))
    const invalid = !!dlg.querySelector('.cd-invalid')
    const stillOpen = !!dlg.offsetParent
    out.push({ val, expectOk, pass: invalid === !expectOk, blocked: stillOpen === !expectOk })
    // 重置 showErr：重新触发一次 input（无影响），仅恢复状态
  }
  // 关闭对话框（取消）
  const cancel = [...dlg.querySelectorAll('button')].find(b => b.textContent.trim() === '取消')
  cancel.click()
  await new Promise(r => setTimeout(r, 300))
  return JSON.stringify(out)
})()""".replace("%CASES%", json.dumps(cases))

print(ev(js))
