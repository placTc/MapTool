"""Generate examples/demo.png: a small province map that passes MapTool's input rules.

Voronoi-style provinces with blocky border noise, plus a square enclave, a
rectangular province and a multi-pixel exclave. Seeds are retried until the
map has no single-pixel exclaves and no four-way junctions.

    python3 examples/make_demo.py
"""

import random
import struct
import zlib
from pathlib import Path

W, H = 240, 160
OUT = Path(__file__).parent / "demo.png"


def build(seed: int) -> list[list[tuple[int, int, int]]]:
    rng = random.Random(seed)
    seeds = [(rng.randrange(W), rng.randrange(H)) for _ in range(14)]
    colors = [tuple(rng.randrange(40, 255) for _ in range(3)) for _ in seeds]
    # Border noise is constant over 3x3 blocks so borders wobble without producing lone pixels.
    noise = {}

    def jitter(x: int, y: int, i: int) -> float:
        key = (x // 3, y // 3, i)
        if key not in noise:
            noise[key] = rng.random() * 60
        return noise[key]

    rows = []
    for y in range(H):
        row = []
        for x in range(W):
            best = min(range(len(seeds)), key=lambda i: (x - seeds[i][0]) ** 2 + (y - seeds[i][1]) ** 2 + jitter(x, y, i))
            row.append(colors[best])
        rows.append(row)

    def paint(x0: int, y0: int, w: int, h: int, color: tuple[int, int, int]) -> None:
        for y in range(y0, y0 + h):
            for x in range(x0, x0 + w):
                rows[y][x] = color

    paint(100, 60, 12, 12, (255, 255, 255))  # square enclave
    paint(20, 20, 40, 9, (250, 200, 40))  # rectangular province
    paint(190, 120, 8, 8, colors[0])  # exclave: same color as the first seed's province
    return rows


def png(rows: list[list[tuple[int, int, int]]]) -> bytes:
    raw = b"".join(b"\x00" + b"".join(bytes(px) for px in row) for row in rows)

    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def problems(rows: list[list[tuple[int, int, int]]]) -> list[str]:
    """The two input rules: no lone pixel of a color that appears elsewhere, and no
    corner where four different colors meet."""
    found = []
    counts: dict[tuple[int, int, int], int] = {}
    for row in rows:
        for px in row:
            counts[px] = counts.get(px, 0) + 1
    for y in range(H):
        for x in range(W):
            px = rows[y][x]
            same = (
                (x > 0 and rows[y][x - 1] == px)
                or (x + 1 < W and rows[y][x + 1] == px)
                or (y > 0 and rows[y - 1][x] == px)
                or (y + 1 < H and rows[y + 1][x] == px)
            )
            if counts[px] > 1 and not same:
                found.append(f"single-pixel exclave at ({x}, {y})")
            if x > 0 and y > 0 and len({px, rows[y][x - 1], rows[y - 1][x], rows[y - 1][x - 1]}) == 4:
                found.append(f"four-way junction at corner ({x}, {y})")
    return found


def main() -> None:
    for seed in range(1, 200):
        rows = build(seed)
        found = problems(rows)
        if found:
            print(f"seed {seed}: rejected, {len(found)} problem(s), e.g. {found[0]}")
            continue
        OUT.write_bytes(png(rows))
        print(f"seed {seed}: ok -> {OUT}")
        return
    raise SystemExit("no valid seed found")


if __name__ == "__main__":
    main()
