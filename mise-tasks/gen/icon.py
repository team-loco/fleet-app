import sys
from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

CANVAS_SIZE = 1024
ARTWORK_SIZE = 824
EDGE_STEP = 6
EDGE_INSET = 1
SUPERSAMPLE = 4
ICONSET_SIZES = (16, 32, 128, 256, 512)
RETINA_SCALE = 2
MAX_CHANNEL = 255


def edge_barrier(rgb):
    pixels = np.asarray(rgb).astype(int)
    barrier = np.zeros(pixels.shape[:2], dtype=bool)
    horizontal = np.abs(np.diff(pixels, axis=1)).max(axis=2) > EDGE_STEP
    vertical = np.abs(np.diff(pixels, axis=0)).max(axis=2) > EDGE_STEP
    barrier[:, 1:] |= horizontal
    barrier[:, :-1] |= horizontal
    barrier[1:, :] |= vertical
    barrier[:-1, :] |= vertical
    return barrier


def surround_mask(barrier):
    height, width = barrier.shape
    seen = np.zeros_like(barrier)
    queue = deque()
    for x, y in ((0, 0), (width - 1, 0), (0, height - 1), (width - 1, height - 1)):
        seen[y, x] = True
        queue.append((x, y))
    while queue:
        x, y = queue.popleft()
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < width and 0 <= ny < height and not seen[ny, nx] and not barrier[ny, nx]:
                seen[ny, nx] = True
                queue.append((nx, ny))
    return seen


def erode(mask, steps):
    for _ in range(steps):
        grown = mask.copy()
        grown[1:, :] &= mask[:-1, :]
        grown[:-1, :] &= mask[1:, :]
        grown[:, 1:] &= mask[:, :-1]
        grown[:, :-1] &= mask[:, 1:]
        mask = grown
    return mask


def convex_hull(points):
    ordered = sorted(set(points))

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    def half(sequence):
        chain = []
        for point in sequence:
            while len(chain) >= 2 and cross(chain[-2], chain[-1], point) <= 0:
                chain.pop()
            chain.append(point)
        return chain[:-1]

    return half(ordered) + half(reversed(ordered))


def tile_coverage(tile):
    boundary = tile & ~erode(tile, 1)
    ys, xs = np.nonzero(boundary)
    hull = convex_hull(list(zip(xs.tolist(), ys.tolist())))
    height, width = tile.shape
    canvas = Image.new("L", (width * SUPERSAMPLE, height * SUPERSAMPLE), 0)
    scaled = [(x * SUPERSAMPLE + SUPERSAMPLE // 2, y * SUPERSAMPLE + SUPERSAMPLE // 2) for x, y in hull]
    ImageDraw.Draw(canvas).polygon(scaled, fill=1)
    inner = erode(np.asarray(canvas, dtype=bool), EDGE_INSET * SUPERSAMPLE)
    counts = inner.reshape(height, SUPERSAMPLE, width, SUPERSAMPLE).sum(axis=(1, 3))
    return counts / (SUPERSAMPLE * SUPERSAMPLE), xs.min(), ys.min(), xs.max() + 1, ys.max() + 1


def unmatte(rgb, coverage):
    pixels = np.asarray(rgb).astype(float)
    shadow = (MAX_CHANNEL - pixels.min(axis=2)) / MAX_CHANNEL
    alpha = coverage + (1 - coverage) * shadow
    safe = np.where(alpha > 0, alpha, 1)
    colour = pixels * (coverage / safe)[..., None]
    out = np.zeros(pixels.shape[:2] + (4,), dtype=np.uint8)
    out[..., :3] = np.clip(np.rint(colour), 0, MAX_CHANNEL)
    out[..., 3] = np.rint(alpha * MAX_CHANNEL)
    return Image.fromarray(out, "RGBA")


def place(image, bounds):
    left, top, right, bottom = bounds
    tile_width = right - left
    scale = ARTWORK_SIZE / tile_width
    scaled_size = round(image.width * scale)
    scaled = image.resize((scaled_size, scaled_size), Image.LANCZOS)
    centre_x = round((left + right) / 2 * scale)
    centre_y = round((top + bottom) / 2 * scale)
    canvas = Image.new("RGBA", (CANVAS_SIZE, CANVAS_SIZE), (0, 0, 0, 0))
    canvas.paste(scaled, (CANVAS_SIZE // 2 - centre_x, CANVAS_SIZE // 2 - centre_y))
    return canvas


def main():
    source, png_out, iconset = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    rgb = Image.open(source).convert("RGB")
    tile = ~surround_mask(edge_barrier(rgb))
    coverage, *bounds = tile_coverage(tile)
    bounds = tuple(int(v) for v in bounds)
    print("tile bounds", bounds, file=sys.stderr)
    icon = place(unmatte(rgb, coverage), bounds)
    icon.save(png_out)
    for size in ICONSET_SIZES:
        icon.resize((size, size), Image.LANCZOS).save(iconset / f"icon_{size}x{size}.png")
        pixels = size * RETINA_SCALE
        icon.resize((pixels, pixels), Image.LANCZOS).save(iconset / f"icon_{size}x{size}@2x.png")


main()
