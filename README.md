# MapTool

Turn PNG/BMP images made of solid-colored areas ("provinces") into smoothed vector
geometry for interactive, per-province map display in the browser: SVG paths for
export, and GPU triangle meshes for a fast WebGL viewer. Built for large
Clausewitz-engine-style province maps.

- A province is identified by its **exact RGB color**. Disconnected regions of
  one color are one province (extra subpaths); enclaves are holes.
- Borders are traced on pixel edges **once** and shared by both neighbours, then
  simplified and fitted with cubic Béziers, so smoothing never leaves gaps or
  overlaps between provinces. Image edges stay straight.
- Fully transparent pixels (RGBA) belong to no province.

Speed (5632×2048, ~2800 provinces, single thread): SVG paths in about 0.3 s, a full
triangle mesh in about 0.3 s. In the browser the whole load (PNG decode, smoothing,
triangulation, GPU upload) takes about 0.7 s, and pan/zoom stays at the display's
refresh rate with about 0.1 ms of CPU per frame.

## Layout

| Path | What |
| --- | --- |
| `crates/maptool-core` | The algorithm. No I/O by default; `io` feature adds PNG/BMP decoding |
| `crates/maptool-cli` | `maptool` command line tool |
| `crates/maptool-wasm` | wasm-bindgen wrapper: SVG paths, or a mesh with zero-copy buffers and hit testing |
| `crates/maptool-py` + `src/maptool` | Python package (pyo3, built with maturin) |
| `web/` | Svelte + WebGL2 viewer with hover, click, pan and zoom |

## Usage

```sh
# CLI
cargo run --release -p maptool-cli -- map.png -o map.svg
cargo run --release -p maptool-cli -- --help

# Python (inside the uv venv)
maturin develop --release
python -c "import maptool; m = maptool.vectorize_file('map.png'); print(m, m[0].path[:60])"

# Browser viewer
cd web && npm install && npm run wasm && npm run dev
```

## Browser viewer

The heavy work all happens in Rust/WASM: PNG/BMP decoding, smoothing, triangulation
and hit testing. The TypeScript side only uploads the buffers to the GPU and draws.
Pixel colors never pass through the browser's color pipeline, so province colors
are exact.

- Needs a browser with **WebGL2** (any current Firefox, Chrome or Safari).
- `npm run wasm` builds the WASM package into `crates/maptool-wasm/pkg` (git-ignored).
  **Run it again after any change to the Rust crates**, then restart `npm run dev`.
- It needs `wasm-pack` on your `PATH`: `cargo install wasm-pack`, and make sure
  `~/.cargo/bin` is on `PATH`.
- Province colors live in a palette texture, so recoloring provinces (for example
  by game data) means updating that texture. `MapRenderer.setPalette()` does it
  without touching any geometry.

## Input rules

Two situations make borders ambiguous, so the input is rejected with an error
that lists where they are (pass `validate=False` / `--no-validate` to skip the check):

- **Single-pixel exclave:** a lone pixel whose color also appears elsewhere.
  Same-colored pixels that touch only at a corner count as separate pieces, so
  a checkerboard is rejected. A province that is one pixel in total is fine.
- **Four-way junction:** four pixels of four different colors meeting at one
  corner. Three colors meeting is normal and allowed.

## Tuning

| Option | Default | Effect |
| --- | --- | --- |
| `tolerance` | 1.0 | Simplification in pixels. `0` gives exact pixel-edge polygons (no smoothing). Larger means fewer vertices and rounder borders |
| `corner_angle` | 100 | Turns sharper than this many degrees always stay corners |
| `corner_run` | 4 | Two straight borders at least this many pixels long that meet at a turn of 75° or more keep the corner, so squares and rectangles stay square while organic borders are still rounded |
| `min_chain_len` | 6 | Borders shorter than this (in pixel edges) are left unsmoothed so tiny provinces keep their shape |
| `precision` | 2 | Decimal places in the output |
| `validate` | true | Enforce the input rules above |

Smoothed curves run through the outer corners of the pixel staircase, so a
tight curve can sit up to about half a pixel outside the true pixel edge (a
disc of radius 20 grows by ~4% in area). Neighbours still tile exactly.

## Tests

```sh
cargo test --workspace
cargo build --release -p maptool-cli && .venv/bin/python -m pytest tests
```
