# Generic world-package zipper: pack <package-dir> into output/<name>-world.zip.
# Mirrors the app export layout: manifest.json at ZIP root, world/, characters/, assets/, tools.json.
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKIP_PARTS = {"__pycache__", ".DS_Store", "Thumbs.db"}

def pack(name: str) -> None:
    src = ROOT / "examples" / "world-packages" / name
    if not (src / "manifest.json").exists():
        print(f"SKIP {name}: no manifest.json")
        return
    out = ROOT / "output" / f"{name}-world.zip"
    out.parent.mkdir(parents=True, exist_ok=True)
    count = 0
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for path in sorted(src.rglob("*")):
            if path.is_dir() or any(part in SKIP_PARTS for part in path.parts):
                continue
            z.write(path, path.relative_to(src).as_posix())
            count += 1
    print(f"ok {name}: {count} files -> {out.name} ({out.stat().st_size} bytes)")

if __name__ == "__main__":
    names = sys.argv[1:] or [
        "healthy-life",
        "mortal-cultivation-journey",
        "stock-analyst",
        "stock-council",
        "accounting-assistant",
    ]
    for name in names:
        pack(name)
