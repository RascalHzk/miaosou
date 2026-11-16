#!/usr/bin/env python3
"""生成「秒搜」应用图标（PNG / ICNS / ICO）。

设计：圆角方块 + 橙红渐变（速度）+ 白色放大镜（搜索）+ 左侧速度线（秒）。
小尺寸（≤48px）改用「紧凑版」：去掉速度线、放大镜放大 10%、加粗圆环，
否则速度线在 32px 下会糊成一块脏点。

用法（需要 Pillow；.icns 需要 macOS 的 iconutil）：
    python3 linux/src-tauri/icons/source/make_icons.py

产物写回 linux/src-tauri/icons/，可重复执行（幂等）。
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ICON_DIR = Path(__file__).resolve().parent.parent
SOURCE_DIR = Path(__file__).resolve().parent

# 1024 逻辑画布；内部按 SUPERSAMPLE 倍绘制再缩小，得到干净的边缘。
CANVAS = 1024
SUPERSAMPLE = 4

# 配色：暖橙 → 中国红，斜向渐变
COLOR_TOP_LEFT = (255, 146, 64)
COLOR_BOTTOM_RIGHT = (214, 38, 54)

TILE_PADDING = 40
TILE_RADIUS = 216

LENS_CENTER = (462, 452)
LENS_RADIUS = 196
RING_WIDTH = 92

# 手柄起点落在圆环带宽内（r=150..242），环画在其后，接头被环盖住，避免内圈出现缺口
HANDLE_START = (614, 604)
HANDLE_END = (752, 742)
HANDLE_WIDTH = 96

SPEED_BARS = (
    # (x0, x1, y_center, 高度, 不透明度)；x1 均小于圆环外缘 220，留出间隙
    (104, 196, 376, 28, 140),
    (104, 200, 452, 28, 190),
    (104, 188, 528, 28, 120),
)

# 小尺寸用紧凑版：无量速线、图形放大、圆环加粗
COMPACT_SCALE = 1.10
COMPACT_RING_WIDTH = 102


def _lerp(a: int, b: int, t: float) -> int:
    return round(a + (b - a) * t)


def _gradient(size: int) -> Image.Image:
    """对角线性渐变；低分辨率生成后放大，避免逐像素循环。"""
    steps = 64
    small = Image.new("RGB", (steps, steps))
    pixels = small.load()
    for y in range(steps):
        for x in range(steps):
            t = (x + y) / (2 * (steps - 1))
            pixels[x, y] = (
                _lerp(COLOR_TOP_LEFT[0], COLOR_BOTTOM_RIGHT[0], t),
                _lerp(COLOR_TOP_LEFT[1], COLOR_BOTTOM_RIGHT[1], t),
                _lerp(COLOR_TOP_LEFT[2], COLOR_BOTTOM_RIGHT[2], t),
            )
    return small.resize((size, size), Image.Resampling.BICUBIC)


def _round_caps(draw: ImageDraw.ImageDraw, start, end, width: int, fill) -> None:
    """Pillow 的 line 是平头端点，这里补两个圆做圆角。"""
    draw.line([start, end], fill=fill, width=width)
    radius = width / 2
    for x, y in (start, end):
        draw.ellipse([x - radius, y - radius, x + radius, y + radius], fill=fill)


def render_master(compact: bool = False) -> Image.Image:
    size = CANVAS * SUPERSAMPLE
    scale = SUPERSAMPLE
    grow = COMPACT_SCALE if compact else 1.0

    def s(value: float) -> float:
        return value * scale

    def g(value: float) -> float:
        """图形坐标按画布中心缩放（紧凑版放大）。"""
        return CANVAS / 2 + (value - CANVAS / 2) * grow

    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [s(TILE_PADDING), s(TILE_PADDING), size - s(TILE_PADDING), size - s(TILE_PADDING)],
        radius=s(TILE_RADIUS),
        fill=255,
    )

    master = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    master.paste(_gradient(size), (0, 0), mask)

    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)

    if not compact:
        # 速度线（秒）：三条短横线，越靠中间越亮
        for x0, x1, y_center, height, alpha in SPEED_BARS:
            draw.rounded_rectangle(
                [s(x0), s(g(y_center) - height / 2), s(x1), s(g(y_center) + height / 2)],
                radius=s(height / 2),
                fill=(255, 255, 255, alpha),
            )

    # 放大镜（搜）：先画手柄、再画外环，手柄根部藏在环带里
    _round_caps(
        draw,
        (s(g(HANDLE_START[0])), s(g(HANDLE_START[1]))),
        (s(g(HANDLE_END[0])), s(g(HANDLE_END[1]))),
        int(s(HANDLE_WIDTH * grow)),
        (255, 255, 255, 255),
    )
    cx, cy = g(LENS_CENTER[0]), g(LENS_CENTER[1])
    radius = LENS_RADIUS * grow
    ring = COMPACT_RING_WIDTH if compact else RING_WIDTH
    draw.ellipse(
        [s(cx - radius), s(cy - radius), s(cx + radius), s(cy + radius)],
        outline=(255, 255, 255, 255),
        width=int(s(ring * grow)),
    )

    master.alpha_composite(layer)
    return master.resize((CANVAS, CANVAS), Image.Resampling.LANCZOS)


def _resize(image: Image.Image, size: int) -> Image.Image:
    return image.resize((size, size), Image.Resampling.LANCZOS)


def write_pngs(full: Image.Image, compact: Image.Image) -> None:
    # 32px 及以下用紧凑版；128/256/512 用完整版（启动器/详情页展示的都是这些尺寸）
    _resize(compact, 32).save(ICON_DIR / "32x32.png", "PNG")
    for filename, size in (("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)):
        _resize(full, size).save(ICON_DIR / filename, "PNG")
    full.save(SOURCE_DIR / "miaosou-1024.png", "PNG")
    compact.save(SOURCE_DIR / "miaosou-32-design.png", "PNG")


def write_icns(full: Image.Image, compact: Image.Image) -> None:
    iconset = SOURCE_DIR / "miaosou.iconset"
    iconset.mkdir(exist_ok=True)
    # macOS 会在 Dock/列表里同时用到 16/32，这两档用紧凑版
    mapping = {
        "icon_16x16.png": (compact, 16),
        "icon_16x16@2x.png": (compact, 32),
        "icon_32x32.png": (compact, 32),
        "icon_32x32@2x.png": (compact, 64),
        "icon_128x128.png": (full, 128),
        "icon_128x128@2x.png": (full, 256),
        "icon_256x256.png": (full, 256),
        "icon_256x256@2x.png": (full, 512),
        "icon_512x512.png": (full, 512),
        "icon_512x512@2x.png": (full, 1024),
    }
    for filename, (image, size) in mapping.items():
        _resize(image, size).save(iconset / filename, "PNG")
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(ICON_DIR / "icon.icns")], check=True)


def write_ico(compact: Image.Image) -> None:
    # Windows 任务栏/资源管理器基本只用到 ≤256px，统一用紧凑版避免小尺寸糊
    compact.save(
        ICON_DIR / "icon.ico",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )


def main() -> int:
    full = render_master(compact=False)
    compact = render_master(compact=True)
    write_pngs(full, compact)
    write_ico(compact)
    if sys.platform == "darwin":
        write_icns(full, compact)
    else:
        print("⚠️ 非 macOS：跳过 .icns 生成（需要在 macOS 上跑一次 iconutil）", file=sys.stderr)
    print(f"✅ 图标已写入 {ICON_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
