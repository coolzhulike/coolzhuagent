"""从 P5-F 最终透明母版生成 Windows 应用图标派生资源。

脚本必须从仓库根或任意工作目录均可调用；它只读取最终母版，不会覆盖
母版文件。所有生成物都使用确定的尺寸、高质量 Lanczos 重采样和 RGBA
PNG 编码。ICO 目录按 256px 到 16px 排列，让 Tauri/Windows 读取默认帧
时优先得到高分辨率图像。
"""

from __future__ import annotations

from io import BytesIO
from pathlib import Path
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
SOURCE = ROOT / "docs" / "design-assets" / "coolzhu-icons-2026-08-27" / "final" / f"{ASSET_NAME}.png"
FINAL_DIR = SOURCE.parent
TAURI_DIR = ROOT / "modules" / "gui-desktop" / "packages" / "tauri-shell" / "src-tauri" / "icons"
DESKTOP_DIR = ROOT / "modules" / "gui-desktop" / "packages" / "desktop-console" / "assets"
SIZES = (16, 24, 32, 48, 64, 128, 256)
ICO_ORDER = tuple(reversed(SIZES))


def encode_png(image: Image.Image) -> bytes:
    output = BytesIO()
    image.save(output, format="PNG", optimize=False, compress_level=9)
    return output.getvalue()


def write_ico(frames: dict[int, bytes], destination: Path) -> None:
    directory = bytearray(struct.pack("<HHH", 0, 1, len(ICO_ORDER)))
    payload = bytearray()
    offset = 6 + (16 * len(ICO_ORDER))
    for size in ICO_ORDER:
        frame = frames[size]
        dimension = 0 if size == 256 else size
        directory.extend(
            struct.pack(
                "<BBBBHHII",
                dimension,
                dimension,
                0,
                0,
                1,
                32,
                len(frame),
                offset,
            )
        )
        payload.extend(frame)
        offset += len(frame)
    destination.write_bytes(bytes(directory) + bytes(payload))


def main() -> None:
    if not SOURCE.is_file():
        raise FileNotFoundError(f"最终图标母版不存在: {SOURCE}")

    with Image.open(SOURCE) as loaded:
        if loaded.size != (1254, 1254):
            raise ValueError(f"母版尺寸异常: {loaded.size}")
        if loaded.mode != "RGBA":
            raise ValueError(f"母版必须是 RGBA，实际为: {loaded.mode}")
        corner_alphas = [
            loaded.getpixel(point)[3]
            for point in ((0, 0), (1253, 0), (0, 1253), (1253, 1253))
        ]
        # 母版已由设计阶段核验为透明角；允许单个边缘抗锯齿像素为 1，
        # 但拒绝真正可见的角落颜色，且不在此处修改母版。
        if any(alpha > 1 for alpha in corner_alphas):
            raise ValueError(f"母版透明角像素校验失败: {corner_alphas}")
        source = loaded.copy()

    FINAL_DIR.mkdir(parents=True, exist_ok=True)
    TAURI_DIR.mkdir(parents=True, exist_ok=True)
    DESKTOP_DIR.mkdir(parents=True, exist_ok=True)

    frames: dict[int, bytes] = {}
    for size in SIZES:
        resized = source.resize((size, size), Image.Resampling.LANCZOS)
        if resized.mode != "RGBA":
            raise ValueError(f"{size}px 派生图像未保留 RGBA")
        frame = encode_png(resized)
        frames[size] = frame
        (FINAL_DIR / f"{ASSET_NAME}-{size}.png").write_bytes(frame)

    ico_path = FINAL_DIR / f"{ASSET_NAME}.ico"
    write_ico(frames, ico_path)

    # Tauri 的 bundle 图标和 eframe 窗口图标使用清晰的 256px RGBA PNG；
    # Tauri 托盘在运行时复用 bundle 的默认图标，避免再维护一套视觉资源。
    runtime_png = frames[256]
    (TAURI_DIR / f"{ASSET_NAME}.png").write_bytes(runtime_png)
    (TAURI_DIR / f"{ASSET_NAME}.ico").write_bytes(ico_path.read_bytes())
    (DESKTOP_DIR / f"{ASSET_NAME}.png").write_bytes(runtime_png)

    print(f"pillow={Image.__version__}")
    print(f"source={SOURCE}")
    print(f"ico={ico_path}")
    print(f"sizes={','.join(str(size) for size in SIZES)}")
    print(f"tauri_png={TAURI_DIR / f'{ASSET_NAME}.png'}")
    print(f"desktop_png={DESKTOP_DIR / f'{ASSET_NAME}.png'}")


if __name__ == "__main__":
    main()
