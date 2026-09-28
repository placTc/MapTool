"""Turn solid-colored province bitmaps into smoothed, interactable SVG paths."""

import argparse
from pathlib import Path

from maptool._core import Province, VectorMap, vectorize, vectorize_file

__all__ = ["Province", "VectorMap", "vectorize", "vectorize_file", "main"]


def main() -> None:
    parser = argparse.ArgumentParser(prog="maptool-py", description="Convert a PNG/BMP province map to SVG.")
    parser.add_argument("input", type=Path)
    parser.add_argument("-o", "--output", type=Path)
    parser.add_argument("-t", "--tolerance", type=float, default=1.0, help="0 = exact pixel edges")
    args = parser.parse_args()

    result = vectorize_file(args.input, tolerance=args.tolerance)
    out = args.output or args.input.with_suffix(".svg")
    out.write_text(result.to_svg())
    print(f"{result} -> {out}")
