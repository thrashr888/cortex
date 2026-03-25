#!/usr/bin/env python3
import csv
import math
import os
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "autoresearch" / "results.tsv"
OUT = ROOT / "autoresearch" / "progress.png"

WIDTH = 960
HEIGHT = 540
MARGIN_LEFT = 70
MARGIN_RIGHT = 20
MARGIN_TOP = 40
MARGIN_BOTTOM = 60
BG = (255, 255, 255)
GRID = (230, 230, 230)
AXIS = (90, 90, 90)
TEXT = (40, 40, 40)
BLUE = (31, 119, 180)
ORANGE = (255, 127, 14)
GREEN = (44, 160, 44)
RED = (214, 39, 40)

FONT = {
    "0": ["111", "101", "101", "101", "111"],
    "1": ["010", "110", "010", "010", "111"],
    "2": ["111", "001", "111", "100", "111"],
    "3": ["111", "001", "111", "001", "111"],
    "4": ["101", "101", "111", "001", "001"],
    "5": ["111", "100", "111", "001", "111"],
    "6": ["111", "100", "111", "101", "111"],
    "7": ["111", "001", "010", "010", "010"],
    "8": ["111", "101", "111", "101", "111"],
    "9": ["111", "101", "111", "001", "111"],
    ".": ["0", "0", "0", "0", "1"],
    "-": ["0", "0", "1", "0", "0"],
}


def read_rows():
    rows = []
    if not RESULTS.exists():
        return rows
    with RESULTS.open() as f:
        reader = csv.DictReader(f, delimiter='\t')
        for row in reader:
            hill = row.get("hillclimb_score", "")
            rows.append(
                {
                    "commit": row.get("commit", ""),
                    "status": row.get("status", ""),
                    "total": parse_float(row.get("total_score", "")),
                    "hill": parse_float(hill),
                }
            )
    return rows


def parse_float(value):
    try:
        if value is None or value == "":
            return None
        return float(value)
    except ValueError:
        return None


def make_canvas(width, height, color):
    return [[color for _ in range(width)] for _ in range(height)]


def set_px(img, x, y, color):
    if 0 <= x < WIDTH and 0 <= y < HEIGHT:
        img[y][x] = color


def fill_rect(img, x0, y0, x1, y1, color):
    for y in range(max(0, y0), min(HEIGHT, y1)):
        row = img[y]
        for x in range(max(0, x0), min(WIDTH, x1)):
            row[x] = color


def line(img, x0, y0, x1, y1, color, thickness=1):
    dx = abs(x1 - x0)
    dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx + dy
    while True:
        for ox in range(-(thickness // 2), thickness // 2 + 1):
            for oy in range(-(thickness // 2), thickness // 2 + 1):
                set_px(img, x0 + ox, y0 + oy, color)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy
            x0 += sx
        if e2 <= dx:
            err += dx
            y0 += sy


def circle(img, cx, cy, r, color):
    for y in range(cy - r, cy + r + 1):
        for x in range(cx - r, cx + r + 1):
            if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                set_px(img, x, y, color)


def draw_char(img, x, y, ch, color, scale=2):
    pattern = FONT.get(ch)
    if not pattern:
        return 4 * scale
    for row_idx, row in enumerate(pattern):
        for col_idx, bit in enumerate(row):
            if bit == "1":
                fill_rect(
                    img,
                    x + col_idx * scale,
                    y + row_idx * scale,
                    x + (col_idx + 1) * scale,
                    y + (row_idx + 1) * scale,
                    color,
                )
    return (len(pattern[0]) + 1) * scale


def draw_text(img, x, y, text, color, scale=2):
    cursor = x
    for ch in text:
        if ch == ' ':
            cursor += 3 * scale
        else:
            cursor += draw_char(img, cursor, y, ch, color, scale)


def write_png(path, img):
    raw = bytearray()
    for row in img:
        raw.append(0)
        for r, g, b in row:
            raw.extend((r, g, b))
    compressed = zlib.compress(bytes(raw), 9)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = bytearray(b"\x89PNG\r\n\x1a\n")
    png.extend(chunk(b"IHDR", struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 2, 0, 0, 0)))
    png.extend(chunk(b"IDAT", compressed))
    png.extend(chunk(b"IEND", b""))
    path.write_bytes(png)


def draw_panel(img, left, top, right, bottom, rows, values, max_value, color, title):
    line(img, left, bottom, right, bottom, AXIS, 2)
    line(img, left, top, left, bottom, AXIS, 2)
    draw_text(img, left + 8, top - 18, title, TEXT, scale=2)

    for frac in [0.0, 0.25, 0.5, 0.75, 1.0]:
        y = int(bottom - frac * (bottom - top))
        line(img, left, y, right, y, GRID, 1)
        label = f"{max_value * frac:.0f}"
        draw_text(img, max(4, left - 48), y - 6, label, TEXT, scale=2)

    if not rows:
        return

    xs = []
    points = []
    width = right - left
    height = bottom - top
    for i, row in enumerate(rows):
        x = left if len(rows) == 1 else int(left + i * width / (len(rows) - 1))
        xs.append(x)
        value = values(row)
        if value is not None:
            y = int(bottom - (value / max_value) * height)
            points.append((x, y, row, value))

    for idx in range(1, len(points)):
        line(img, points[idx - 1][0], points[idx - 1][1], points[idx][0], points[idx][1], color, 2)
    for x, y, row, value in points:
        point_color = color
        if row["status"] == "keep":
            point_color = GREEN if color == BLUE else color
        elif row["status"] == "discard":
            point_color = RED
        circle(img, x, y, 4, point_color)


def main():
    rows = read_rows()
    img = make_canvas(WIDTH, HEIGHT, BG)
    fill_rect(img, 0, 0, WIDTH, HEIGHT, BG)
    draw_text(img, 20, 12, "autoresearch progress", TEXT, scale=3)

    top_left = MARGIN_LEFT
    top_right = WIDTH - MARGIN_RIGHT
    top_top = 70
    top_bottom = 250
    bottom_top = 320
    bottom_bottom = HEIGHT - MARGIN_BOTTOM

    draw_panel(img, top_left, top_top, top_right, top_bottom, rows, lambda row: row["total"], 100.0, BLUE, "gate total_score")

    max_hill = max((row["hill"] for row in rows if row["hill"] is not None), default=1.0)
    hill_scale = max(10.0, math.ceil(max_hill / 10.0) * 10.0)
    draw_panel(img, top_left, bottom_top, top_right, bottom_bottom, rows, lambda row: row["hill"], hill_scale, ORANGE, "hillclimb_score")

    draw_text(img, WIDTH - 220, 16, f"runs {len(rows)}", TEXT, scale=2)
    if rows:
        latest_total = next((row["total"] for row in reversed(rows) if row["total"] is not None), None)
        latest_hill = next((row["hill"] for row in reversed(rows) if row["hill"] is not None), None)
        if latest_total is not None:
            draw_text(img, WIDTH - 220, 34, f"latest gate {latest_total:.1f}", BLUE, scale=2)
        if latest_hill is not None:
            draw_text(img, WIDTH - 220, 52, f"latest hill {latest_hill:.1f}", ORANGE, scale=2)

    write_png(OUT, img)
    print(OUT)


if __name__ == "__main__":
    main()
