from __future__ import annotations

import json
import math
from pathlib import Path
from statistics import median

from PIL import Image


ROOT = Path(__file__).resolve().parents[6]
ACTION_DIR = ROOT / "packages" / "tauri-shell" / "ui" / "assets" / "pet-actions"
SHEET_DIR = ACTION_DIR / "generated-sheets-20260617"
MANIFEST_PATH = ACTION_DIR / "generated-previews-20260617" / "manifest.json"
PREVIEW_DIR = ACTION_DIR / "generated-previews-20260617"

CANVAS = 256
ANCHOR_X = 128
BASELINE_Y = 246
CENTER_Y = 128
EDGE_PAD = 8
TARGET_SLEEPING_WIDTH = 236
FINAL_FIT_PAD = 2
TARGET_STANDING_HEIGHT = 238
TARGET_STANDING_BODY_HEIGHT = 226
TARGET_FACE_HEIGHT = 44


STATE_POLICIES = {
    "blink": {"mode": "height_crop", "anchor": "baseline", "target_primary_height": TARGET_STANDING_HEIGHT},
    "thinking": {"mode": "height_crop", "anchor": "baseline"},
    "working": {"mode": "uniform_fit", "anchor": "baseline"},
    "carrying": {"mode": "height_crop", "anchor": "baseline"},
    "juggling": {"mode": "height_crop", "anchor": "baseline"},
    "sweeping": {"mode": "uniform_fit", "anchor": "baseline"},
    "success": {"mode": "face_scale", "anchor": "baseline"},
    "perform_martial": {"mode": "face_scale", "anchor": "baseline"},
    "sword-flight": {"mode": "uniform_fit", "anchor": "center", "center_y": 132},
    "sleeping": {"mode": "uniform_fit", "anchor": "center", "center_y": 134},
}

STATE_FRAME_SELECTIONS = {
    # Runtime blink only plays six close-eye frames; keep generated metrics in
    # lockstep with the theme instead of letting inactive source frames hide
    # scale regressions.
    "blink": [0, 1, 2, 3, 4, 5],
    # Drop the oversized first close-up and loop the stable generated poses.
    "success": [1, 2, 3, 4, 5, 6, 7, 1],
    # Keep the generated poses whose sword effects fit the pet canvas at a
    # character scale matching idle. Reversing the middle poses makes a loop.
    "perform_martial": [0, 3, 6, 7, 6, 3, 0, 7],
}

ORPHAN_CLEANUP_STATES = {"idle", "blink", "thinking", "working", "carrying", "juggling", "crowned"}


def remove_green_screen(image: Image.Image) -> Image.Image:
    """Convert Image Gen green-screen sheets to transparent RGBA sprites."""
    rgb = image.convert("RGB")
    rgba = Image.new("RGBA", rgb.size)
    output = []
    for r, g, b in rgb.getdata():
        green_advantage = g - max(r, b)
        is_green = g > 95 and green_advantage > 34 and g > r * 1.24 and g > b * 1.18
        if is_green:
            output.append((r, g, b, 0))
            continue
        # Suppress green spill on antialiased sprite edges without repainting the art.
        if g > 80 and green_advantage > 18 and g > r * 1.05 and g > b * 1.05:
            g = int(max(r, b, min(g, (r + b) / 2 + 18)))
        output.append((r, g, b, 255))
    rgba.putdata(output)
    return rgba


def alpha_bbox(image: Image.Image) -> tuple[int, int, int, int]:
    bbox = image.getchannel("A").getbbox()
    if not bbox:
        raise RuntimeError("empty transparent frame")
    return bbox


def crop_frame(sheet: Image.Image, bbox: list[int]) -> Image.Image:
    left, top, right, bottom = bbox
    crop_box = (
        max(0, left - EDGE_PAD),
        max(0, top - EDGE_PAD),
        min(sheet.width, right + EDGE_PAD),
        min(sheet.height, bottom + EDGE_PAD),
    )
    frame = sheet.crop(crop_box)
    visible = alpha_bbox(frame)
    trim_box = (
        max(0, visible[0] - 2),
        max(0, visible[1] - 2),
        min(frame.width, visible[2] + 2),
        min(frame.height, visible[3] + 2),
    )
    return frame.crop(trim_box)


def resize_frame(frame: Image.Image, scale: float) -> Image.Image:
    width = max(1, int(round(frame.width * scale)))
    height = max(1, int(round(frame.height * scale)))
    return frame.resize((width, height), Image.Resampling.LANCZOS)


def per_frame_height_scale(frame: Image.Image) -> float:
    left, top, right, bottom = alpha_bbox(frame)
    width = max(1, right - left)
    height = max(1, bottom - top)
    return min(
        TARGET_STANDING_HEIGHT / height,
        (CANVAS - 2 * FINAL_FIT_PAD) / width,
        (CANVAS - 2 * FINAL_FIT_PAD) / height,
    )


def per_frame_height_crop_scale(frame: Image.Image, target_primary_height: float = TARGET_STANDING_BODY_HEIGHT) -> float:
    left, top, right, bottom = primary_bbox(frame)
    height = max(1, bottom - top)
    return target_primary_height / height


def skin_components(image: Image.Image) -> list[tuple[int, int, int, int, int]]:
    rgba = image.convert("RGBA")
    pixels = rgba.load()
    mask = Image.new("L", rgba.size)
    mask_pixels = mask.load()
    for y in range(rgba.height):
        for x in range(rgba.width):
            r, g, b, a = pixels[x, y]
            if (
                a > 80
                and r > 145
                and 70 < g < 220
                and b > 45
                and r > g * 1.04
                and g > b * 1.03
                and r - b > 45
            ):
                mask_pixels[x, y] = 255
    component_image = Image.new("RGBA", rgba.size, (0, 0, 0, 0))
    component_image.putalpha(mask)
    return alpha_components(component_image)


def face_bbox(image: Image.Image) -> tuple[int, int, int, int]:
    width, height = image.size
    candidates = []
    for area, left, top, right, bottom in skin_components(image):
        face_width = right - left
        face_height = bottom - top
        aspect = face_width / max(1, face_height)
        center_x = (left + right) / 2
        if (
            area >= 200
            and 0.7 <= aspect <= 1.4
            and 20 <= face_width <= 120
            and 20 <= face_height <= 120
            and abs(center_x - width / 2) <= width * 0.28
            and top < height * 0.65
        ):
            candidates.append((area, left, top, right, bottom))
    if not candidates:
        raise RuntimeError("character face not found")
    _, left, top, right, bottom = max(candidates)
    return left, top, right, bottom


def per_frame_face_scale(frame: Image.Image) -> float:
    left, top, right, bottom = face_bbox(frame)
    face_height = max(1, bottom - top)
    alpha_left, alpha_top, alpha_right, alpha_bottom = alpha_bbox(frame)
    width = max(1, alpha_right - alpha_left)
    height = max(1, alpha_bottom - alpha_top)
    fit_scale = min(
        CANVAS / width,
        CANVAS / height,
    )
    return min(TARGET_FACE_HEIGHT / face_height, fit_scale)


def uniform_fit_scale(frames: list[Image.Image], state: str) -> float:
    sizes = []
    for frame in frames:
        left, top, right, bottom = alpha_bbox(frame)
        sizes.append((right - left, bottom - top))
    max_width = max(width for width, _ in sizes)
    max_height = max(height for _, height in sizes)
    if state == "sleeping":
        return min(TARGET_SLEEPING_WIDTH / max_width, (CANVAS - 2 * EDGE_PAD) / max_height)
    return min((CANVAS - 2 * EDGE_PAD) / max_width, (CANVAS - 2 * EDGE_PAD) / max_height)


def alpha_components(image: Image.Image) -> list[tuple[int, int, int, int, int]]:
    alpha = image.getchannel("A")
    pixels = alpha.load()
    width, height = alpha.size
    seen: set[tuple[int, int]] = set()
    components: list[tuple[int, int, int, int, int]] = []
    for y in range(height):
        for x in range(width):
            if not pixels[x, y] or (x, y) in seen:
                continue
            stack = [(x, y)]
            seen.add((x, y))
            left = right = x
            top = bottom = y
            area = 0
            for current_x, current_y in stack:
                area += 1
                left = min(left, current_x)
                right = max(right, current_x)
                top = min(top, current_y)
                bottom = max(bottom, current_y)
                for next_x in (current_x - 1, current_x, current_x + 1):
                    for next_y in (current_y - 1, current_y, current_y + 1):
                        if (
                            next_x < 0
                            or next_y < 0
                            or next_x >= width
                            or next_y >= height
                            or (next_x, next_y) in seen
                            or not pixels[next_x, next_y]
                        ):
                            continue
                        seen.add((next_x, next_y))
                        stack.append((next_x, next_y))
            components.append((area, left, top, right + 1, bottom + 1))
    return sorted(components, reverse=True)


def primary_bbox(image: Image.Image) -> tuple[int, int, int, int]:
    components = alpha_components(image)
    if not components:
        raise RuntimeError("empty transparent frame")
    _, left, top, right, bottom = components[0]
    return left, top, right, bottom


def clear_box(image: Image.Image, box: tuple[int, int, int, int]) -> None:
    pixels = image.load()
    left, top, right, bottom = box
    for y in range(top, bottom):
        for x in range(left, right):
            r, g, b, a = pixels[x, y]
            if a:
                pixels[x, y] = (r, g, b, 0)


def alpha_composite_clipped(canvas: Image.Image, source: Image.Image, position: tuple[int, int]) -> None:
    paste_x, paste_y = position
    source_left = max(0, -paste_x)
    source_top = max(0, -paste_y)
    source_right = min(source.width, CANVAS - paste_x)
    source_bottom = min(source.height, CANVAS - paste_y)
    if source_right <= source_left or source_bottom <= source_top:
        return
    canvas.alpha_composite(
        source.crop((source_left, source_top, source_right, source_bottom)),
        (max(0, paste_x), max(0, paste_y)),
    )


def remove_top_orphans(image: Image.Image, state: str) -> Image.Image:
    if state not in ORPHAN_CLEANUP_STATES:
        return image
    components = alpha_components(image)
    if len(components) <= 1:
        return image
    largest = components[0]
    cleaned = image.copy()
    largest_top = largest[2]
    for area, left, top, right, bottom in components[1:]:
        if bottom < largest_top and area < 2000:
            clear_box(cleaned, (left, top, right, bottom))
        elif area < 450 and bottom < 96 and bottom <= largest_top + 90:
            clear_box(cleaned, (left, top, right, bottom))
    return cleaned


def compose_frame(frame: Image.Image, state: str, policy: dict[str, object], scale: float) -> tuple[Image.Image, dict[str, float]]:
    scaled = resize_frame(frame, scale)
    if policy.get("mode") == "height_crop":
        left, top, right, bottom = primary_bbox(scaled)
    else:
        left, top, right, bottom = alpha_bbox(scaled)
    center_x = (left + right) / 2
    center_y = (top + bottom) / 2
    anchor = policy.get("anchor", "baseline")
    if anchor == "center":
        target_y = float(policy.get("center_y", CENTER_Y))
        paste_x = int(round(ANCHOR_X - center_x))
        paste_y = int(round(target_y - center_y))
    else:
        paste_x = int(round(ANCHOR_X - center_x))
        paste_y = int(round(BASELINE_Y - bottom))

    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    alpha_composite_clipped(canvas, scaled, (paste_x, paste_y))
    canvas = remove_top_orphans(canvas, state)
    composed_bbox = alpha_bbox(canvas)
    try:
        composed_face_bbox = face_bbox(canvas)
    except RuntimeError:
        composed_face_bbox = (0, 0, 0, 0)
    metrics = {
        "scale": scale,
        "left": composed_bbox[0],
        "top": composed_bbox[1],
        "right": composed_bbox[2],
        "bottom": composed_bbox[3],
        "width": composed_bbox[2] - composed_bbox[0],
        "height": composed_bbox[3] - composed_bbox[1],
        "center_x": (composed_bbox[0] + composed_bbox[2]) / 2,
        "center_y": (composed_bbox[1] + composed_bbox[3]) / 2,
        "face_width": composed_face_bbox[2] - composed_face_bbox[0],
        "face_height": composed_face_bbox[3] - composed_face_bbox[1],
    }
    return canvas, metrics


def metric_summary(rows: list[dict[str, float]]) -> dict[str, float]:
    def values(key: str) -> list[float]:
        return [float(row[key]) for row in rows]

    return {
        "width_min": min(values("width")),
        "width_max": max(values("width")),
        "height_min": min(values("height")),
        "height_max": max(values("height")),
        "center_x_delta": max(values("center_x")) - min(values("center_x")),
        "center_y_delta": max(values("center_y")) - min(values("center_y")),
        "bottom_delta": max(values("bottom")) - min(values("bottom")),
        "scale_min": min(values("scale")),
        "scale_max": max(values("scale")),
        "face_width_min": min(values("face_width")),
        "face_width_max": max(values("face_width")),
        "face_height_min": min(values("face_height")),
        "face_height_max": max(values("face_height")),
    }


def make_preview(frames_by_state: dict[str, list[Image.Image]]) -> None:
    states = list(frames_by_state)
    columns = 8
    cell = 128
    preview = Image.new("RGBA", (columns * cell, len(states) * cell), (8, 14, 22, 255))
    for row, state in enumerate(states):
        for col, frame in enumerate(frames_by_state[state]):
            thumb = frame.resize((cell, cell), Image.Resampling.LANCZOS)
            preview.alpha_composite(thumb, (col * cell, row * cell))
    preview.save(PREVIEW_DIR / "all-states-stabilized-preview.png")


def cleanup_existing_state_frames(state: str) -> None:
    for path in ACTION_DIR.glob(f"{state}-*.png"):
        path.unlink()


def stabilize() -> None:
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    PREVIEW_DIR.mkdir(parents=True, exist_ok=True)
    metrics = {}
    frames_by_state: dict[str, list[Image.Image]] = {}

    for state, info in manifest.items():
        source = Path(info["source"])
        if not source.exists():
            source = SHEET_DIR / f"{state}-sheet.png"
        sheet = remove_green_screen(Image.open(source))
        frames = [crop_frame(sheet, bbox) for bbox in info["bboxes"]]
        selected_indices = STATE_FRAME_SELECTIONS.get(state)
        if selected_indices:
            frames = [frames[index] for index in selected_indices]
        policy = {"mode": "height", "anchor": "baseline"} | STATE_POLICIES.get(state, {})
        if policy["mode"] == "uniform_fit":
            scales = [uniform_fit_scale(frames, state)] * len(frames)
        elif policy["mode"] == "face_scale":
            scales = [per_frame_face_scale(frame) for frame in frames]
        elif policy["mode"] == "height_crop":
            target_primary_height = float(policy.get("target_primary_height", TARGET_STANDING_BODY_HEIGHT))
            scales = [per_frame_height_crop_scale(frame, target_primary_height) for frame in frames]
        else:
            scales = [per_frame_height_scale(frame) for frame in frames]

        output_frames = []
        rows = []
        cleanup_existing_state_frames(state)
        for index, (frame, scale) in enumerate(zip(frames, scales)):
            composed, row = compose_frame(frame, state, policy, scale)
            output_frames.append(composed)
            rows.append(row)
            composed.save(ACTION_DIR / f"{state}-{index}.png")
        output_frames[0].save(ACTION_DIR / f"{state}.png")
        frames_by_state[state] = output_frames
        metrics[state] = {
            "policy": policy,
            "summary": metric_summary(rows),
            "frames": rows,
        }

    # Compatibility aliases used by older paths.
    if "sleeping" in frames_by_state:
        for index, frame in enumerate(frames_by_state["sleeping"]):
            frame.save(ACTION_DIR / f"resting-{index}.png")
        frames_by_state["sleeping"][0].save(ACTION_DIR / "resting.png")

    make_preview(frames_by_state)
    (PREVIEW_DIR / "stabilized-metrics.json").write_text(
        json.dumps(metrics, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    print(f"stabilized {sum(len(v) for v in frames_by_state.values())} pet frames")
    print(f"actions={ACTION_DIR}")
    print(f"metrics={PREVIEW_DIR / 'stabilized-metrics.json'}")
    print(f"preview={PREVIEW_DIR / 'all-states-stabilized-preview.png'}")


if __name__ == "__main__":
    stabilize()
