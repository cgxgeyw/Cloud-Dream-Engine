"""Build and pack the HTML mortal-cultivation world using manifest-declared files."""
import json
from pathlib import Path
import subprocess
import zipfile

root = Path(__file__).resolve().parent.parent
subprocess.run(["node", str(root / "scripts/build-mortal-world-ui.mjs")], cwd=root, check=True)
source = root / "examples/world-packages/mortal-cultivation-journey"
manifest = json.loads((source / "manifest.json").read_text(encoding="utf-8"))
files = {"manifest.json", "README.md", "ui.ts", "ui.css"}
for key in ("world_file", "desktop_ui_file", "mobile_ui_file", "desktop_ui_stylesheet_file", "mobile_ui_stylesheet_file", "logic_file", "mcp_tools_file"):
    if manifest.get(key):
        files.add(manifest[key])
files.update(c["file_path"] for c in manifest["character_files"])
files.update(a["archive_path"] for a in manifest["assets"])
target = root / "output/mortal-cultivation-journey-world.zip"
with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
    for name in sorted(files):
        archive.write(source / name, name)
with zipfile.ZipFile(target) as archive:
    assert archive.testzip() is None
    assert not any(name.endswith(".jsonc") for name in archive.namelist())
    assert json.loads(archive.read("manifest.json"))["version"] == 10
print(f"Packed and verified {len(files)} files: {target} ({target.stat().st_size} bytes)")
