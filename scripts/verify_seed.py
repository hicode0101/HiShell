#!/usr/bin/env python3
"""种子配置验证（数据安全优先）：
 1. 备份用户真实 profiles.json
 2. 移除后启动 exe → 首次运行应生成 3 条无害示例配置
 3. 还原用户真实 profiles.json（try/finally 保证必还原）
"""
import json
import os
import shutil
import subprocess
import time

CFG = os.path.join(os.environ["APPDATA"], "hishell", "profiles.json")
BAK = CFG + ".seed-bak"
EXE = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "target", "release", "HiShell.exe")
ENV = dict(os.environ, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222 --remote-allow-origins=*")

report = {}
try:
    # 1) 备份并移除用户配置
    shutil.copy2(CFG, BAK)
    before = json.load(open(BAK, encoding="utf-8"))
    report["backupCount"] = len(before)
    os.remove(CFG)

    # 2) 启动 exe，等待首次运行生成种子文件
    proc = subprocess.Popen([EXE], env=ENV, cwd=os.path.dirname(EXE))
    for _ in range(30):
        time.sleep(0.5)
        if os.path.exists(CFG):
            break
    time.sleep(1.5)  # 等文件写完

    seeded = json.load(open(CFG, encoding="utf-8"))
    report["seedCount"] = len(seeded)
    report["seeds"] = [
        {"name": p.get("name"), "host": p.get("host"), "user": p.get("username"),
         "auth": p.get("auth"), "temporary": p.get("temporary", False)}
        for p in seeded
    ]
    report["noRealHosts"] = not any(
        h in json.dumps(seeded) for h in ("149.104.22.80", "top.hicode.top", "hicode", "yt - 10808")
    )
finally:
    # 3) 无论成败：杀掉应用并还原用户配置
    subprocess.run(["taskkill", "/F", "/IM", "HiShell.exe"], capture_output=True)
    time.sleep(1)
    if os.path.exists(BAK):
        shutil.copy2(BAK, CFG)
        os.remove(BAK)
    restored = json.load(open(CFG, encoding="utf-8"))
    report["restoredCount"] = len(restored)
    report["restoredOk"] = restored == before

print(json.dumps(report, ensure_ascii=False, indent=1))
ok = (report.get("backupCount") == report.get("restoredCount") == report.get("restoredOk", False)
      and report.get("seedCount") == 3 and report.get("noRealHosts")
      and all(not p["temporary"] for p in report.get("seeds", [])))
print("ALL PASS" if ok else "SOME FAILED")
