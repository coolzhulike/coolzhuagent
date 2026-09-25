"""离线校验 P5-F 应用图标的 PNG/ICO 尺寸、Alpha、哈希与帧目录。"""

from __future__ import annotations

from io import BytesIO
from pathlib import Path
import hashlib
import struct
import sys


SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent
BUNDLED_PILLOW = ROOT / "tmp" / "imagegen" / "python-packages"

try:
    from PIL import Image
except ModuleNotFoundError as error:
    if not BUNDLED_PILLOW.is_dir():
        raise RuntimeError(
            "需要 Pillow；请安装 Pillow，或准备仓库约定的 tmp/imagegen/python-packages。"
        ) from error
    sys.path.insert(0, str(BUNDLED_PILLOW))
    from PIL import Image


ASSET_NAME = "app-icon-cz-moon-gate-lantern-v1"
FINAL_DIR = ROOT / "docs" / "design-assets" / "coolzhu-icons-2026-08-27" / "final"
TAURI_DIR = ROOT / "modules" / "gui-desktop" / "packages" / "tauri-shell" / "src-tauri" / "icons"
DESKTOP_DIR = ROOT / "modules" / "gui-desktop" / "packages" / "desktop-console" / "assets"
SIZES = (16, 24, 32, 48, 64, 128, 256)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inspect_png(path: Path, expected_size: int) -> dict[str, object]:
    with Image.open(path) as image:
        image.load()
        if image.size != (expected_size, expected_size):
            raise AssertionError(f"PNG 尺寸错误: {path} -> {image.size}")
        if image.mode != "RGBA":
            raise AssertionError(f"PNG 非 RGBA: {path} -> {image.mode}")
        alpha_min, alpha_max = image.getchannel("A").getextrema()
        return {
            "path": str(path),
            "size": expected_size,
            "mode": image.mode,
            "alpha_min": alpha_min,
            "alpha_max": alpha_max,
            "sha256": sha256(path),
        }


def inspect_ico(path: Path) -> list[dict[str, object]]:
    raw = path.read_bytes()
    if len(raw) < 6:
        raise AssertionError(f"ICO 太短: {path}")
    reserved, kind, count = struct.unpack_from("<HHH", raw, 0)
    if (reserved, kind) != (0, 1):
        raise AssertionError(f"ICO 头错误: {path} -> {(reserved, kind)}")
    if count != len(SIZES):
        raise AssertionError(f"ICO 帧数错误: {path} -> {count}")

    entries: list[dict[str, object]] = []
    for index in range(count):
        offset = 6 + (16 * index)
        if offset + 16 > len(raw):
            raise AssertionError(f"ICO 目录越界: {path} index={index}")
        width_byte, height_byte, _colors, _reserved, planes, bpp, length, data_offset = struct.unpack_from(
            "<BBBBHHII", raw, offset
        )
        width = 256 if width_byte == 0 else width_byte
        height = 256 if height_byte == 0 else height_byte
        if width != height or planes != 1 or bpp != 32:
            raise AssertionError(
                f"ICO 条目属性错误: {path} index={index} size={width}x{height} planes={planes} bpp={bpp}"
            )
        if data_offset + length > len(raw):
            raise AssertionError(f"ICO 数据越界: {path} index={index}")
        frame_bytes = raw[data_offset : data_offset + length]
        with Image.open(BytesIO(frame_bytes)) as frame:
            frame.load()
            if frame.size != (width, height) or frame.mode != "RGBA":
                raise AssertionError(
                    f"ICO PNG 帧错误: {path} index={index} -> {frame.size} {frame.mode}"
                )
            alpha_min, alpha_max = frame.getchannel("A").getextrema()
        entries.append(
            {
                "size": width,
                "bytes": length,
                "alpha_min": alpha_min,
                "alpha_max": alpha_max,
            }
        )

    actual_sizes = tuple(entry["size"] for entry in entries)
    if set(actual_sizes) != set(SIZES):
        raise AssertionError(f"ICO 尺寸集合错误: {path} -> {actual_sizes}")
    if actual_sizes[0] != 256:
        raise AssertionError(f"ICO 首帧不是 256px: {path} -> {actual_sizes}")
    return entries


def main() -> None:
    source_path = FINAL_DIR / f"{ASSET_NAME}.png"
    with Image.open(source_path) as source:
        source.load()
        if source.size != (1254, 1254) or source.mode != "RGBA":
            raise AssertionError(f"母版属性错误: {source_path} -> {source.size} {source.mode}")
        corners = tuple(
            source.getpixel(point)[3]
            for point in ((0, 0), (1253, 0), (0, 1253), (1253, 1253))
        )
        if any(alpha > 1 for alpha in corners):
            raise AssertionError(f"母版透明角存在可见像素: {source_path} -> {corners}")

    png_results = [
        inspect_png(FINAL_DIR / f"{ASSET_NAME}-{size}.png", size)
        for size in SIZES
    ]
    final_ico = FINAL_DIR / f"{ASSET_NAME}.ico"
    tauri_ico = TAURI_DIR / f"{ASSET_NAME}.ico"
    tauri_png = TAURI_DIR / f"{ASSET_NAME}.png"
    desktop_png = DESKTOP_DIR / f"{ASSET_NAME}.png"
    ico_results = inspect_ico(final_ico)
    inspect_ico(tauri_ico)
    inspect_png(tauri_png, 256)
    inspect_png(desktop_png, 256)

    runtime_png = (FINAL_DIR / f"{ASSET_NAME}-256.png").read_bytes()
    if tauri_png.read_bytes() != runtime_png:
        raise AssertionError("Tauri PNG 与最终 256px 派生资源不一致")
    if desktop_png.read_bytes() != runtime_png:
        raise AssertionError("desktop-console PNG 与最终 256px 派生资源不一致")
    if tauri_ico.read_bytes() != final_ico.read_bytes():
        raise AssertionError("Tauri ICO 与最终 ICO 不一致")

    print(f"pillow={Image.__version__}")
    print(f"PASS p5f-icon source={source_path} sha256={sha256(source_path)} corners-alpha={corners}")
    print(f"PASS p5f-icon ico={final_ico} sha256={sha256(final_ico)} frames={tuple(entry['size'] for entry in ico_results)}")
    for result in png_results:
        print(
            f"PASS png {result['size']}x{result['size']} RGBA "
            f"alpha={result['alpha_min']}..{result['alpha_max']} sha256={result['sha256']}"
        )
    print(f"PASS runtime tauri_ico_sha256={sha256(tauri_ico)}")
    print(f"PASS runtime png_sha256={sha256(tauri_png)} desktop_png_sha256={sha256(desktop_png)}")


if __name__ == "__main__":
    main()
