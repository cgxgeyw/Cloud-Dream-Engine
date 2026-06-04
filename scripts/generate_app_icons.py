from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageOps


ROOT = Path(__file__).resolve().parents[1]
ICON_ROOT = ROOT / "src-tauri" / "icons"
DEFAULT_SOURCE = ICON_ROOT / "source-launcher.png"

ROOT_PNGS = {
    "32x32.png": 32,
    "64x64.png": 64,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "256x256.png": 256,
    "512x512.png": 512,
    "1024x1024.png": 1024,
    "icon.png": 512,
    "Square30x30Logo.png": 30,
    "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71,
    "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png": 50,
}

IOS_PNGS = {
    "AppIcon-20x20@1x.png": 20,
    "AppIcon-20x20@2x.png": 40,
    "AppIcon-20x20@2x-1.png": 40,
    "AppIcon-20x20@3x.png": 60,
    "AppIcon-29x29@1x.png": 29,
    "AppIcon-29x29@2x.png": 58,
    "AppIcon-29x29@2x-1.png": 58,
    "AppIcon-29x29@3x.png": 87,
    "AppIcon-40x40@1x.png": 40,
    "AppIcon-40x40@2x.png": 80,
    "AppIcon-40x40@2x-1.png": 80,
    "AppIcon-40x40@3x.png": 120,
    "AppIcon-60x60@2x.png": 120,
    "AppIcon-60x60@3x.png": 180,
    "AppIcon-76x76@1x.png": 76,
    "AppIcon-76x76@2x.png": 152,
    "AppIcon-83.5x83.5@2x.png": 167,
    "AppIcon-512@2x.png": 1024,
}

ANDROID_LEGACY = {
    "mipmap-mdpi": 48,
    "mipmap-hdpi": 72,
    "mipmap-xhdpi": 96,
    "mipmap-xxhdpi": 144,
    "mipmap-xxxhdpi": 192,
}

ANDROID_FOREGROUND = {
    "mipmap-mdpi": 108,
    "mipmap-hdpi": 162,
    "mipmap-xhdpi": 216,
    "mipmap-xxhdpi": 324,
    "mipmap-xxxhdpi": 432,
}


def render_square(image: Image.Image, size: int) -> Image.Image:
    return ImageOps.fit(image, (size, size), method=Image.Resampling.LANCZOS, centering=(0.5, 0.5))


def render_square_with_padding(image: Image.Image, size: int, padding_ratio: float = 0.16) -> Image.Image:
    """Render square with padding for Android adaptive icon safe zone."""
    safe_size = int(size * (1 - padding_ratio * 2))
    scaled = image.resize((safe_size, safe_size), Image.Resampling.LANCZOS)
    result = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    offset = int(size * padding_ratio)
    result.paste(scaled, (offset, offset))
    return result


def save_pngs(image: Image.Image, mapping: dict[str, int], base_dir: Path) -> None:
    for filename, size in mapping.items():
        output = base_dir / filename
        output.parent.mkdir(parents=True, exist_ok=True)
        render_square(image, size).save(output)


def save_android_icons(image: Image.Image) -> None:
    android_root = ICON_ROOT / "android"
    for folder, size in ANDROID_LEGACY.items():
        render_square(image, size).save(android_root / folder / "ic_launcher.png")
        render_square(image, size).save(android_root / folder / "ic_launcher_round.png")

    for folder, size in ANDROID_FOREGROUND.items():
        render_square_with_padding(image, size).save(android_root / folder / "ic_launcher_foreground.png")


def save_ico(image: Image.Image) -> None:
    output = ICON_ROOT / "icon.ico"
    image.save(output, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])


def save_icns(image: Image.Image) -> None:
    output = ICON_ROOT / "icon.icns"
    try:
        image.save(output, format="ICNS")
    except Exception as exc:  # pragma: no cover
        print(f"warning: failed to write {output.name}: {exc}", file=sys.stderr)


def main() -> int:
    source = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else DEFAULT_SOURCE
    if not source.exists():
        print(f"source icon not found: {source}", file=sys.stderr)
        return 1

    base = Image.open(source).convert("RGBA")

    save_pngs(base, ROOT_PNGS, ICON_ROOT)
    save_pngs(base, IOS_PNGS, ICON_ROOT / "ios")
    save_android_icons(base)
    save_ico(render_square(base, 1024))
    save_icns(render_square(base, 1024))

    print(f"generated icons from {source}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
