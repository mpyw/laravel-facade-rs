"""Generates assets/logo.svg: an "R" built from isometric line-art blocks.

Run it from the repository root:

    python3 assets/generate_logo.py

The background stays transparent. Each block hides the lines behind it with
an SVG mask instead of a white fill, so the logo also works on dark pages.
"""

import math

# Boxes as (name, x0, x1, y0, y1, z0, z1). The letter lies in the x-z plane:
# x runs to the right (down-right on screen), z runs up, y is depth.
D = 1.0
BOXES = [
    ("stem",   0, 1, 0, D, 0, 7),
    ("top",    1, 3, 0, D, 6, 7),
    ("bowl",   3, 4, 0, D, 4, 7),
    ("middle", 1, 3, 0, D, 3, 4),
    # The leg is a staircase of cubes, after the lone cube in Laravel's logo.
    ("leg1",   2, 3, 0, D, 2, 3),
    ("leg2",   3, 4, 0, D, 1, 2),
    ("leg3",   4, 5, 0, D, 0, 1),
]

S = 40.0
C, SN = math.cos(math.radians(30)), math.sin(math.radians(30))

def proj(x, y, z):
    return ((x - y) * C * S, (x + y) * SN * S - z * S)

def faces(b):
    _, x0, x1, y0, y1, z0, z1 = b
    top   = [(x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    front = [(x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1)]  # +y face, letter face
    right = [(x1, y0, z0), (x1, y1, z0), (x1, y1, z1), (x1, y0, z1)]  # +x face
    return [[proj(*p) for p in f] for f in (top, front, right)]

def key(b):
    _, x0, x1, y0, y1, z0, z1 = b
    return x0 + y0 + z0

boxes = sorted(BOXES, key=key)
pts = [p for b in boxes for f in faces(b) for p in f]
minx = min(p[0] for p in pts); maxx = max(p[0] for p in pts)
miny = min(p[1] for p in pts); maxy = max(p[1] for p in pts)
pad = 0.12 * max(maxx - minx, maxy - miny)
W = maxx - minx + 2 * pad; H = maxy - miny + 2 * pad
side = max(W, H)
ox = -minx + pad + (side - W) / 2
oy = -miny + pad + (side - H) / 2

def poly(f):
    return " ".join(f"{x + ox:.2f},{y + oy:.2f}" for x, y in f)

stroke = 0.12 * S
out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side:.2f} {side:.2f}" width="512" height="512">',
       "<defs>"]
for i, b in enumerate(boxes):
    out.append(f'<mask id="m{i}" maskUnits="userSpaceOnUse" x="0" y="0" width="{side:.2f}" height="{side:.2f}">')
    out.append(f'<rect x="0" y="0" width="{side:.2f}" height="{side:.2f}" fill="white"/>')
    for later in boxes[i + 1:]:
        for f in faces(later):
            out.append(f'<polygon points="{poly(f)}" fill="black"/>')
    out.append("</mask>")
out.append("</defs>")
out.append(f'<g fill="none" stroke="#FF2D20" stroke-width="{stroke:.2f}" stroke-linejoin="round" stroke-linecap="round">')
for i, b in enumerate(boxes):
    out.append(f'<g mask="url(#m{i})">')
    for f in faces(b):
        out.append(f'<polygon points="{poly(f)}"/>')
    out.append("</g>")
out.append("</g></svg>")
open("assets/logo.svg", "w").write("\n".join(out))
