import zipfile
from pathlib import Path

src = Path("examples/world-packages/healthy-life")
out = Path("output/healthy-life-world.zip")
out.parent.mkdir(parents=True, exist_ok=True)

files = [
    "manifest.json",
    "world/world.json",
    "world/ui.desktop.html",
    "world/ui.desktop.css",
    "world/ui.mobile.html",
    "world/ui.mobile.css",
    "world/logic.js",
    "characters/health-coach/character.json",
    "README.md",
]

with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for rel in files:
        p = src / rel
        if not p.exists():
            print("MISSING", rel)
            continue
        z.write(p, rel)
        print("ok", rel, p.stat().st_size)
print("written", out, out.stat().st_size)

bundled = Path("src-tauri/src/db/seeds/assets/healthy-life-world.zip")
bundled.write_bytes(out.read_bytes())
print("written", bundled, bundled.stat().st_size)
