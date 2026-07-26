#!/bin/bash
# 有效代码行门禁：单文件非空非注释行 > 350 即失败（行数膨胀 = 该拆了，见仓库拆分惯例）。
# 覆盖 src-tauri/src（.rs）与 src（.ts/.tsx），测试文件同规。
set -e
cd "$(dirname "$0")/.."

python3 - <<'EOF'
import os, sys

LIMIT = 350
ROOTS = (("src-tauri/src", (".rs",)), ("src", (".ts", ".tsx")))

def eff_lines(path):
    n = 0
    in_block = False
    with open(path, encoding="utf-8", errors="ignore") as f:
        for line in f:
            s = line.strip()
            if in_block:
                if "*/" in s:
                    in_block = False
                    s = s.split("*/", 1)[1].strip()
                    if not s or s.startswith("//"):
                        continue
                else:
                    continue
            if not s or s.startswith("//"):
                continue
            if s.startswith("/*"):
                if "*/" not in s:
                    in_block = True
                continue
            n += 1
    return n

violations = []
for root, exts in ROOTS:
    for dp, _, fns in os.walk(root):
        for fn in fns:
            if fn.endswith(exts):
                p = os.path.join(dp, fn)
                n = eff_lines(p)
                if n > LIMIT:
                    violations.append((n, p))

if violations:
    violations.sort(reverse=True)
    print(f"effective-line limit {LIMIT} exceeded:")
    for n, p in violations:
        print(f"  {n:5d}  {p}")
    sys.exit(1)
print(f"effective-line check OK (limit {LIMIT})")
EOF
