#!/usr/bin/env python3
"""Generate the original prop and modular kit models used by Dungeon Forge
kits (props in assets/models/props/, modular pieces in assets/models/kit/).

Writes glTF 2.0 binary files (.glb) with flat-shaded low-poly geometry and
plain PBR materials. Standard library only; the output is deterministic, so
the committed files can always be regenerated and compared.

    python scripts/make_prop_models.py

Provenance: written for Dungeon Sandbox (MIT OR Apache-2.0, like the code).
No third-party assets, textures or references were used.
"""

import json
import math
import os
import struct

OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "models", "props")


def ring(r, y, n=8, phase=math.pi / 8):
    return [(r * math.cos(phase + 2 * math.pi * k / n), y, r * math.sin(phase + 2 * math.pi * k / n)) for k in range(n)]


class Mesh:
    """Flat-shaded triangles: every face gets its own vertices and normal."""

    def __init__(self):
        self.pos, self.nrm, self.idx = [], [], []

    def tri(self, a, b, c):
        u = [b[i] - a[i] for i in range(3)]
        v = [c[i] - a[i] for i in range(3)]
        n = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
        ln = math.sqrt(sum(x * x for x in n)) or 1.0
        n = tuple(x / ln for x in n)
        base = len(self.pos)
        for p in (a, b, c):
            self.pos.append(p)
            self.nrm.append(n)
        self.idx += [base, base + 1, base + 2]

    def quad(self, a, b, c, d):
        self.tri(a, b, c)
        self.tri(a, c, d)

    def frustum(self, r0, y0, r1, y1, cap_top=True, cap_bottom=False, n=8):
        lo, hi = ring(r0, y0, n), ring(r1, y1, n)
        for k in range(n):
            j = (k + 1) % n
            # Counter-clockwise seen from outside.
            self.quad(lo[k], hi[k], hi[j], lo[j])
        if cap_top:
            c = (0.0, y1, 0.0)
            for k in range(n):
                self.tri(c, hi[(k + 1) % n], hi[k])
        if cap_bottom:
            c = (0.0, y0, 0.0)
            for k in range(n):
                self.tri(c, lo[k], lo[(k + 1) % n])

    def cone(self, r, y0, y1, n=8):
        lo = ring(r, y0, n)
        tip = (0.0, y1, 0.0)
        for k in range(n):
            self.tri(lo[k], tip, lo[(k + 1) % n])


def write_glb(path, parts):
    """parts: list of (name, Mesh, material dict)."""
    bin_data = bytearray()
    views, accessors, prims, materials = [], [], [], []

    def add_view(data, target):
        while len(bin_data) % 4:
            bin_data.append(0)
        views.append({"buffer": 0, "byteOffset": len(bin_data), "byteLength": len(data), "target": target})
        bin_data.extend(data)
        return len(views) - 1

    for name, mesh, mat in parts:
        pos = b"".join(struct.pack("<3f", *p) for p in mesh.pos)
        nrm = b"".join(struct.pack("<3f", *n) for n in mesh.nrm)
        idx = b"".join(struct.pack("<H", i) for i in mesh.idx)
        vp = add_view(pos, 34962)
        vn = add_view(nrm, 34962)
        vi = add_view(idx, 34963)
        mins = [round(min(p[i] for p in mesh.pos), 5) for i in range(3)]
        maxs = [round(max(p[i] for p in mesh.pos), 5) for i in range(3)]
        accessors.append({"bufferView": vp, "componentType": 5126, "count": len(mesh.pos), "type": "VEC3", "min": mins, "max": maxs})
        accessors.append({"bufferView": vn, "componentType": 5126, "count": len(mesh.nrm), "type": "VEC3"})
        accessors.append({"bufferView": vi, "componentType": 5123, "count": len(mesh.idx), "type": "SCALAR"})
        a = len(accessors)
        materials.append(dict(mat, name=name))
        prims.append({"attributes": {"POSITION": a - 3, "NORMAL": a - 2}, "indices": a - 1, "material": len(materials) - 1})

    while len(bin_data) % 4:
        bin_data.append(0)
    gltf = {
        "asset": {"version": "2.0", "generator": "Dungeon Sandbox make_prop_models.py", "copyright": "Dungeon Sandbox contributors, MIT OR Apache-2.0"},
        "extensionsUsed": ["KHR_materials_emissive_strength"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": os.path.splitext(os.path.basename(path))[0]}],
        "meshes": [{"primitives": prims}],
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(bin_data)}],
    }
    js = json.dumps(gltf, separators=(",", ":"), sort_keys=True).encode()
    while len(js) % 4:
        js += b" "
    total = 12 + 8 + len(js) + 8 + len(bin_data)
    with open(path, "wb") as f:
        f.write(struct.pack("<4sII", b"glTF", 2, total))
        f.write(struct.pack("<I4s", len(js), b"JSON") + js)
        f.write(struct.pack("<I4s", len(bin_data), b"BIN\x00") + bytes(bin_data))
    print(f"wrote {os.path.relpath(path)} ({total} bytes)")


def pbr(color, metallic=0.0, rough=0.9, emissive=None, strength=1.0):
    m = {"pbrMetallicRoughness": {"baseColorFactor": list(color) + [1.0], "metallicFactor": metallic, "roughnessFactor": rough}}
    if emissive:
        m["emissiveFactor"] = list(emissive)
        m["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": strength}}
    return m


def brazier():
    stone, iron, coals, flame = Mesh(), Mesh(), Mesh(), Mesh()
    stone.frustum(0.45, 0.0, 0.32, 0.15, cap_top=True)          # foot
    stone.frustum(0.22, 0.15, 0.18, 1.0, cap_top=False)         # stem
    iron.frustum(0.2, 0.95, 0.75, 1.35, cap_top=False, cap_bottom=True)  # bowl
    iron.frustum(0.75, 1.35, 0.8, 1.45, cap_top=False)          # rim
    coals.frustum(0.72, 1.3, 0.62, 1.38, cap_top=True)          # bed of coals
    flame.cone(0.45, 1.38, 2.2, n=6)                            # flame
    return [
        ("stone", stone, pbr((0.32, 0.29, 0.27))),
        ("iron", iron, pbr((0.12, 0.11, 0.1), metallic=0.8, rough=0.55)),
        ("coals", coals, pbr((0.3, 0.08, 0.02), emissive=(1.0, 0.35, 0.08), strength=3.0)),
        ("flame", flame, pbr((1.0, 0.6, 0.2), emissive=(1.0, 0.45, 0.1), strength=5.0)),
    ]


def shifted(make, dx, dz):
    """Build a part with `make(mesh)` and move it by (dx, dz)."""
    m = Mesh()
    make(m)
    m.pos = [(x + dx, y, z + dz) for (x, y, z) in m.pos]
    return m


def merge(target, part):
    base = len(target.pos)
    target.pos += part.pos
    target.nrm += part.nrm
    target.idx += [i + base for i in part.idx]


def candelabrum():
    """A tall iron candle stand for crypt pillars: four candles above head
    height so their flames show over the low walls round a pillar."""
    iron, wax, flame = Mesh(), Mesh(), Mesh()
    iron.frustum(0.5, 0.0, 0.35, 0.12, cap_top=True)                 # foot
    iron.frustum(0.08, 0.12, 0.07, 3.2, cap_top=False)               # shaft
    iron.frustum(0.75, 3.2, 0.78, 3.3, cap_top=True, cap_bottom=True)  # tray
    for (dx, dz) in [(0.0, 0.0), (0.55, 0.0), (-0.28, 0.48), (-0.28, -0.48)]:
        merge(wax, shifted(lambda m: m.frustum(0.09, 3.3, 0.08, 3.75, cap_top=True), dx, dz))
        merge(flame, shifted(lambda m: m.cone(0.07, 3.75, 4.0, n=5), dx, dz))
    return [
        ("iron", iron, pbr((0.09, 0.09, 0.1), metallic=0.85, rough=0.5)),
        ("wax", wax, pbr((0.85, 0.82, 0.72), rough=0.6)),
        ("flame", flame, pbr((1.0, 0.75, 0.4), emissive=(1.0, 0.62, 0.25), strength=6.0)),
    ]


# --------------------------------------------------------------------------
# Modular kit pieces (0.8). Shared frame for every piece:
#   x along the wall (or across a doorway), centred on 0;
#   y up from the floor (0);
#   z away from walkable space: the inner face of a wall is at z = 0 and the
#   body extends to +z. Nothing goes below z = -0.15 under 2.2 m (walk areas
#   keep actors 0.4 m from every wall), so art never covers floor.
# Wall-mounted lights hang from (0, 0, 0) on the wall face and reach -z.
# --------------------------------------------------------------------------

OUT_KIT = os.path.join(os.path.dirname(__file__), "..", "assets", "models", "kit")


def cuboid(m, corners):
    """Six faces from 8 corners ordered like a box: (x0,y0,z0) (x1,y0,z0)
    (x1,y1,z0) (x0,y1,z0) then the same at z1."""
    a0, b0, c0, d0, a1, b1, c1, d1 = corners
    m.quad(a0, d0, c0, b0)  # -z
    m.quad(a1, b1, c1, d1)  # +z
    m.quad(a0, b0, b1, a1)  # -y
    m.quad(d0, d1, c1, c0)  # +y
    m.quad(a0, a1, d1, d0)  # -x
    m.quad(b0, c0, c1, b1)  # +x


def box(m, x0, y0, z0, x1, y1, z1):
    cuboid(m, [
        (x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
        (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1),
    ])


def box_rot_z(m, cx, cy, hx, hy, z0, z1, angle):
    """A box rotated about the z axis around (cx, cy)."""
    ca, sa = math.cos(angle), math.sin(angle)

    def p(x, y, z):
        return (cx + x * ca - y * sa, cy + x * sa + y * ca, z)

    cuboid(m, [
        p(-hx, -hy, z0), p(hx, -hy, z0), p(hx, hy, z0), p(-hx, hy, z0),
        p(-hx, -hy, z1), p(hx, -hy, z1), p(hx, hy, z1), p(-hx, hy, z1),
    ])


def prism(m, cx, cz, r, y0, y1, n=6, phase=0.0):
    """Vertical n-gon prism (basalt column) at (cx, cz)."""
    pts = [(cx + r * math.cos(phase + 2 * math.pi * k / n), cz + r * math.sin(phase + 2 * math.pi * k / n)) for k in range(n)]
    for k in range(n):
        (ax, az), (bx, bz) = pts[k], pts[(k + 1) % n]
        m.quad((ax, y0, az), (ax, y1, az), (bx, y1, bz), (bx, y0, bz))
    c = (cx, y1, cz)
    for k in range(n):
        (ax, az), (bx, bz) = pts[k], pts[(k + 1) % n]
        m.tri(c, (bx, y1, bz), (ax, y1, az))


class Lcg:
    """Tiny deterministic generator (the output must never change by itself)."""

    def __init__(self, seed):
        self.s = seed & 0xFFFFFFFF

    def next(self):
        self.s = (self.s * 1664525 + 1013904223) & 0xFFFFFFFF
        return self.s / 4294967296.0


# ---- crypt: dressed limestone, ashlar courses, round arches --------------

CRYPT_STONE = pbr((0.47, 0.46, 0.43), rough=0.92)
CRYPT_STONE_DARK = pbr((0.30, 0.30, 0.29), rough=0.95)
CRYPT_JOINT = pbr((0.13, 0.13, 0.13), rough=1.0)


def ashlar(stone, joint, x0, x1, y0, y1, depth, course=0.5, seed=1, face=0.0):
    """Courses of blocks with recessed joints, inner face at z = face."""
    rng = Lcg(seed)
    box(joint, x0, y0, face + 0.06, x1, y1, depth)  # mortar bed behind the blocks
    y = y0
    row = 0
    while y < y1 - 1e-6:
        h = min(course, y1 - y)
        x = x0 - (0.45 if row % 2 else 0.0)
        while x < x1 - 1e-6:
            w = 0.95 + rng.next() * 0.5
            bx0, bx1 = max(x, x0), min(x + w, x1)
            if bx1 - bx0 > 0.08:
                inset = 0.02 + rng.next() * 0.03
                box(stone, bx0 + 0.025, y + 0.025, face + inset, bx1 - 0.025, y + h - 0.025, face + 0.3)
            x += w
        y += h
        row += 1


def crypt_wall(niche=False):
    """4 m long, 4.5 m tall, 0.7 m deep. A plinth, ashlar courses and a
    cornice. The niche variant has a recessed arched niche with a candle."""
    stone, dark, joint, wax, flame = Mesh(), Mesh(), Mesh(), Mesh(), Mesh()
    box(dark, -2.0, 0.0, 0.0, 2.0, 0.45, 0.7)                 # plinth
    if niche:
        ashlar(stone, joint, -2.0, -0.75, 0.45, 4.1, 0.7, seed=7)
        ashlar(stone, joint, 0.75, 2.0, 0.45, 4.1, 0.7, seed=8)
        ashlar(stone, joint, -0.75, 0.75, 0.45, 1.0, 0.7, seed=9)
        ashlar(stone, joint, -0.75, 0.75, 3.1, 4.1, 0.7, seed=10)
        box(dark, -0.75, 1.0, 0.45, 0.75, 3.1, 0.7)             # niche back
        box(dark, -0.8, 0.95, 0.0, 0.8, 1.05, 0.45)             # sill
        for k in range(7):                                       # arch head
            a = math.pi * k / 6
            box_rot_z(stone, 0.75 * math.cos(a), 2.6 + 0.75 * math.sin(a), 0.14, 0.2, 0.02, 0.45, a + math.pi / 2)
        for dx in (-0.25, 0.0, 0.25):
            merge(wax, shifted(lambda m: m.frustum(0.05, 1.05, 0.045, 1.3 + abs(dx), cap_top=True, n=6), dx, 0.25))
            merge(flame, shifted(lambda m: m.cone(0.04, 1.3 + abs(dx), 1.42 + abs(dx), n=5), dx, 0.25))
    else:
        ashlar(stone, joint, -2.0, 2.0, 0.45, 4.1, 0.7, seed=3)
    box(dark, -2.0, 4.1, -0.12, 2.0, 4.5, 0.7)                 # cornice (above heads)
    parts = [
        ("stone", stone, CRYPT_STONE),
        ("plinth", dark, CRYPT_STONE_DARK),
        ("joints", joint, CRYPT_JOINT),
    ]
    if niche:
        parts += [
            ("wax", wax, pbr((0.85, 0.82, 0.72), rough=0.6)),
            ("flame", flame, pbr((1.0, 0.75, 0.4), emissive=(1.0, 0.62, 0.25), strength=6.0)),
        ]
    return parts


def crypt_corner():
    """A square pilaster at a wall joint: 0.9 m wide, 4.8 m tall."""
    stone, dark = Mesh(), Mesh()
    box(dark, -0.5, 0.0, -0.5, 0.5, 0.55, 0.5)
    box(stone, -0.4, 0.55, -0.4, 0.4, 4.35, 0.4)
    box(dark, -0.5, 4.35, -0.5, 0.5, 4.8, 0.5)
    return [("stone", stone, CRYPT_STONE), ("plinth", dark, CRYPT_STONE_DARK)]


def crypt_arch():
    """A round-arched door frame for a 4 m opening: jambs outside |x| = 2,
    springing at 2.6 m, apex 4.6 m; reaches 0.12 m into the room and 0.8 m
    into the passage."""
    stone, dark = Mesh(), Mesh()
    for s in (-1, 1):
        box(dark, min(s * 2.0, s * 2.7), 0.0, -0.12, max(s * 2.0, s * 2.7), 0.5, 0.8)
        box(stone, min(s * 2.0, s * 2.6), 0.5, -0.1, max(s * 2.0, s * 2.6), 2.6, 0.78)
        box(dark, min(s * 2.0, s * 2.75), 2.45, -0.12, max(s * 2.0, s * 2.75), 2.65, 0.8)  # impost
    n = 11
    for k in range(n):
        a = math.pi * (k + 0.5) / n
        r = 2.3
        box_rot_z(stone, r * math.cos(a), 2.6 + r * math.sin(a), 0.34, 0.3, -0.1, 0.78, a + math.pi / 2)
    box(dark, -0.32, 4.6, -0.12, 0.32, 5.15, 0.8)                # keystone
    box(stone, -2.7, 4.7, -0.05, 2.7, 5.2, 0.75)                 # wall over the arch
    return [("stone", stone, CRYPT_STONE), ("trim", dark, CRYPT_STONE_DARK)]


def crypt_pillar():
    """A square pillar filling a 1 m hole (scaled to the real hole): base,
    chamfered shaft, capital; 7 m tall."""
    stone, dark = Mesh(), Mesh()
    box(dark, -0.5, 0.0, -0.5, 0.5, 0.6, 0.5)
    h = 0.43
    c = 0.1
    # Chamfered shaft: an octagon prism inside the square.
    pts = [(-h + c, -h), (h - c, -h), (h, -h + c), (h, h - c), (h - c, h), (-h + c, h), (-h, h - c), (-h, -h + c)]
    for k in range(8):
        (ax, az), (bx, bz) = pts[k], pts[(k + 1) % 8]
        stone.quad((ax, 0.6, az), (ax, 6.3, az), (bx, 6.3, bz), (bx, 0.6, bz))
    box(dark, -0.5, 6.3, -0.5, 0.5, 7.0, 0.5)
    box(stone, -0.46, 6.15, -0.46, 0.46, 6.3, 0.46)
    return [("stone", stone, CRYPT_STONE), ("trim", dark, CRYPT_STONE_DARK)]


def crypt_sconce():
    """An iron wall bracket with three candles; hangs from the wall face."""
    iron, wax, flame = Mesh(), Mesh(), Mesh()
    box(iron, -0.12, -0.25, -0.04, 0.12, 0.2, 0.0)              # wall plate
    box(iron, -0.03, -0.05, -0.42, 0.03, 0.01, -0.04)           # arm
    box(iron, -0.22, 0.0, -0.5, 0.22, 0.04, -0.3)               # tray
    for dx in (-0.14, 0.0, 0.14):
        merge(wax, shifted(lambda m: m.frustum(0.04, 0.04, 0.035, 0.22 + abs(dx) * 0.6, cap_top=True, n=6), dx, -0.4))
        merge(flame, shifted(lambda m: m.cone(0.035, 0.22 + abs(dx) * 0.6, 0.33 + abs(dx) * 0.6, n=5), dx, -0.4))
    return [
        ("iron", iron, pbr((0.09, 0.09, 0.1), metallic=0.85, rough=0.5)),
        ("wax", wax, pbr((0.85, 0.82, 0.72), rough=0.6)),
        ("flame", flame, pbr((1.0, 0.75, 0.4), emissive=(1.0, 0.62, 0.25), strength=6.0)),
    ]


# ---- volcanic: columnar basalt, iron-bound forge arches -------------------

BASALT = pbr((0.11, 0.095, 0.09), rough=0.96)
BASALT_LIGHT = pbr((0.16, 0.13, 0.12), rough=0.94)
FORGE_IRON = pbr((0.11, 0.1, 0.1), metallic=0.8, rough=0.5)
EMBER = pbr((0.35, 0.08, 0.02), emissive=(1.0, 0.38, 0.08), strength=4.0)


def basalt_wall():
    """4 m long, up to 5.4 m tall: packed, overlapping hexagonal basalt
    columns of varying height in front of a rough rock mass; flat column
    faces flush with z = 0."""
    a, b, back = Mesh(), Mesh(), Mesh()
    rng = Lcg(42)
    # Rock mass behind the columns: rough blocks of varying height.
    x = -2.0
    while x < 2.0 - 1e-6:
        w = min(0.7 + rng.next() * 0.5, 2.0 - x)
        top = 3.4 + rng.next() * 1.2
        box(back, x, 0.0, 0.45, x + w, top, 1.0)
        x += w
    x = -2.0
    k = 0
    while x < 2.0 - 0.05:
        r = 0.3 + rng.next() * 0.12
        w = 2 * r * math.cos(math.pi / 6)                       # flat-to-flat
        cx = min(x + w / 2, 2.0 - w / 2 + 0.05)
        top = 4.0 + rng.next() * 1.4
        # A hexagon with a flat face toward -z: centre at z = flat distance.
        prism(a if k % 3 else b, cx, r * math.cos(math.pi / 6), r, 0.0, top, n=6, phase=math.pi / 6 + math.pi / 2)
        x += w * 0.82                                           # overlapping
        k += 1
    return [("basalt", a, BASALT), ("basalt_light", b, BASALT_LIGHT), ("rock", back, pbr((0.12, 0.1, 0.095), rough=1.0))]


def basalt_corner():
    """A cluster of tall basalt columns at a wall joint (about 1.4 m wide)."""
    a, b = Mesh(), Mesh()
    for (dx, dz, r, top, m) in [(0.0, 0.0, 0.45, 6.0, a), (0.45, 0.35, 0.33, 5.1, b), (-0.4, 0.38, 0.3, 4.6, b), (0.1, -0.45, 0.28, 4.2, a)]:
        prism(m, dx, dz, r, 0.0, top, n=6, phase=0.3)
    return [("basalt", a, BASALT), ("basalt_light", b, BASALT_LIGHT)]


def forge_arch():
    """A forge gate for a 5 m opening: basalt piers outside |x| = 2.5, an
    iron lintel at 3.6-4.3 m with a glowing seam; 0.12 m into the room."""
    stone, iron, ember = Mesh(), Mesh(), Mesh()
    rng = Lcg(5)
    for s in (-1, 1):
        y = 0.0
        while y < 3.6:
            h = 0.6 + rng.next() * 0.35
            h = min(h, 3.6 - y)
            jitter = rng.next() * 0.12
            x0, x1 = s * 2.5, s * (3.25 + jitter)
            box(stone, min(x0, x1), y, -0.12, max(x0, x1), y + h - 0.04, 0.95)
            y += h
        box(iron, min(s * 2.45, s * 3.3), 1.6, -0.15, max(s * 2.45, s * 3.3), 1.8, 0.98)   # band
    box(iron, -3.4, 3.6, -0.15, 3.4, 4.3, 0.98)                  # lintel
    box(ember, -2.9, 3.86, -0.17, 2.9, 3.98, 0.0)                # glowing seam
    for k in range(9):                                           # rivets
        x = -3.0 + k * 0.75
        box(iron, x - 0.07, 4.08, -0.2, x + 0.07, 4.22, -0.15)
    box(stone, -3.5, 4.3, -0.1, 3.5, 5.0, 0.95)                  # rubble above
    return [("stone", stone, BASALT_LIGHT), ("iron", iron, FORGE_IRON), ("ember", ember, EMBER)]


def forge_lantern():
    """An iron cage with an ember core, hanging from a wall bracket."""
    iron, ember = Mesh(), Mesh()
    box(iron, -0.1, -0.2, -0.05, 0.1, 0.25, 0.0)                 # plate
    box(iron, -0.03, 0.15, -0.45, 0.03, 0.21, -0.05)             # arm
    for (dx, dz) in [(-0.14, -0.31), (0.14, -0.31), (-0.14, -0.59), (0.14, -0.59)]:
        box(iron, dx - 0.02, -0.35, dz - 0.02, dx + 0.02, 0.15, dz + 0.02)
    box(iron, -0.17, 0.12, -0.62, 0.17, 0.17, -0.28)             # cage top
    box(iron, -0.17, -0.38, -0.62, 0.17, -0.33, -0.28)           # cage bottom
    ember.frustum(0.1, -0.33, 0.06, 0.05, cap_top=True, n=6)
    ember.pos = [(x, y, z - 0.45) for (x, y, z) in ember.pos]
    return [("iron", iron, FORGE_IRON), ("ember", ember, pbr((0.9, 0.35, 0.08), emissive=(1.0, 0.45, 0.1), strength=8.0))]


def write_kit():
    os.makedirs(OUT_KIT, exist_ok=True)
    for name, parts in [
        ("crypt_wall", crypt_wall()),
        ("crypt_wall_niche", crypt_wall(niche=True)),
        ("crypt_corner", crypt_corner()),
        ("crypt_arch", crypt_arch()),
        ("crypt_pillar", crypt_pillar()),
        ("crypt_sconce", crypt_sconce()),
        ("basalt_wall", basalt_wall()),
        ("basalt_corner", basalt_corner()),
        ("forge_arch", forge_arch()),
        ("forge_lantern", forge_lantern()),
    ]:
        write_glb(os.path.join(OUT_KIT, name + ".glb"), parts)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    write_glb(os.path.join(OUT, "forge_brazier.glb"), brazier())
    write_glb(os.path.join(OUT, "crypt_candelabrum.glb"), candelabrum())
    write_kit()
