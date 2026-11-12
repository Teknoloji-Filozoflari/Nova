#!/usr/bin/env python3
"""Convert Nova TestBackend JSON frames to PNG screenshots.

Usage:
  python3 docs/screenshots/render_frames.py docs/screenshots/frames docs/screenshots
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

CELL_W = 9
CELL_H = 18
PAD = 16
DEFAULT_BG = (13, 17, 23, 255)  # mock #0d1117
DEFAULT_FG = (165, 179, 207, 255)  # mock #a5b3cf
FONT_CANDIDATES = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
]


def load_font(size: int) -> ImageFont.ImageFont:
    for path in FONT_CANDIDATES:
        if Path(path).exists():
            return ImageFont.truetype(path, size=size)
    return ImageFont.load_default()


def rgba(value, fallback):
    if not value or len(value) < 4:
        return fallback
    r, g, b, a = value[:4]
    if a == 0:
        return fallback
    return (int(r), int(g), int(b), 255)


def render_frame(data: dict, out_path: Path) -> None:
    width = int(data["width"])
    height = int(data["height"])
    cells = data["cells"]
    font = load_font(15)

    img_w = PAD * 2 + width * CELL_W
    img_h = PAD * 2 + height * CELL_H
    img = Image.new("RGBA", (img_w, img_h), DEFAULT_BG)
    draw = ImageDraw.Draw(img)

    # Soft vignette edge so the shot reads as a product frame, not raw dump.
    draw.rounded_rectangle(
        (6, 6, img_w - 7, img_h - 7),
        radius=10,
        outline=(40, 48, 58, 255),
        width=2,
    )

    for y in range(height):
        for x in range(width):
            cell = cells[y * width + x]
            bg = rgba(cell.get("bg"), DEFAULT_BG)
            fg = rgba(cell.get("fg"), DEFAULT_FG)
            symbol = cell.get("s") or " "
            px = PAD + x * CELL_W
            py = PAD + y * CELL_H
            draw.rectangle((px, py, px + CELL_W, py + CELL_H), fill=bg)
            if symbol.strip() or symbol in ("█", "■", "▌", "▐", "▀", "▄"):
                draw.text((px, py - 1), symbol, fill=fg, font=font)

    out_path.parent.mkdir(parents=True, exist_ok=True)
    img.convert("RGB").save(out_path, "PNG", optimize=True)
    print(f"wrote {out_path}")


def main() -> int:
    frames_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "docs/screenshots/frames")
    out_dir = Path(sys.argv[2] if len(sys.argv) > 2 else "docs/screenshots")
    mapping = {
        "01-main-list.json": "main-list.png",
        "02-store.json": "store.png",
        "03-settings.json": "settings.png",
        "04-ai-actions.json": "ai-actions.png",
        "05-commands-start.json": "commands-start.png",
        "06-commands-task-detail.json": "commands-task-detail.png",
        "07-commands-security-checklist.json": "commands-security-checklist.png",
        # PR önizleme kareleri (isteğe bağlı; yoksa atlanır)
        "preview-topic-guide-wide.json": "preview-topic-guide-wide.png",
        "preview-topic-guide-learning.json": "preview-topic-guide-learning.png",
        "syntax-highlight-list.json": "syntax-highlight-list.png",
        "syntax-highlight-detail.json": "syntax-highlight-detail.png",
    }
    required = {
        k: v
        for k, v in mapping.items()
        if not k.startswith("preview-")
    }
    for src_name, dst_name in required.items():
        src = frames_dir / src_name
        if not src.exists():
            print(f"missing {src}", file=sys.stderr)
            return 1
        render_frame(json.loads(src.read_text()), out_dir / dst_name)
    for src_name, dst_name in mapping.items():
        if not (
            src_name.startswith("preview-") or src_name.startswith("syntax-highlight-")
        ):
            continue
        src = frames_dir / src_name
        if src.exists():
            render_frame(json.loads(src.read_text()), out_dir / dst_name)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
