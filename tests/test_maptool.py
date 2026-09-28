import struct
import zlib

import pytest

import maptool


def png_bytes(rows: list[list[tuple[int, int, int]]]) -> bytes:
    h, w = len(rows), len(rows[0])
    raw = b"".join(b"\x00" + b"".join(bytes(px) for px in row) for row in rows)

    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


RED, BLUE = (255, 0, 0), (0, 0, 255)


def test_vectorize_bytes():
    data = bytes(RED * 2 + BLUE * 2) * 2
    m = maptool.vectorize(data, 4, 2, channels=3, tolerance=0)
    assert (m.width, m.height, len(m)) == (4, 2, 2)
    red, blue = m.provinces
    assert red.color == RED and red.pixel_count == 4 and red.bbox == (0, 0, 2, 2)
    assert blue.color == BLUE and blue.bbox == (2, 0, 4, 2)
    assert red.path.startswith("M") and red.path.endswith("Z")
    assert m[-1].id == 1


def test_svg_output():
    m = maptool.vectorize(bytes(RED + BLUE), 2, 1, channels=3)
    svg = m.to_svg()
    assert svg.startswith("<svg") and 'data-id="1"' in svg and 'fill="#0000ff"' in svg


def test_rgba_transparent_pixels_are_ignored():
    data = bytes([0, 0, 0, 0, 255, 0, 0, 255])
    m = maptool.vectorize(data, 2, 1, channels=4)
    assert len(m) == 1 and m[0].pixel_count == 1


def test_file_round_trip(tmp_path):
    rows = [[RED, RED, BLUE], [RED, BLUE, BLUE]]
    f = tmp_path / "map.png"
    f.write_bytes(png_bytes(rows))
    m = maptool.vectorize_file(f, tolerance=0)
    assert len(m) == 2 and m[0].pixel_count == 3


def test_errors():
    with pytest.raises(ValueError, match="expected"):
        maptool.vectorize(b"\x00" * 5, 2, 2, channels=3)
    with pytest.raises(ValueError, match="channels"):
        maptool.vectorize(b"", 1, 1, channels=2)
    with pytest.raises(ValueError, match="cannot read image"):
        maptool.vectorize_file("/nonexistent/map.png")
    m = maptool.vectorize(bytes(RED), 1, 1, channels=3)
    with pytest.raises(IndexError):
        m[5]


def lone_pixel_image() -> bytes:
    """5x5 blue with a lone red pixel at (1, 1) and a 2x2 red block: a single-pixel exclave."""
    px = [BLUE] * 25
    px[6] = RED
    for i in (18, 19, 23, 24):
        px[i] = RED
    return bytes(c for p in px for c in p)


def test_single_pixel_exclave_is_rejected():
    with pytest.raises(ValueError, match=r"single-pixel exclave at pixel \(1, 1\)"):
        maptool.vectorize(lone_pixel_image(), 5, 5, channels=3)
    # The check can be switched off.
    assert len(maptool.vectorize(lone_pixel_image(), 5, 5, channels=3, validate=False)) == 2


def test_four_way_junction_is_rejected():
    green, white = (0, 255, 0), (255, 255, 255)
    quadrants = [[RED, BLUE], [green, white]]  # four 2x2 blocks meeting at corner (2, 2)
    rows = [[quadrants[y // 2][x // 2] for x in range(4)] for y in range(4)]
    data = bytes(c for row in rows for p in row for c in p)
    with pytest.raises(ValueError, match=r"four-way junction at corner \(2, 2\)"):
        maptool.vectorize(data, 4, 4, channels=3)


def test_square_keeps_its_corners():
    rows = [[BLUE if 14 <= x < 26 and 14 <= y < 26 else RED for x in range(40)] for y in range(40)]
    data = bytes(c for row in rows for p in row for c in p)
    square = maptool.vectorize(data, 40, 40, channels=3)[1]
    assert "C" not in square.path and square.pixel_count == 144


def test_matches_cli(tmp_path):
    """The Python wrapper and the CLI are the same code path and must agree."""
    import pathlib
    import subprocess

    cli = pathlib.Path(__file__).parent.parent / "target/release/maptool"
    if not cli.exists():
        pytest.skip("build the CLI first: cargo build --release -p maptool-cli")
    rows = [[RED if (x // 5 + y // 4) % 2 == 0 else BLUE for x in range(30)] for y in range(20)]
    f = tmp_path / "map.png"
    f.write_bytes(png_bytes(rows))
    subprocess.run([cli, f, "-q"], check=True)
    assert f.with_suffix(".svg").read_text() == maptool.vectorize_file(f).to_svg()
