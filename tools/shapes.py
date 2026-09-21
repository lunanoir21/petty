#!/usr/bin/env python3
"""Drawing primitives for the pet sprites.

At 64x64 counting spans by hand stops being practical, so poses are built from
ellipses, triangles and capsules -- the shapes you would actually reach for
when drawing a cat -- and then shaded and rimmed.

A "span" is (y, x0, x1) inclusive.  A "grid" is HEIGHT rows of WIDTH chars.
"""

# A cat in profile is much longer than it is tall, so the frame is not square.
WIDTH = 80
HEIGHT = 72
TRANSPARENT = "."

# Light comes from the top left, so an edge gets a different tone depending on
# which way it faces.
TOP = "l"
SIDE = "m"
UNDER = "n"


# --------------------------------------------------------------------------
# shapes -> spans
# --------------------------------------------------------------------------


def ellipse(cx, cy, rx, ry, y0=None, y1=None):
    """Filled ellipse, optionally clipped to a band of rows."""
    spans = []
    for y in range(int(cy - ry), int(cy + ry) + 1):
        if y0 is not None and y < y0:
            continue
        if y1 is not None and y > y1:
            continue
        dy = (y - cy) / ry
        if abs(dy) > 1:
            continue
        half = rx * (1 - dy * dy) ** 0.5
        spans.append((y, round(cx - half), round(cx + half)))
    return spans


def triangle(apex, base_y, half_w, lean=0):
    """An ear: a triangle from a point down to a base, optionally leaning."""
    ax, ay = apex
    spans = []
    steps = base_y - ay
    for i in range(steps + 1):
        t = i / max(steps, 1)
        cx = ax + lean * t
        half = half_w * t
        spans.append((ay + i, round(cx - half), round(cx + half)))
    return spans


def capsule(x0, y0, x1, y1, r):
    """A thick line with rounded ends -- a leg segment, or a length of tail."""
    spans = {}
    steps = max(abs(x1 - x0), abs(y1 - y0), 1)
    for i in range(steps + 1):
        t = i / steps
        cx, cy = x0 + (x1 - x0) * t, y0 + (y1 - y0) * t
        for y in range(round(cy - r), round(cy + r) + 1):
            dy = y - cy
            if abs(dy) > r:
                continue
            half = (r * r - dy * dy) ** 0.5
            lo, hi = round(cx - half), round(cx + half)
            if y in spans:
                spans[y] = (min(spans[y][0], lo), max(spans[y][1], hi))
            else:
                spans[y] = (lo, hi)
    return [(y, lo, hi) for y, (lo, hi) in sorted(spans.items())]


def union(*groups):
    out = []
    for g in groups:
        out.extend(g)
    return out


def moved(spans, dx=0, dy=0):
    return [(y + dy, x0 + dx, x1 + dx) for y, x0, x1 in spans]


def inset(spans, left=0, right=0, top=0, bottom=0):
    """Shrink a shape, for an inner ear or a shadow that follows a form."""
    rows = sorted({y for y, _, _ in spans})
    keep = set(rows[top:len(rows) - bottom or None])
    return [
        (y, x0 + left, x1 - right)
        for y, x0, x1 in spans
        if y in keep and x0 + left <= x1 - right
    ]


# --------------------------------------------------------------------------
# spans -> grid
# --------------------------------------------------------------------------


def blank():
    return [[TRANSPARENT] * WIDTH for _ in range(HEIGHT)]


def fill(g, spans, ch):
    for y, x0, x1 in spans:
        if not 0 <= y < HEIGHT:
            continue
        for x in range(max(x0, 0), min(x1, WIDTH - 1) + 1):
            g[y][x] = ch
    return g


def put(g, pixels):
    out = [row[:] for row in g]
    for x, y, ch in pixels:
        if 0 <= x < WIDTH and 0 <= y < HEIGHT:
            out[y][x] = ch
    return out


def opaque(g, x, y):
    return 0 <= x < WIDTH and 0 <= y < HEIGHT and g[y][x] != TRANSPARENT


def rim(g):
    """A 1px lit edge, its tone set by which way the edge faces."""
    out = [row[:] for row in g]
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if g[y][x] == TRANSPARENT:
                continue
            up, down = opaque(g, x, y - 1), opaque(g, x, y + 1)
            side = not opaque(g, x - 1, y) or not opaque(g, x + 1, y)
            if not up:
                out[y][x] = TOP
            elif not down:
                out[y][x] = UNDER
            elif side:
                out[y][x] = SIDE
    return out


def soften(g):
    """Classic pixel-art anti-aliasing: dim the notch of every staircase.

    Only fires in the concave step of a diagonal edge, so straight edges stay
    crisp and the silhouette simply stops looking like stairs once the sprite
    is blown up on screen.
    """
    out = [row[:] for row in g]
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if g[y][x] != TRANSPARENT:
                continue
            horizontal = opaque(g, x - 1, y) or opaque(g, x + 1, y)
            vertical = opaque(g, x, y - 1) or opaque(g, x, y + 1)
            if horizontal and vertical:
                out[y][x] = UNDER
    return out


def shade_only(g, spans, ch):
    """Paint a shadow, but only where the part actually is.

    Shading with a free-floating ellipse punches holes in a face; a shadow has
    to be clipped to the form it sits on.
    """
    for y, x0, x1 in spans:
        if not 0 <= y < HEIGHT:
            continue
        for x in range(max(x0, 0), min(x1, WIDTH - 1) + 1):
            if g[y][x] != TRANSPARENT:
                g[y][x] = ch
    return g


def ease(g):
    """One step of mid tone wherever body meets deep shadow, so the terminator
    is a gradient rather than a cut-out."""
    out = [row[:] for row in g]
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if g[y][x] != "d":
                continue
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                if opaque(g, x + dx, y + dy) and g[y + dy][x + dx] == "k":
                    out[y][x] = "n"
                    break
    return out


def shade_form(g, light):
    """Form shading, derived from the silhouette itself.

    A pixel is lit when the body carries on in the direction of the light and
    in shadow when it does not -- so the shaded band is exactly the crescent
    along the edge that faces away from the light, whatever the shape.
    """
    dx, dy = light
    inner = (round(dx * 0.45), round(dy * 0.45))
    out = [row[:] for row in g]
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if g[y][x] == TRANSPARENT:
                continue
            if not opaque(g, x + dx, y + dy):
                out[y][x] = "n"
            if not opaque(g, x + inner[0], y + inner[1]):
                out[y][x] = "k"
    return out


def mark(g, pixels):
    """Paint detail, but only on the part itself.

    Markings that spill past the silhouette read as stray lines floating in
    the background, so a feature is clipped to the shape it belongs to.
    """
    out = [row[:] for row in g]
    for x, y, ch in pixels:
        if opaque(g, x, y):
            out[y][x] = ch
    return out


def layer(shape, mid=(), deep=(), features=(), light=None, outside=()):
    """One body part: fill, shade, rim, then mark it.

    Pass `light=(dx, dy)` to shade from the silhouette, or give explicit
    `mid`/`deep` spans for a shadow the form alone would not produce.
    `features` are clipped to the part; `outside` (whiskers) is not.
    """
    g = blank()
    fill(g, shape, "d")
    if light is not None:
        g = shade_form(g, light)
    shade_only(g, mid, "n")
    shade_only(g, deep, "k")
    g = ease(g)
    g = rim(g)
    return put(mark(g, features), outside)


def stack(*layers):
    """Composite bottom-up; a layer's opaque pixels win."""
    out = blank()
    for lay in layers:
        for y in range(HEIGHT):
            for x in range(WIDTH):
                if lay[y][x] != TRANSPARENT:
                    out[y][x] = lay[y][x]
    return out


def mirror(g):
    return [list(reversed(row)) for row in g]


def shift(g, dx=0, dy=0):
    out = blank()
    for y in range(HEIGHT):
        for x in range(WIDTH):
            nx, ny = x + dx, y + dy
            if 0 <= nx < WIDTH and 0 <= ny < HEIGHT:
                out[ny][nx] = g[y][x]
    return out


def flat(g):
    return ["".join(row) for row in g]
