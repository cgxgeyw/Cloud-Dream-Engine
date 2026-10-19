# One-shot: strip self-managed system avoidance from all world packages.
# Framework contract now owns top/bottom safe area (native insets) and keyboard yield.
# Worlds keep fixed content paddings; layout roots use height:100%.
import io
import json
import re
import sys

FILES_CSS = [
    "examples/world-packages/healthy-life/world/ui.mobile.css",
    "examples/world-packages/mortal-cultivation-journey/world/ui.mobile.css",
    "examples/world-packages/stock-analyst/world/ui.mobile.css",
    "examples/world-packages/stock-council/world/ui.mobile.css",
    "examples/world-packages/narrative-rpg-starter/world/ui.mobile.css",
    "examples/world-packages/accounting-assistant/world/ui.mobile.css",
    "examples/world-packages/accounting-assistant/world/ui.desktop.css",
]
FILES_JSONC = [
    "src-tauri/src/db/seeds/assets/default-mobile-ui.jsonc",
    "src-tauri/src/db/seeds/assets/poetry-mobile-ui.jsonc",
    "src-tauri/src/db/seeds/assets/schedule-assistant-mobile-ui.jsonc",
    "src-tauri/src/db/seeds/assets/gwtw-mobile-ui.jsonc",
    "examples/world-packages/mortal-cultivation-journey/world/ui.mobile.jsonc",
    "examples/world-packages/stock-analyst/world/ui.mobile.jsonc",
    "examples/world-packages/stock-council/world/ui.mobile.jsonc",
    "examples/world-packages/narrative-rpg-starter/world/ui.mobile.jsonc",
]

fail = []

def sub(text, pattern, repl, label, expect_min=0):
    new, n = re.subn(pattern, repl, text)
    print(f"    {label}: {n}")
    return new

def root_pad_removals(text, file):
    """Root-level padding-top/height declarations must be removed (host owns them),
    not converted to fixed values. Targeted exact-string ops per file."""
    ops = [
        # default (one-liner)
        ("src-tauri/src/db/seeds/assets/default-mobile-ui.jsonc",
         "overflow: hidden; padding-top: max(env(safe-area-inset-top, 0px), 20px); box-sizing: border-box;",
         "overflow: hidden; box-sizing: border-box;"),
        # poetry (one-liner)
        ("src-tauri/src/db/seeds/assets/poetry-mobile-ui.jsonc",
         "height: var(--game-visual-viewport-height, 100dvh); min-height: var(--game-visual-viewport-height, 100dvh); overflow: hidden; padding-top: 0; box-sizing: border-box;",
         "overflow: hidden; box-sizing: border-box;"),
        # schedule (multiline, escaped \n)
        ("src-tauri/src/db/seeds/assets/schedule-assistant-mobile-ui.jsonc",
         "min-height: var(--game-visual-viewport-height, 100dvh);\\n  padding-top: max(env(safe-area-inset-top, 0px), 12px);\\n  box-sizing: border-box;",
         "box-sizing: border-box;"),
        # gwtw: two root rules + explicit zero
        ("src-tauri/src/db/seeds/assets/gwtw-mobile-ui.jsonc",
         "height: var(--game-visual-viewport-height, 100dvh);\\n  min-height: var(--game-visual-viewport-height, 100dvh);",
         ""),
        ("src-tauri/src/db/seeds/assets/gwtw-mobile-ui.jsonc",
         "overflow: hidden;\\n  padding-top: 0;\\n  box-sizing: border-box;",
         "overflow: hidden;\\n  box-sizing: border-box;"),
    ]
    for path, find, repl in ops:
        if path != file:
            continue
        n = text.count(find)
        if n == 1:
            text = text.replace(find, repl)
            print("    root op ok")
        elif n == 0:
            print("    root op already applied")
        else:
            fail.append(f"{file}: root op count={n} for: {find[:60]}")
    return text

print("== seeds (custom_css strings) ==")
for path in FILES_JSONC:
    text = io.open(path, encoding="utf-8").read()
    print(f"  {path}")
    text = root_pad_removals(text, path)
    # jsonc inline style: layout root height -> 100%
    text = sub(text, r'"height": "var\(--game-visual-viewport-height, 100dvh\)"', '"height": "100%"', "jsonc root height")
    # generic safe-area conversions inside custom_css strings
    text = sub(text, r"max\(calc\(env\(safe-area-inset-top, 0px\) \+ (\d+)px\), (\d+)px\)", r"\2px", "max(calc(top+N),M)")
    text = sub(text, r"max\(env\(safe-area-inset-(top|right|bottom|left), 0px\), (\d+)px\)", r"\2px", "max(env(side),N)")
    text = sub(text, r"calc\(env\(safe-area-inset-(top|right|bottom|left), 0px\) \+ (\d+)px\)", r"\2px", "calc(env(side)+N)")
    text = sub(text, r"calc\((\d+)px \+ env\(safe-area-inset-[a-z]+, 0px\)\)", r"\1px", "calc(N+env(side))")
    text = sub(text, r"(right|left): env\(safe-area-inset-(right|left), 0px\);?", "", "dock decl")
    text = sub(text, r" (min-height|height): var\(--game-visual-viewport-height, 100dvh\);", "", "root height decl")
    text = sub(text, r"padding-bottom: env\(safe-area-inset-bottom, 0px\);?", "", "dock pad")
    io.open(path, "w", encoding="utf-8", newline="").write(text)

print("== example stylesheets ==")
for path in FILES_CSS:
    text = io.open(path, encoding="utf-8").read()
    print(f"  {path}")
    if "healthy-life" in path:
        # parallel session owns the root height rule; keep it, only fix the stale comment value
        text = text.replace(
            "/* 顶部安全区由宿主基线垫在 root 上（安卓为 --world-safe-area-top: 88px），",
            "/* 顶部安全区由宿主基线垫在 root 上（安卓为原生精确状态栏高度），")
    text = sub(text, r"max\(calc\(env\(safe-area-inset-top, 0px\) \+ (\d+)px\), (\d+)px\)", r"\2px", "max(calc(top+N),M)")
    text = sub(text, r"max\(env\(safe-area-inset-(top|right|bottom|left), 0px\), (\d+)px\)", r"\2px", "max(env(side),N)")
    text = sub(text, r"max\((\d+)px, env\(safe-area-inset-[a-z]+, 0px\)\)", r"\1px", "max(N,env(side))")
    text = sub(text, r"calc\(env\(safe-area-inset-(top|right|bottom|left), 0px\) \+ (\d+)px\)", r"\2px", "calc(env(side)+N)")
    text = sub(text, r"calc\((\d+)px \+ env\(safe-area-inset-[a-z]+, 0px\)\)", r"\1px", "calc(N+env(side))")
    text = sub(text, r"max\(30px, var\(--world-safe-area-(right|left), 0px\)\)", "30px", "acc desktop side")
    text = sub(text, r"max\(16px, calc\(var\(--world-safe-area-top, 0px\) \+ 10px\)\)", "16px", "acc top")
    text = sub(text, r"max\(10px, calc\(var\(--world-safe-area-top, 0px\) \+ 6px\)\)", "10px", "acc top small")
    text = sub(text, r"max\(10px, calc\(var\(--world-safe-area-bottom, 0px\) \+ 6px\)\)", "10px", "acc bottom")
    text = sub(text, r"\n\s*padding-bottom: env\(safe-area-inset-bottom, 0px\);", "", "gwtw dock pad")
    text = sub(text, r"(min-height|height): var\(--game-visual-viewport-height, 100dvh\);", r"\1: 100%;", "viewport height -> 100%")
    text = sub(text, r"max\(var\(--world-safe-area-[a-z]+, 0px\), env\(safe-area-inset-[a-z]+, 0px\), 0px\)", "0px", "leftover baseline fmt")
    io.open(path, "w", encoding="utf-8", newline="").write(text)

print("== validate ==")
# jsonc: custom_css strings still parse; css brace balance; zero leftover refs (healthy-life exempt: 3 known)
for path in FILES_JSONC:
    t = io.open(path, encoding="utf-8").read()
    m = re.search(r'"custom_css"\s*:\s*"((?:[^"\\]|\\.)*)"', t, re.S)
    if not m:
        print(f"    {path}: no custom_css, skipped")
        continue
    css = json.loads('"' + m.group(1) + '"')
    d = css.count("{") - css.count("}")
    if d != 0:
        fail.append(f"{path}: brace delta {d}")
    left = re.findall(r"env\(safe-area-inset-|--game-visual-viewport-height|--world-safe-area-", css)
    if left:
        fail.append(f"{path}: {len(left)} leftover refs in custom_css")
for path in FILES_CSS:
    t = io.open(path, encoding="utf-8").read()
    d = t.count("{") - t.count("}")
    if d != 0:
        fail.append(f"{path}: brace delta {d}")
    left = re.findall(r"env\(safe-area-inset-|--game-visual-viewport-height|--world-safe-area-", t)
    limit = 3 if "healthy-life" in path else 0
    if len(left) > limit:
        fail.append(f"{path}: {len(left)} leftover refs (limit {limit})")

if fail:
    print("FAILURES:")
    for f in fail:
        print(" -", f)
    sys.exit(1)
print("all world packages updated and validated")
