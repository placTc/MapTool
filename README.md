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
| `web/` | Svelte + WebGL2 viewer with hover, click, pan and zoom |

## Usage

```sh
# CLI
cargo run --release -p maptool-cli -- map.png -o map.svg
cargo run --release -p maptool-cli -- --help

# Browser viewer
cd web && npm install && npm run wasm && npm run dev
```

## Browser viewer

The heavy work all happens in Rust/WASM: PNG/BMP decoding, smoothing, triangulation,
hit testing, colors, outlines, and the editing model. The TypeScript side only uploads
buffers to the GPU, draws, and wires up the UI. Pixel colors never pass through the
browser's color pipeline, so province colors are exact.

- Needs a browser with **WebGL2** (any current Firefox, Chrome or Safari).
- `npm run wasm` builds the WASM package into `crates/maptool-wasm/pkg` (git-ignored).
  **Run it again after any change to the Rust crates**, then restart `npm run dev`.
- It needs `wasm-pack` on your `PATH`: `cargo install wasm-pack`, and make sure
  `~/.cargo/bin` is on `PATH`.

**Start screen.** Open a PNG/BMP (or a saved `.maptool` file), or pick a recent map.
The smoothing and validation settings live here and apply to newly built maps.

**Editing.** Click a province to select it; ctrl/cmd/shift-click adds or removes provinces;
the wheel zooms, Esc clears the selection.

- *Tools*: the tool sidebar on the left picks what dragging on the map does. **Pan** (`P`)
  moves the map; **Box** (`B`) draws a selection box. Clicking selects with either, and holding
  Space pans with any tool. Keys are ignored while you type in a field. A new tool is one entry
  in `web/src/lib/tools.ts` plus its behavior in the editor.
- *Box selection* (the **Box** tool, or press `B`): drag a rectangle over the map. In
  the sidebar, choose what dropping the box does (replace the selection, add to it, or
  deselect what it covers; Shift adds and Alt removes whatever the mode is), whether it takes
  provinces it touches or only those fully inside it, land and/or sea only, and whether to
  skip provinces already in a state. The provinces a box would take light up while you drag.
  Buttons deselect provinces already in states, select every unassigned province, or invert
  the selection. Pan with middle-drag or Space held, or switch back to the **Pan** tool.
- *States* are named, colored groups of provinces, with an optional description. Create one
  from the selection, add the selection to an existing state, rename, recolor or delete it.
  A province is in at most one state: putting it in another moves it.
- *Countries* and *strategic regions* are groups of **states**, with a name, color and
  description. Select states (in the States view, from the states list, or with a box), then
  create a country or region from them or add them to an existing one. A state is in at most
  one country and at most one region, and the two are independent. Deleting a state removes
  it from both.
- *Province details*: name (blank shows the province number), description, land or sea,
  biome, and population. Type and biome can be set on many provinces at once. A sea
  province's biome is always **Sea** and is locked; making it land again brings back the
  biome it had. In a bulk biome edit, sea provinces are skipped.
- *Map name*: click the name at the top left to rename the map. It is saved with the map
  (in the recent list, in autosave, and inside downloaded files), so it no longer depends on
  what the image file was called.
- *Import CSV*: gives each province its type and its own ID by hex color (see below).
- *Views*: **Provinces** shows every province and can color by source colors, state, country,
  strategic region, land/sea, or biome. **States**, **Countries** and **Regions** show those
  groups instead of provinces: borders inside a group disappear, hovering or clicking anywhere
  on a group picks all of it, and where a province has no group at that level it falls back to
  its state and then to itself (dimmed). The sidebar shows province counts, area and
  population for what is selected.

**Import CSV.** (Hover the **i** next to the button for a short version of this.) Load a CSV whose rows give a hex color, a province type (`land` or `sea`),
and the province's own ID (a whole number). Rows are matched to provinces by the color in the
image, and the ID becomes the province's number: it is shown as `#ID` and is the province's
name when it has none.

- Columns can come in any order, separated by commas, semicolons or tabs, with or without a
  header row. Headers named like `color`/`hex`, `type`, and `id`/`number` are recognised;
  otherwise the columns are worked out from the data. Colors may be `#1a2b3c`, `1a2b3c` or
  `0x1a2b3c`. Either the type or the ID column may be left out.
- A row with a bad value, a color that is not in the map, or a color or ID that an earlier row
  already used is skipped and listed in a report; the other rows are applied. If no row
  matches, nothing changes and you are told why.
- Importing replaces earlier IDs; types only change for provinces the file lists.
- A file with separate red, green and blue columns (such as a Clausewitz `definition.csv`) is
  not read directly; it needs a single hex color column.

**Saving.** **Save as…** asks for a file name and writes a `.maptool` file (your browser puts it
in its download folder, unless it is set to ask where). Opened maps are also kept in this browser's IndexedDB (localStorage is limited to
a few MB of text and cannot hold them), as the finished geometry, so reopening a recent
map takes about a tenth of a second instead of rebuilding it. Your states and province
details are autosaved separately, a few KB per edit. At most 10 maps are kept. A `.maptool` file has the geometry and all your edits; open it again from the
start screen (or drop it on the page) to continue on another machine. Opening the same
image again finds its saved copy, edits included.

The file format is documented in `crates/maptool-core/src/document.rs` and `mesh.rs`.
Loading validates everything and rejects damaged or incompatible files with a message.

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
```
