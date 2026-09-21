#!/usr/bin/env python3
"""Hand-authored pixel art for pets/cat_black.json.

Poses are built from drawing primitives (see shapes.py) rather than from
hand-counted spans: at 64x64 an ellipse is both easier to reason about and far
harder to get subtly wrong.  Each body part is its own layer, shaded and rimmed
on its own, then composited -- so a leg lying over the body still reads as a
separate limb.

Light comes from the top left throughout: a top edge is `l`, a side edge `m`,
an underside `n`, and the form darkens `d` -> `n` -> `k` away from the light.

Run this to regenerate pets/cat_black.json.
"""

import json
import math
import os
import sys

# tools/cat/black.py -> tools/ (one level up) holds the shared shapes module
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from shapes import (  # noqa: E402
    HEIGHT, WIDTH, capsule, ellipse, flat, inset, layer, moved, put, shift, stack,
    triangle, union,
)

GROUND = 64
# Poses drawn before the frame grew are laid out for a 64x64 grid; this drops
# them into the middle of the wider one.
LEGACY = (8, 8)

PALETTE = {
    "k": "#0b0b0f",  # deepest shadow, the far side of the cat
    "d": "#1a1a1f",  # body
    "n": "#262630",  # mid tone: form shading, and the softened edge of curves
    "m": "#3a3a4a",  # rim light, sides
    "l": "#4e4e60",  # rim light, facing up into the light
    "w": "#e6e6e6",  # whiskers, teeth, the sleeping z
    "o": "#f2b23c",  # eye
    "p": "#b3808f",  # inner ear, nose
}


# --------------------------------------------------------------------------
# shared pieces
# --------------------------------------------------------------------------


def eye(cx, cy, look=0, open_=1.0):
    """One eye: orange almond, dark pupil, a highlight up and to the left."""
    if open_ <= 0:
        # closed: a lit crease, which is all you see of a shut eye on black fur
        return [(x, cy, "m") for x in range(cx - 4, cx + 5)] + [
            (cx - 5, cy - 1, "m"), (cx + 5, cy - 1, "m")
        ]
    ry = max(1, round(2 * open_))
    px = []
    for y, x0, x1 in ellipse(cx, cy, 3, ry):
        px += [(x, y, "o") for x in range(x0, x1 + 1)]
    # a small upright pupil, kept clear of the lids so the eye stays whole
    for dy in range(-ry + 1, ry):
        px.append((cx + look, cy + dy, "k"))
    if open_ > 0.6:
        px.append((cx - 2, cy - 1, "w"))
    return px


def ear(apex_x, apex_y, base_y, half, lean):
    """Outer ear shape plus the pink inside it."""
    outer = triangle((apex_x, apex_y), base_y, half, lean)
    pink = inset(outer, left=3, right=3, top=3, bottom=2)
    return outer, [(x, y, "p") for y, x0, x1 in pink for x in range(x0, x1 + 1)]


def whiskers(left_x, right_x, cy, reach, rows=(-2, 2)):
    """Fine lines springing outward from just beyond the cheeks."""
    px = []
    for dy in rows:
        for i in range(reach):
            px.append((left_x - i, cy + dy - (dy * i) // (reach * 2), "w"))
            px.append((right_x + i, cy + dy - (dy * i) // (reach * 2), "w"))
    return px


def leg(hip, knee, foot, r=4, near=True):
    """A jointed leg: hip -> knee -> ankle -> paw."""
    ankle = (round((knee[0] + foot[0]) / 2), round((knee[1] + foot[1]) / 2) + 2)
    shape = union(
        capsule(hip[0], hip[1], knee[0], knee[1], r),
        capsule(knee[0], knee[1], ankle[0], ankle[1], r - 1),
        capsule(ankle[0], ankle[1], foot[0], foot[1], r - 1),
        ellipse(foot[0] + 1, foot[1], r, 1),
    )
    if near:
        return layer(shape, light=(3, 2))
    # the far pair sits in shadow so the near pair reads in front of it
    return layer(shape, mid=shape, deep=moved(shape, 3, 2))


# --------------------------------------------------------------------------
# sit / idle -- front view, tail curled round the right side
# --------------------------------------------------------------------------

SIT_EAR_L, SIT_EAR_L_PINK = ear(20, 6, 21, 8, 2)
SIT_EAR_R, SIT_EAR_R_PINK = ear(43, 6, 21, 8, -2)

SIT_HEAD_SHAPE = union(ellipse(32, 28, 17, 13), SIT_EAR_L, SIT_EAR_R)

SIT_FACE = (
    SIT_EAR_L_PINK
    + SIT_EAR_R_PINK
    + [(32, 34, "p"), (31, 34, "p"), (33, 34, "p"), (32, 35, "p")]
    + [(30, 37, "m"), (34, 37, "m"), (31, 38, "m"), (33, 38, "m")]
)
SIT_WHISKERS = whiskers(13, 51, 34, 6)


DEFAULT_EARS = (union(SIT_EAR_L, SIT_EAR_R), SIT_EAR_L_PINK + SIT_EAR_R_PINK)
# pinned back: the whole silhouette says "leave me alone"
FLAT_EAR_L, FLAT_EAR_L_PINK = ear(22, 12, 20, 7, -9)
FLAT_EAR_R, FLAT_EAR_R_PINK = ear(41, 12, 20, 7, 9)
FLAT_EARS = (union(FLAT_EAR_L, FLAT_EAR_R), FLAT_EAR_L_PINK + FLAT_EAR_R_PINK)


def sit_head(look=0, open_=1.0, ears=None, tilt=0, extra=()):
    shape, pink = ears or DEFAULT_EARS
    face = [px for px in SIT_FACE if px not in SIT_EAR_L_PINK + SIT_EAR_R_PINK]
    head = layer(
        union(ellipse(32, 28, 17, 13), shape),
        light=(7, 6),
        features=face + pink + eye(25, 27, look, open_) + eye(39, 27, look, open_) + list(extra),
        outside=SIT_WHISKERS,
    )
    return shift(head, tilt, 0) if tilt else head


SIT_PAWS = layer(
    union(ellipse(23, 58, 7, 4), ellipse(41, 58, 7, 4)),
    light=(5, 4),
)


def sit_tail(lift=0):
    """Curls out to the right and up the flank."""
    shape = union(
        capsule(46, 60, 54, 58, 3),
        capsule(54, 58, 58, 51, 3),
        capsule(58, 51, 56, 42 - lift, 3),
        capsule(56, 42 - lift, 51, 37 - lift, 3),
        ellipse(49, 37 - lift, 3, 2),
    )
    return layer(shape, light=(3, 2))


def sit_frame(look=0, open_=1.0, breath=0, lift=0, ears=None, tilt=0, tail=None):
    body = layer(
        union(ellipse(32, 48, 20 + breath, 14), ellipse(32, 40 - breath, 14, 9)),
        light=(7, 6),
    )
    return stack(tail if tail is not None else sit_tail(lift), body, SIT_PAWS,
                 sit_head(look, open_, ears, tilt))


SIT_FRAMES = [
    sit_frame(),
    sit_frame(breath=1),
    sit_frame(breath=1, lift=1),
    sit_frame(lift=2),
    sit_frame(lift=2, open_=0.5),
    sit_frame(lift=1, open_=0.0),
    sit_frame(open_=0.5),
    sit_frame(),
    sit_frame(breath=1),
    sit_frame(lift=1),
]

# --------------------------------------------------------------------------
# walk -- a side profile, facing right; the renderer mirrors it for left
# --------------------------------------------------------------------------
#
# Body, neck, head and tail are a single layer, so the character has one
# unbroken contour: separate layers each carried their own outline, which put
# a seam across the throat and across the rump.  Only the legs are separate,
# because a limb does need an edge to read in front of the flank.
#
# Proportions follow the reference walk sheet: a long shallow body, a small
# head, and legs longer than the body is deep.

WALK_FRAMES_N = 16
# How far the cat travels, in sprite pixels, over one full cycle.  The renderer
# drives the frame from distance rather than from a clock, so a planted foot
# stays planted and the walk stops looking like skating.
WALK_ADVANCE = 22

GROUND_W = 67
FORE_X, HIND_X = 50, 26      # shoulder and hip, in sprite pixels
FAR = 6                      # how far the off-side legs sit behind the near

# Ears: a wide base tapering to a soft point, set on the skull where a cat
# carries them -- the near one forward, the far one behind and turned out.
EAR_NEAR = triangle((60, 4), 16, 8, 1)
EAR_FAR = triangle((72, 6), 17, 7, -1)


def eye_almond(cx, cy, open_=1.0):
    """A hand-shaped almond.  An ellipse this small comes out a rectangle."""
    if open_ <= 0.35:
        return [(x, cy, "m") for x in range(cx - 3, cx + 4)]
    rows = [(cx - 2, cx + 2), (cx - 3, cx + 3), (cx - 3, cx + 3), (cx - 2, cx + 2)]
    if open_ < 0.8:
        rows = rows[1:3]
    px = []
    for i, (x0, x1) in enumerate(rows):
        px += [(x, cy - 1 + i, "o") for x in range(x0, x1 + 1)]
    top, bottom = cy - 1, cy - 1 + len(rows) - 1
    # a narrow upright slit, kept off the lids so the eye stays one shape
    px += [(cx, y, "k") for y in range(top, bottom + 1)]
    px.append((cx - 2, top, "w"))
    return px


def walk_body(bob=0, sway=0):
    """Tail, haunch, flank, chest, neck, skull and muzzle as one silhouette."""
    shape = union(
        # tail, rooted in the rump so the contour flows out of the body
        capsule(16, 34, 9, 25, 4),
        capsule(9, 25, 11 + sway, 15, 3),
        capsule(11 + sway, 15, 19 + sway, 10, 2),
        ellipse(22 + sway, 11, 2, 2),
        # body
        ellipse(21, 35, 11, 10),      # rump
        capsule(23, 34, 46, 35, 8),   # flank
        ellipse(48, 36, 13, 11),      # ribcage
        capsule(55, 32, 61, 25, 8),   # neck
        # head
        ellipse(64, 21, 10, 9),       # skull
        ellipse(71, 26, 6, 4),        # muzzle
        ellipse(68, 29, 6, 3),        # jaw
        EAR_NEAR,
        EAR_FAR,
    )
    marks = (
        # inner ear
        [(x, y, "p") for y, x0, x1 in inset(EAR_NEAR, 4, 4, 5, 3) for x in range(x0, x1 + 1)]
        # the far ear is turned away, so it sits a tone darker
        + [(x, y, "k") for y, x0, x1 in inset(EAR_FAR, 1, 1, 1, 0) for x in range(x0, x1 + 1)]
        + eye_almond(67, 20)
        # nose, and a mouth that runs back from it under the cheek
        + [(76, 24, "p"), (77, 24, "p"), (76, 25, "p")]
        + [(75, 26, "k"), (74, 27, "k"), (73, 28, "k"), (72, 28, "k"), (71, 29, "k")]
        # cheek, shoulder blade and the crease at the top of the hind leg
        + [(69, 23, "n"), (69, 24, "n"), (68, 25, "n")]
    )
    whisk = [(77, 22, "w"), (78, 21, "w"),
             (77, 28, "w"), (78, 29, "w")]
    return shift(layer(shape, light=(4, 5), features=marks, outside=whisk), 0, bob)


# The gait: nine frames with the foot planted and travelling backwards, seven
# with it lifted and swinging forward again.
STANCE = 9
SWING = WALK_FRAMES_N - STANCE
LIFT = [1, 5, 8, 9, 8, 5, 2]


def foot_at(phase):
    """Foot offset from the hip, and how far it is off the ground."""
    if phase < STANCE:
        t = phase / (STANCE - 1)
        return WALK_ADVANCE / 2 - WALK_ADVANCE * t, 0
    i = phase - STANCE
    t = (i + 1) / SWING
    return -WALK_ADVANCE / 2 + WALK_ADVANCE * t, LIFT[i]


def limb(joints, radii, near):
    """A leg tapering down its length, ending in a paw with a toe split."""
    shape = []
    for i in range(len(joints) - 1):
        (x0, y0), (x1, y1) = joints[i], joints[i + 1]
        shape += capsule(x0, y0, x1, y1, radii[i])
    px, py = joints[-1]
    shape += union(capsule(px - 1, py, px + 2, py, 2), ellipse(px + 1, py - 1, 3, 1))
    if near:
        return layer(shape, light=(3, 3), features=[(px + 2, py + 1, "k")])
    # the off-side pair is in shadow: no bright edge, so it stays behind
    return layer(shape, mid=shape, deep=moved(shape, 2, 2))


def foreleg(phase, near):
    """Elbow, wrist, paw -- the upper arm stays inside the chest."""
    fx, lift = foot_at(phase)
    x = FORE_X + (0 if near else FAR)
    return limb(
        [
            (round(x + fx * 0.08), 42),
            (round(x + fx * 0.62), 56 - round(lift * 0.35)),
            (round(x + fx), GROUND_W - lift),
        ],
        radii=(4, 2) if near else (3, 2),
        near=near,
    )


def hindleg(phase, near):
    """Stifle, hock, foot: the backwards-bending joint a cat is known for."""
    fx, lift = foot_at(phase)
    x = HIND_X + (0 if near else FAR)
    return limb(
        [
            (x + 5, 40),
            (round(x - 3 + fx * 0.32), 55 - round(lift * 0.3)),
            (round(x + fx), GROUND_W - lift),
        ],
        radii=(5, 3) if near else (4, 2),
        near=near,
    )


WALK_FRAMES = []
for i in range(WALK_FRAMES_N):
    # the body lifts as each diagonal pair gathers under it, twice per cycle
    bob = -1 if math.sin(2 * math.pi * 2 * i / WALK_FRAMES_N) > 0.4 else 0
    sway = round(2 * math.sin(2 * math.pi * i / WALK_FRAMES_N))
    half = WALK_FRAMES_N // 2
    quarter = WALK_FRAMES_N // 4
    WALK_FRAMES.append(
        stack(
            hindleg((i + half) % WALK_FRAMES_N, near=False),
            foreleg((i + half + quarter) % WALK_FRAMES_N, near=False),
            hindleg(i, near=True),
            foreleg((i + quarter) % WALK_FRAMES_N, near=True),
            walk_body(bob, sway),
        )
    )

# --------------------------------------------------------------------------
# sleep -- curled up, z particles rising in the last frames
# --------------------------------------------------------------------------

SLEEP_EAR, SLEEP_EAR_PINK = ear(20, 26, 38, 7, 3)


def zed(x, y, size=5):
    px = []
    for i in range(size):
        px += [(x + i, y, "w"), (x + i, y + size - 1, "w")]
        px.append((x + size - 1 - i, y + i, "w"))
    return px


def sleep_frame(breath=0, zs=()):
    body = layer(
        union(ellipse(36, 46 - breath, 23, 15 + breath), ellipse(50, 50, 12, 11)),
        light=(5, 7),
    )
    head = layer(
        union(ellipse(17, 47, 13, 11), SLEEP_EAR),
        light=(7, 6),
        features=SLEEP_EAR_PINK
        + eye(15, 46, open_=0)
        + [(4, 50, "p"), (5, 50, "p"), (4, 51, "p")]
        ,
        outside=whiskers(2, 30, 50, 4, rows=(-1, 2)),
    )
    tail = layer(
        union(capsule(56, 56, 40, 60, 4), capsule(40, 60, 24, 59, 4),
              ellipse(22, 58, 4, 3)),
        light=(2, 3),
    )
    paws = layer(ellipse(10, 57, 8, 4), light=(3, 2))
    g = stack(body, tail, head, paws)
    for x, y in zs:
        g = put(g, zed(x, y))
    return g


SLEEP_FRAMES = [
    sleep_frame(0),
    sleep_frame(1),
    sleep_frame(2),
    sleep_frame(2),
    sleep_frame(1),
    sleep_frame(0),
    sleep_frame(0, zs=[(44, 26)]),
    sleep_frame(1, zs=[(44, 26), (52, 18)]),
    sleep_frame(2, zs=[(52, 18), (58, 9)]),
    sleep_frame(1, zs=[(58, 9)]),
]


# --------------------------------------------------------------------------
# extras -- short reactions the pet drops into between idling and walking
# --------------------------------------------------------------------------

LOOK_RIGHT = [sit_frame(look=l, tilt=t) for l, t in
              ((0, 0), (1, 1), (1, 1), (1, 1), (1, 1), (0, 0))]
LOOK_LEFT = [sit_frame(look=l, tilt=t) for l, t in
             ((0, 0), (-1, -1), (-1, -1), (-1, -1), (-1, -1), (0, 0))]

# head tips over to one side, the way a cat does when a noise makes no sense
CURIOUS = [sit_frame(look=l, tilt=t) for l, t in
           ((0, 0), (1, 1), (1, 2), (1, 3), (1, 3), (1, 2), (1, 1), (0, 0))]


def grumpy_tail(lash):
    shape = union(
        capsule(47, 59, 55, 51, 3),
        capsule(55, 51, 57 + lash, 34, 3),
        ellipse(57 + lash, 31, 3, 3),
    )
    return layer(shape, light=(4, 3))


GRUMPY = [sit_frame(open_=0.45, ears=FLAT_EARS, tail=grumpy_tail(l))
          for l in (0, 1, 2, 1, 0, -1, -2, -1)]


# --- seen from behind: all you get is a back and a raised tail --------------
BACK_EAR_L, BACK_EAR_L_PINK = ear(19, 6, 18, 7, 2)
BACK_EAR_R, BACK_EAR_R_PINK = ear(44, 6, 18, 7, -2)


def back_frame(sway):
    body = layer(
        union(
            ellipse(32, 24, 17, 10),
            ellipse(32, 44, 21, 18),
            ellipse(18, 58, 8, 4),
            ellipse(46, 58, 8, 4),
            BACK_EAR_L,
            BACK_EAR_R,
        ),
        light=(7, 6),
        features=BACK_EAR_L_PINK + BACK_EAR_R_PINK,
    )
    tail = layer(
        union(capsule(47, 52, 54, 42, 3),
              capsule(54, 42, 52 + sway, 26, 3),
              ellipse(51 + sway, 23, 3, 3)),
        light=(4, 3),
    )
    spine = [(32, y, "n") for y in range(18, 56)] + [(33, y, "k") for y in range(20, 54)]
    return put(stack(body, tail), spine)


BACK_VIEW = [back_frame(s) for s in (0, 1, 2, 1, 0, -1, -2, -1)]


# --- stretch: chest down, rear in the air -----------------------------------
STRETCH_EAR_L, STRETCH_EAR_L_PINK = ear(44, 22, 33, 6, 1)
STRETCH_EAR_R, STRETCH_EAR_R_PINK = ear(54, 22, 33, 6, -1)


def stretch_frame(reach):
    body = layer(
        union(
            ellipse(20, 36, 13, 11),
            ellipse(32, 44, 15, 10),
            ellipse(44, 50, 12, 8),
        ),
        light=(5, 6),
    )
    head = layer(
        union(ellipse(50, 42, 11, 9), ellipse(57, 45, 6, 5),
              STRETCH_EAR_L, STRETCH_EAR_R),
        light=(5, 5),
        features=STRETCH_EAR_L_PINK + STRETCH_EAR_R_PINK
        + [(62, 45, "p"), (61, 45, "p")]
        + eye(52, 41, 0, 0.7),
    )
    # front legs slide further forward as the stretch deepens
    front = layer(
        union(capsule(44, 52, 34 - reach, 59, 4), ellipse(30 - reach, 59, 6, 3)),
        light=(3, 3),
    )
    rear = stack(leg((18, 44), (16, 52), (15, 61), r=3, near=False),
                 leg((22, 45), (21, 52), (20, 61), r=3, near=True))
    tail = layer(
        union(capsule(10, 34, 6, 26, 3), capsule(6, 26, 9, 18, 3),
              ellipse(12, 16, 3, 2)),
        light=(3, 3),
    )
    return stack(tail, rear, body, front, head)


STRETCH = [stretch_frame(r) for r in (0, 2, 4, 6, 7, 7, 5, 2)]


# --- jump: crouch, launch, tuck, fall, land ---------------------------------
def jump_frame(dy, knee_y, foot_y):
    """Placeholder built on the new body; the jump gets its own pass next."""
    legs = [
        limb([(HIND_X + FAR + 5, 40), (HIND_X + FAR - 2, knee_y), (HIND_X + FAR, foot_y)],
             (4, 2), near=False),
        limb([(FORE_X + FAR, 42), (FORE_X + FAR + 3, knee_y), (FORE_X + FAR + 4, foot_y)],
             (3, 2), near=False),
        limb([(HIND_X + 5, 40), (HIND_X - 3, knee_y), (HIND_X, foot_y)], (5, 3), near=True),
        limb([(FORE_X, 42), (FORE_X + 3, knee_y), (FORE_X + 4, foot_y)], (4, 2), near=True),
    ]
    return shift(stack(*legs, walk_body(0, 1)), 0, dy)


JUMP = [
    jump_frame(2, 58, 67),         # crouch
    jump_frame(0, 57, 67),         # push off
    jump_frame(-6, 52, 60),        # tucked
    jump_frame(-10, 51, 58),       # apex
    jump_frame(-5, 54, 62),        # coming down
    jump_frame(2, 58, 67),         # land
]


# --- carried: scruffed, hanging, tail swinging ------------------------------
def carry_frame(sway):
    head = sit_head(0, 1.0, tilt=sway)
    body = layer(
        union(ellipse(32, 41, 14, 9), ellipse(32, 51, 12, 8)),
        light=(4, 3),
    )
    legs = stack(
        leg((26, 53), (24 - sway, 58), (23 - sway, 62), r=3, near=True),
        leg((38, 53), (40 + sway, 58), (41 + sway, 62), r=3, near=True),
    )
    tail = layer(
        union(capsule(43, 50, 50 + sway, 55, 3),
              capsule(50 + sway, 55, 54 + sway, 62, 3),
              ellipse(55 + sway, 63, 3, 2)),
        light=(3, 3),
    )
    return stack(tail, body, legs, head)


CARRY = [carry_frame(s) for s in (0, 1, 2, 1, 0, -1, -2, -1)]


# --- waking up: uncurl, sit up, yawn ----------------------------------------
def _yawn():
    px = []
    for y, x0, x1 in ellipse(32, 37, 5, 5):
        px += [(x, y, "k") for x in range(x0, x1 + 1)]
    for y, x0, x1 in ellipse(32, 39, 3, 2):
        px += [(x, y, "p") for x in range(x0, x1 + 1)]
    for y, x0, x1 in ellipse(32, 37, 6, 6):
        px += [(x0, y, "m"), (x1, y, "m")]
    return px


YAWN = _yawn()

WAKE_UP = [
    sleep_frame(0),
    sleep_frame(2),
    shift(sit_frame(open_=0.0), 0, 14),
    shift(sit_frame(open_=0.0), 0, 7),
    sit_frame(open_=0.0),
    put(sit_frame(open_=0.3), YAWN),
    sit_frame(open_=0.6),
    sit_frame(),
]

# --------------------------------------------------------------------------
# emit
# --------------------------------------------------------------------------

def centred(frames):
    """Poses drawn for the old square frame, dropped into the wider one."""
    return [shift(f, *LEGACY) for f in frames]


ANIMATIONS = {
    "sit": ("front", 8, True, centred(SIT_FRAMES), None),
    "walk": ("side", 14, True, WALK_FRAMES, WALK_ADVANCE),
    "sleep": ("side", 4, True, centred(SLEEP_FRAMES), None),
    "look_right": ("front", 6, False, centred(LOOK_RIGHT), None),
    "look_left": ("front", 6, False, centred(LOOK_LEFT), None),
    "curious": ("front", 7, False, centred(CURIOUS), None),
    "grumpy": ("front", 8, True, centred(GRUMPY), None),
    "back_view": ("front", 6, True, centred(BACK_VIEW), None),
    "stretch": ("side", 8, False, centred(STRETCH), None),
    "jump": ("side", 12, False, centred(JUMP), None),
    "carry": ("front", 7, True, centred(CARRY), None),
    "wake_up": ("front", 5, False, centred(WAKE_UP), None),
}


def main():
    pet = {
        "id": "cat_black",
        "name": "Classic",
        "description": "Her zaman yaninda. Sade, zarif ve zamansiz.",
        "species": "cat",
        "frame": {"width": WIDTH, "height": HEIGHT},
        "transparent": ".",
        "palette": PALETTE,
        "animations": {},
    }
    for name, (view, fps, loop, frames, advance) in ANIMATIONS.items():
        anim = {"view": view, "fps": fps, "loop": loop,
                "frames": [flat(f) for f in frames]}
        if advance:
            anim["advance"] = advance
        pet["animations"][name] = anim

    # tools/cat/black.py -> repo root is two levels up
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    out = os.path.join(root, "pets", "cat", "black.json")
    with open(out, "w") as fh:
        json.dump(pet, fh, indent=1)
        fh.write("\n")
    print(f"wrote {out}  ({os.path.getsize(out) // 1024} KB)")


if __name__ == "__main__":
    main()
