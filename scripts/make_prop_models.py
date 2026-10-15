#!/usr/bin/env python3
"""Generate the original prop models used by Dungeon Forge kits.

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


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    write_glb(os.path.join(OUT, "forge_brazier.glb"), brazier())
    write_glb(os.path.join(OUT, "crypt_candelabrum.glb"), candelabrum())
