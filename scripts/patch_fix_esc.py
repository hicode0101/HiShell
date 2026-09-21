# -*- coding: utf-8 -*-
import io

path = r"D:\AiWork\work-2\HiShell\src\components\terminal\TerminalPane.vue"
src = io.open(path, encoding="utf-8").read()

BAD = "entry?.term.write(`\\r\\n${\\x1b}[${color}m[HiShell] ${text}${\\x1b}[0m\\r\\n\\r\\n`)"
GOOD = "entry?.term.write(`\\r\\n\\x1b[${color}m[HiShell] ${text}\\x1b[0m\\r\\n\\r\\n`)"

if BAD in src:
    src = src.replace(BAD, GOOD)
    io.open(path, "w", encoding="utf-8", newline="\n").write(src)
    print("fixed")
elif GOOD in src:
    print("already fixed")
else:
    print("PATTERN NOT FOUND")
    # 输出当前行帮助定位
    for i, line in enumerate(src.splitlines(), 1):
        if "term.write" in line and "HiShell" in line:
            print(i, repr(line))
