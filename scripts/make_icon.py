#!/usr/bin/env python3
"""Draws BHNotepad's source icon: a 1024x1024 PNG, no dependencies.

Generated rather than hand-drawn so the icon is reproducible and version
controlled as code. `npx tauri icon` turns the output into the .icns/.ico/.png
set the bundler needs.

Design: a rounded square in BH blue holding a page, with three lines of "text"
and a caret sitting after the shortest one — the caret is the point of the app.
Rendered at 2x and box-filtered down, which is enough anti-aliasing for an
icon without pulling in an imaging library.
"""

import struct
import sys
import zlib

SIZE = 1024
SCALE = 2
W = SIZE * SCALE

# BH blue, matching --accent in styles.css, over a slightly deeper base so the
# tile still reads at 16px.
BG_TOP = (0x3B, 0x7F, 0xE4)
BG_BOTTOM = (0x1E, 0x52, 0xA8)
PAGE = (0xFF, 0xFF, 0xFF)
LINE = (0xC2, 0xCB, 0xD6)
LINE_STRONG = (0x8E, 0x9A, 0xAB)
CARET = (0xF0, 0x8A, 0x24)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def rounded_rect_alpha(x, y, left, top, right, bottom, radius):
    """Coverage (0.0-1.0) of a rounded rectangle at pixel centre (x, y)."""
    if x < left or x > right or y < top or y > bottom:
        return 0.0
    # Distance into the nearest corner arc, if any.
    cx = None
    if x < left + radius and y < top + radius:
        cx, cy = left + radius, top + radius
    elif x > right - radius and y < top + radius:
        cx, cy = right - radius, top + radius
    elif x < left + radius and y > bottom - radius:
        cx, cy = left + radius, bottom - radius
    elif x > right - radius and y > bottom - radius:
        cx, cy = right - radius, bottom - radius
    if cx is None:
        return 1.0
    dx, dy = x - cx, y - cy
    dist = (dx * dx + dy * dy) ** 0.5
    if dist <= radius - 1:
        return 1.0
    if dist >= radius:
        return 0.0
    return radius - dist


def render():
    """Returns a W*W RGBA bytearray."""
    buf = bytearray(W * W * 4)

    tile_inset = 0.085 * W
    tile_radius = 0.225 * W
    page_left, page_right = 0.265 * W, 0.735 * W
    page_top, page_bottom = 0.225 * W, 0.775 * W
    page_radius = 0.035 * W

    # Three text lines; the last is short so the caret has somewhere to sit.
    line_height = 0.052 * W
    lines = [
        (0.335 * W, 0.355 * W, 0.665 * W, LINE_STRONG),
        (0.335 * W, 0.465 * W, 0.665 * W, LINE),
        (0.335 * W, 0.575 * W, 0.545 * W, LINE),
    ]
    caret_x = 0.565 * W
    caret_top, caret_bottom = 0.556 * W, 0.648 * W
    caret_width = 0.028 * W

    for py in range(W):
        y = py + 0.5
        row = py * W * 4
        gradient = lerp(BG_TOP, BG_BOTTOM, py / (W - 1))
        for px in range(W):
            x = px + 0.5
            tile = rounded_rect_alpha(
                x, y, tile_inset, tile_inset, W - tile_inset, W - tile_inset, tile_radius
            )
            if tile <= 0.0:
                continue

            r, g, b = gradient

            page = rounded_rect_alpha(
                x, y, page_left, page_top, page_right, page_bottom, page_radius
            )
            if page > 0.0:
                r, g, b = (
                    round(c + (p - c) * page) for c, p in zip((r, g, b), PAGE)
                )

            if page > 0.5:
                for lx0, ly, lx1, colour in lines:
                    cover = rounded_rect_alpha(
                        x, y, lx0, ly, lx1, ly + line_height, line_height / 2
                    )
                    if cover > 0.0:
                        r, g, b = (
                            round(c + (t - c) * cover) for c, t in zip((r, g, b), colour)
                        )

                cover = rounded_rect_alpha(
                    x, y, caret_x, caret_top, caret_x + caret_width, caret_bottom,
                    caret_width / 2,
                )
                if cover > 0.0:
                    r, g, b = (
                        round(c + (t - c) * cover) for c, t in zip((r, g, b), CARET)
                    )

            offset = row + px * 4
            buf[offset] = r
            buf[offset + 1] = g
            buf[offset + 2] = b
            buf[offset + 3] = round(255 * tile)
    return buf


def downsample(buf):
    """Box-filters W*W down to SIZE*SIZE, averaging in premultiplied alpha so
    edge pixels do not pick up colour from fully transparent neighbours."""
    out = bytearray(SIZE * SIZE * 4)
    for oy in range(SIZE):
        for ox in range(SIZE):
            r = g = b = a = 0
            for dy in range(SCALE):
                base = ((oy * SCALE + dy) * W + ox * SCALE) * 4
                for dx in range(SCALE):
                    o = base + dx * 4
                    pa = buf[o + 3]
                    r += buf[o] * pa
                    g += buf[o + 1] * pa
                    b += buf[o + 2] * pa
                    a += pa
            o = (oy * SIZE + ox) * 4
            if a:
                out[o] = r // a
                out[o + 1] = g // a
                out[o + 2] = b // a
            out[o + 3] = a // (SCALE * SCALE)
    return out


def write_png(path, pixels):
    raw = bytearray()
    for y in range(SIZE):
        raw.append(0)  # filter type 0
        raw += pixels[y * SIZE * 4 : (y + 1) * SIZE * 4]

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(
            ">I", zlib.crc32(body) & 0xFFFFFFFF
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as handle:
        handle.write(png)


if __name__ == "__main__":
    target = sys.argv[1] if len(sys.argv) > 1 else "icon-source.png"
    write_png(target, downsample(render()))
    print(f"wrote {target} ({SIZE}x{SIZE})")
