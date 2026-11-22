#!/usr/bin/env python3
"""Draws vma's mark and writes the page's and the plugin's icons from one
definition.

    python3 tools/icon.py

The mark is a machine's screen on its stand, with keys typed on it: three
lit keys and a caret, on a dark tile. It is plain geometry, so it is drawn
here directly, each pixel sampled 6 x 6 times for smooth edges, and every
file comes from the same numbers as the page's own mark (site/index.html):

    site/vma-logo.svg   the small drawing on its tile, for the browser's tab
    site/vma-icon.png   256 px, for the site's list of tools and previews
    site/favicon.ico    16, 32 and 48 px
    plugin/.claude-plugin/icon.png   512 px, the plugin's listing icon

Small sizes get their own drawing, heavier and with two keys, so the screen
and its keys survive 16 px. Only the standard library: a PNG is a zlib
stream with a few chunks around it.
"""

import struct
import zlib
from pathlib import Path

TILE = (0x13, 0x12, 0x11)
SCREEN = (0xEC, 0xE8, 0xE3)
KEYS = (0x6C, 0xC7, 0x92)

# In the page's 52-unit mark: the screen (its centre, half sizes and corner,
# drawn as a stroke), the stand (a line, its half width), the keys (centres
# and radius) and the caret (centre, half sizes, corner). Then where the mark
# sits on the 64-unit tile, centred, and the tile's corner.
# `centre` is the middle of the mark's extent, which the tile centres.
LARGE = {
    "screen": ((26, 24), (21.5, 15.5), 7, 2.5),
    "stand": ((17, 46.5), (35, 46.5), 2.5),
    "keys": ([(14.5, 24), (21.5, 24), (28.5, 24)], 2.6),
    "caret": ((35, 24), (1.5, 6), 1.5),
    "scale": 0.85, "corner": 14, "centre": (26, 27.5),
}
# At 16 px the stand would touch the screen and three keys would run into
# one bar: the screen alone, two keys and the caret, far enough apart.
SMALL = {
    "screen": ((26, 26), (21.5, 16.5), 6, 3.4),
    "stand": None,
    "keys": ([(15, 26), (26, 26)], 3.6),
    "caret": ((37, 26), (2, 7.5), 2),
    "scale": 0.95, "corner": 12, "centre": (26, 26),
}

SAMPLES = 6
ROOT = Path(__file__).resolve().parent.parent


def drawing_for(size: int) -> dict:
    return SMALL if size <= 24 else LARGE


def round_box(x, y, centre, half, corner) -> float:
    """Signed distance to a rounded rectangle: negative inside."""
    qx = abs(x - centre[0]) - half[0] + corner
    qy = abs(y - centre[1]) - half[1] + corner
    outside = (max(qx, 0) ** 2 + max(qy, 0) ** 2) ** 0.5
    return outside + min(max(qx, qy), 0) - corner


def near_segment(x, y, a, b, half_width) -> bool:
    ax, ay = a
    bx, by = b
    t = max(0.0, min(1.0, ((x - ax) * (bx - ax) + (y - ay) * (by - ay)) / ((bx - ax) ** 2 + (by - ay) ** 2)))
    return (x - ax - t * (bx - ax)) ** 2 + (y - ay - t * (by - ay)) ** 2 <= half_width ** 2


def colour_at(x: float, y: float, drawing: dict):
    """The colour at a point of the 64-unit tile, or None outside it."""
    if round_box(x, y, (32, 32), (32, 32), drawing["corner"]) > 0:
        return None
    scale = drawing["scale"]
    mx = drawing["centre"][0] + (x - 32) / scale
    my = drawing["centre"][1] + (y - 32) / scale
    centre, half, corner, stroke = drawing["screen"]
    if abs(round_box(mx, my, centre, half, corner)) <= stroke:
        return SCREEN
    if drawing["stand"] and near_segment(mx, my, *drawing["stand"]):
        return SCREEN
    keys, radius = drawing["keys"]
    if any((mx - kx) ** 2 + (my - ky) ** 2 <= radius ** 2 for kx, ky in keys):
        return KEYS
    if round_box(mx, my, *drawing["caret"]) <= 0:
        return KEYS
    return TILE


def render(size: int) -> bytes:
    """RGBA rows, each sample of a pixel averaged in."""
    drawing = drawing_for(size)
    unit = 64 / size
    rows = bytearray()
    for py in range(size):
        rows.append(0)  # PNG filter: none
        for px in range(size):
            r = g = b = a = 0
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    colour = colour_at((px + (sx + 0.5) / SAMPLES) * unit, (py + (sy + 0.5) / SAMPLES) * unit, drawing)
                    if colour:
                        r += colour[0]
                        g += colour[1]
                        b += colour[2]
                        a += 1
            n = SAMPLES * SAMPLES
            if a:
                rows += bytes((round(r / a), round(g / a), round(b / a), round(255 * a / n)))
            else:
                rows += bytes(4)
    return bytes(rows)


def png(size: int, rgba: bytes) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rgba, 9)) + chunk(b"IEND", b"")


def ico(images) -> bytes:
    """An .ico of PNG images, as Windows has read since Vista."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = len(head) + 16 * len(images)
    entries = b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return head + entries + b"".join(data for _, data in images)


def svg() -> str:
    # Browsers take the SVG for the tab, at 16 px: the small drawing.
    d = SMALL
    scale = d["scale"]
    dx, dy = 32 - d["centre"][0] * scale, 32 - d["centre"][1] * scale
    (cx, cy), (hx, hy), corner, stroke = d["screen"]
    keys, radius = d["keys"]
    (kx, ky), (kw, kh), kr = d["caret"]
    hex_ = "#{:02x}{:02x}{:02x}".format
    circles = "".join(f'<circle cx="{x}" cy="{y}" r="{radius}"/>' for x, y in keys)
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">'
        f'<rect width="64" height="64" rx="{d["corner"]}" fill="{hex_(*TILE)}"/>'
        f'<g transform="translate({dx:g} {dy:g}) scale({scale})">'
        f'<rect x="{cx - hx}" y="{cy - hy}" width="{2 * hx}" height="{2 * hy}" rx="{corner}" fill="none" stroke="{hex_(*SCREEN)}" stroke-width="{2 * stroke}"/>'
        f'<g fill="{hex_(*KEYS)}">{circles}<rect x="{kx - kw}" y="{ky - kh}" width="{2 * kw}" height="{2 * kh}" rx="{kr}"/></g>'
        "</g></svg>\n"
    )


def main() -> None:
    site = ROOT / "site"
    (site / "vma-logo.svg").write_text(svg())
    (site / "vma-icon.png").write_bytes(png(256, render(256)))
    (site / "favicon.ico").write_bytes(ico([(size, png(size, render(size))) for size in (16, 32, 48)]))
    (ROOT / "plugin" / ".claude-plugin" / "icon.png").write_bytes(png(512, render(512)))


if __name__ == "__main__":
    main()
