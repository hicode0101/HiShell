#!/usr/bin/env python3
"""调试：查看当前应用 DOM 状态"""
import base64
import json
import os
import urllib.request

import websocket

info = json.loads(urllib.request.urlopen("http://127.0.0.1:9222/json/list").read())
ws = websocket.create_connection(info[0]["webSocketDebuggerUrl"], timeout=60)
mid = 0


def ev(expr):
    global mid
    mid += 1
    ws.send(json.dumps({"id": mid, "method": "Runtime.evaluate", "params": {
        "expression": expr, "returnByValue": True, "awaitPromise": True}}))
    while True:
        m = json.loads(ws.recv())
        if m.get("id") == mid:
            r = m.get("result", {})
            if "exceptionDetails" in r:
                return "ERR: " + str(r["exceptionDetails"].get("exception", {}).get("description", "?"))
            return r.get("result", {}).get("value")


def shot(name):
    global mid
    mid += 1
    ws.send(json.dumps({"id": mid, "method": "Page.captureScreenshot", "params": {"format": "png"}}))
    while True:
        m = json.loads(ws.recv())
        if m.get("id") == mid:
            open(os.path.join(os.path.dirname(__file__), name), "wb").write(
                base64.b64decode(m["result"]["data"]))
            return name


print("tabs:", ev("document.querySelectorAll('.tab').length"))
print("overlays:", ev("document.querySelectorAll('.f-dialog-overlay').length"))
print("addr value:", ev("document.querySelector('.addr-input')?.value"))
print("tb titles:", ev("[...document.querySelectorAll('.tb-btn')].map(b=>b.title).join('|')"))
print("st titles:", ev("[...document.querySelectorAll('.st-btn')].map(b=>b.title).join('|')"))
print("shot:", shot("cdp_debug_state.png"))
