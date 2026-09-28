# MapTool

Turns PNG/BMP province maps (one solid color per province) into smoothed vector geometry for an
interactive browser editor: WebGL2 meshes for the viewer, SVG paths for export. Built for big
Clausewitz-style maps (5632x2048, ~2800 provinces). `README.md` is the user-facing description;
this file is what you need to work on the code.

## Layout

| Path | Role |
| --- | --- |
| `crates/maptool-core` | All logic, pure Rust, no I/O by default (`io` feature adds PNG/BMP decoding) |
| `crates/maptool-wasm` | wasm-bindgen wrapper. `MapDocument` is the one object the UI holds |
| `crates/maptool-cli` | `maptool` CLI: image to SVG |
| `web/` | Svelte 5 + TypeScript + WebGL2 editor (Vite) |
| `examples/` | demo map, its SVGs, `make_demo.py` (standalone, checks the input rules itself) |

Core modules, in pipeline order: `label` (color -> province id) -> `validate` (input rules) ->
`trace` (crack-edge borders, cut into chains at junctions) -> `smooth` (Douglas-Peucker + Bezier,
corner rules) -> `build` (chains linked into rings; SVG paths or flattened rings) -> `mesh`
(lyon triangulation, hit testing, box queries, mesh file section). Then the editable model:
`provinces` (metadata), `states`, `groups` (countries and strategic regions), `csv`, and
`document` (ties them together, container format, views).

## Ideas that hold the design together

- **Borders are traced once and shared.** Both neighbours use the same chain (one forward, one
  reversed), flattened once, so their vertices are bit-identical: no gaps, no cracks. Anything that
  relies on "the province across this border" (`segment_mate` in `document.rs`) depends on this.
- **A `Document` is the immutable mesh plus the user's edits.** The mesh (~10 MB on the big map)
  is saved once; edits (map name, states, countries, regions, province data) are a few KB and are
  autosaved on their own. `Document::edits_to_bytes` / `set_edits_from_bytes`.
- **Compute lives in Rust.** The user wants as much as possible in Rust/WASM. TypeScript only
  uploads buffers, draws, and wires the UI. Palettes, outlines, borders per view, hit testing, box
  queries, CSV import, selection filters are all Rust. A Rust frontend was considered and rejected:
  measured CPU per frame is 0.1 ms and frames are vsync-locked, so there is nothing to win.
- **Views are levels** (`Level`: provinces, states, countries, regions). `Document::unit` says what a
  province is shown/picked/outlined as at a level, falling back country -> state -> province.
  `border_indices(level)` hides borders inside a unit.
- **Selection is per layer.** `Editor.svelte` keeps four sets (provinces, states, countries, regions)
  but a view only ever fills its own: `Document::layer_object` / `layer_units_of` / `layer_provinces`
  return only the view's own kind of object and nothing for a province that has none (unlike `unit`,
  which falls back for *drawing*). Click, hover, box and the box-panel actions all go through them.
  Panels that pick a group switch to its view first. Do not reintroduce fallbacks into selection.
- **Keep per-selection work in Rust and linear.** A big box selects thousands of provinces and the editor
  recomputes its outline on every pointer move, so anything that touches every selected province must be
  one WASM call doing O(selected) work, never a JS loop of per-province calls and never a hash set of
  segments. `Document::boundary_indices` walks only the selected provinces' segments and asks the
  neighbour table (`segment_mate`) whether the other side is selected: 0.3 to 0.7 ms on the big map, where
  the old hash-set version took 11 to 61 ms and made big-box drags stutter (100 ms per move). Totals for the
  selection panel come from `provinceStats`, `statesOf` and `landBiomes` for the same reason. The
  `bench` example prints these timings; `MapMesh::boundary_indices` stays as the slow reference the tests
  compare against.
- **Model rules** (all decided by the user): province identity is its exact RGB color; a province is
  in at most one state; a state is in at most one country and at most one region, countries and
  regions are independent of each other; a sea province's biome is always Sea (the land biome is
  kept underneath and comes back if it becomes land); a province's "number" is the CSV ID if one was
  imported, else its 0-based position; single-pixel exclaves and four-way junctions are rejected.
- **States and regions get a sequential ID**, shown as "#id" in their panel: it is their internal
  `id`, but its allocation keeps it compact — a new one gets the smallest id not currently in use
  (`StateSet::create`, `GroupSet::create` for `GroupKind::Region`), so deleting state 2 and creating
  another gives that one back id 2 rather than growing past the highest id ever used. `next_id` is
  kept only as a monotonic upper bound for validating loaded data, not as the source of new ids.
  A country instead gets a three-letter tag (`Group::tag`, `auto_tag` in `groups.rs`): the first
  three letters of its name that no other country's tag already uses, tried as combinations of the
  name's letters — the leading three first, then the third letter moved through the rest of the
  name, then (once that is exhausted) the second letter moved with the third scanning again after
  it, then the first letter too. Blank if the name has under three letters or every combination is
  taken; the user can then set one by hand (`setGroupTag`), which also accepts any blank-then-retype.
  A tag is fixed once set: renaming a country does not recompute it. A region's tag is always blank
  and rejects being set. Unlike states and regions, a country's own `id` (invisible to the user) is
  never reused after a delete, same as before — only its visible tag needed the fix.
- **One saved format, no compatibility.** The app is in development, not production, so a saved file is
  only read by the build that wrote it. The container has a single number (`FORMAT_VERSION` in
  `document.rs`) and files with any other number are refused with "saved by another version of this app".
  The parts inside (mesh section, edits, states, groups, province table) carry no versions and have no
  old-layout branches. Change any layout freely, but bump `FORMAT_VERSION` so stale data is refused instead
  of misread, and start tests from fresh maps. The editor copes with stale data: an unreadable recent map
  says so and can be removed, and opening the same image again drops its unreadable saved copy and builds
  fresh. Add real versioning only when there is data worth protecting. (The user decided this, and had
  asked for the old v1/v2/v3 support to be removed.)
- **Loading never panics on bad input** and a refused edit/import changes nothing. The tests fuzz
  this (truncate at every length, flip bits). Keep it that way for any new parser.
- **IndexedDB, not localStorage**, for recent maps (several MB of binary). Three stores: `meta`,
  `data` (the map file), `edits` (rewritten on each edit). Settings alone use localStorage.

## Commands

```sh
export PATH="$HOME/.cargo/bin:$PATH"   # needed in Claude's shell; the user's shell has it
cargo test --workspace                 # ~104 tests: csv 13, document 29, groups 31, vectorize 31
cargo clippy --workspace
cargo run --release -p maptool-core --example bench    # timings on a synthetic 5632x2048 map
cargo run --release -p maptool-cli -- map.png -o map.svg

cd web
npm install
npm run wasm     # wasm-pack build; REQUIRED after any Rust change (pkg/ is git-ignored)
npm run dev      # then restart it after npm run wasm
npm run check    # svelte-check; keep at 0 errors and 0 warnings
npx vite build
```

`vite.config.ts` needs `server.fs.allow` to include `crates/maptool-wasm/pkg`, otherwise the dev
server answers 403 for the WASM file.

## Style

- Rust edition 2024. Comments say why, not what. Doc comments on public items say what the
  function guarantees, including what it does on bad input.
- Tests are behavioral: exact areas against pixel counts, brute-force comparisons on random maps,
  refusals that leave state untouched. New model behavior gets tests in `crates/maptool-core/tests/`.
- Svelte: values shown from the WASM document must depend on `rev` (the edit counter), or they go
  stale when something else edits the document (this bit us with the province panel after a CSV
  import). Do not re-key inputs on every edit: that steals focus on Tab.
- Tools in the tool sidebar are one entry in `web/src/lib/tools.ts` plus behavior in `Editor.svelte`.
- **CSS grid blowout**: `StatesPanel.svelte`/`GroupsPanel.svelte` lay out each list card as a
  `display: grid` `<li>` (so its rows stack with `gap`), inside a `display: grid` `<ul>` (so cards
  stack the same way), inside the `aside` sidebar (a fixed 320px width, `overflow-y: auto`). A grid
  item's automatic minimum size defaults to its content's min-content size, not 0, so a row that
  can't shrink enough (a fixed-width badge or input next to a name field, say) can force its track —
  and everything nested inside it, all the way up to the sidebar's scrollable width — wider than the
  sidebar itself, pushing later content out of view with no visible scrollbar to explain why. Fixed
  once by adding `min-width: 0` to `li`, then hit again because `li` is itself a grid container, so
  its own direct children (`.head`, `dl`, `.actions`, ...) needed the same `min-width: 0` one level
  down (`li > *`). Any new fixed-width element added to a row in these panels can reintroduce this;
  check by measuring `aside.scrollWidth` vs `aside.clientWidth` in a real browser, not just
  `svelte-check` (this is a rendering effect, not a type error).

## Testing the UI in a real browser

The user runs Firefox on Linux (CachyOS). Headless Firefox here sees the real GPU, so WebGL works.
The pattern that has worked, every time:

1. Write a temporary page `web/_something.html` that mounts `App`, drives the real UI (set the file
   input's `files`, dispatch pointer/wheel/key events on the canvas, click buttons by text), records
   `checks`, and beacons the result to a tiny local server (`GET /R/<json>`). Delete the page after.
2. Run `vite` on a port, launch `firefox --headless --no-remote --profile <fresh dir>`.
3. To test persistence, navigate the page to `?phase=2` and check IndexedDB survived.

Pitfalls learned the hard way:
- `pkill -f PATTERN` inside a command that also contains PATTERN kills its own shell (exit 144). Put
  the logic in a script file and run that alone, and use `[x]pattern` brackets.
- A leftover headless Firefox holds the profile lock and silently stops new ones from starting.
  Kill the pid named in `<profile>/lock`.
- `firefox --screenshot` does not capture the WebGL canvas. To see the map, force a redraw and call
  `canvas.toDataURL()` in the same frame (the app draws in `requestAnimationFrame`). To screenshot the
  DOM with the map state, hold the page's `load` event open with a slow `<img>`.
- Synthetic pointer events need a `pointerId`. Svelte updates the DOM in a microtask, so wait a tick
  before clicking a button that a change just enabled.
- Firefox mouse wheels report `deltaMode` lines, not pixels (handled in `wheelPixels`).

## Git

- Author every commit as `Egor Matuk <gregory.matuk2004@gmail.com>` by passing
  `git -c user.name=... -c user.email=...` (there is no git identity configured, and do not set one).
- Commit after each finished piece of work, without asking, with a descriptive message ending in the
  Claude co-author trailer, and push it right away. Keep the working tree clean between tasks.
- Pushing: plain `git push` works. `gh` is installed and logged in as placTc, and `gh auth setup-git`
  has been run, so git takes GitHub credentials from `gh` (token in the keyring). SSH does not work from
  Claude's shell: the key in `~/.ssh` has a passphrase and there is no ssh-agent. **Push after every commit,
  automatically** (the user said to): commit, `git push`, and say what went out.

## Known limits and open ideas

- Loading a big map runs on the main thread (~0.7 s freeze). A Web Worker would fix it.
- Smoothed curves run through the outer corners of the staircase, so tight curves sit up to ~0.5 px
  outside the true pixel edge (a radius-20 disc grows ~4% in area). Neighbours still tile exactly.
- With every border pixel randomized, two smoothed borders can cross in a 1-pixel neck and overlap
  by a sliver (~1 px^2 on 19200). Noted in `tests/vectorize.rs`.
- Squares under ~4 px a side can still be rounded (`corner_run`).
- Not built, by decision: undo/redo (delete or edit states instead), a Python API (removed).
- Not built yet: CSV files with separate r;g;b columns, a native save-file picker (Firefox has none),
  more tools in the sidebar (lasso, flood fill, measure), an info icon on other controls.
