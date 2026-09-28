<script lang="ts">
  import { decodeImage } from './lib/decode';
  import { vectorizeRgba, type LoadedMap, type Province } from './lib/map';

  let map = $state.raw<LoadedMap | null>(null);
  let status = $state('Drop a PNG/BMP province map here, or pick one.');
  let busy = $state(false);
  let tolerance = $state(1);
  let validate = $state(true);
  let lastFile = $state.raw<Blob | null>(null);

  let hoveredId = $state<number | null>(null);
  let selectedId = $state<number | null>(null);
  const hovered = $derived(map && hoveredId !== null ? map.provinces[hoveredId] : null);
  const selected = $derived(map && selectedId !== null ? map.provinces[selectedId] : null);

  // The visible part of the map, in image pixels.
  let view = $state({ x: 0, y: 0, w: 1, h: 1 });
  let svgEl = $state<SVGSVGElement>();

  async function load(file: Blob) {
    lastFile = file;
    busy = true;
    hoveredId = selectedId = null;
    status = 'Decoding…';
    try {
      const { data, width, height } = await decodeImage(file);
      status = `Vectorizing ${width}×${height}…`;
      await new Promise((r) => setTimeout(r)); // let the status paint first
      const t = performance.now();
      map = await vectorizeRgba(data, width, height, tolerance, validate);
      view = { x: 0, y: 0, w: map.width, h: map.height };
      status = `${map.provinces.length} provinces (${Math.round(performance.now() - t)} ms)`;
    } catch (e) {
      status = `Error: ${e instanceof Error ? e.message : e}`;
    } finally {
      busy = false;
    }
  }

  async function loadDemo() {
    await load(await (await fetch('demo.png')).blob());
  }

  function onFile(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0];
    if (f) load(f);
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    const f = e.dataTransfer?.files[0];
    if (f) load(f);
  }

  // --- Hit testing: one delegated listener; each path carries its province id. ---
  function provinceAt(e: Event): number | null {
    const id = (e.target as SVGElement).dataset?.id;
    return id === undefined ? null : Number(id);
  }

  function onPointerMove(e: PointerEvent) {
    if (drag) {
      pan(e);
    } else {
      hoveredId = provinceAt(e);
    }
  }

  // --- Pan and zoom ---
  let drag: { x: number; y: number; view: typeof view; moved: boolean } | null = null;

  function onPointerDown(e: PointerEvent) {
    svgEl!.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, view: { ...view }, moved: false };
  }

  function pan(e: PointerEvent) {
    const d = drag!;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    if (!d.moved && Math.hypot(dx, dy) < 4) return;
    d.moved = true;
    const scale = d.view.w / svgEl!.clientWidth;
    view = { ...d.view, x: d.view.x - dx * scale, y: d.view.y - dy * scale };
  }

  function onPointerUp(e: PointerEvent) {
    const wasClick = drag && !drag.moved;
    drag = null;
    if (wasClick) {
      // Pointer capture retargets events to the svg, so look the element up again.
      const el = document.elementFromPoint(e.clientX, e.clientY) as SVGElement | null;
      const id = el?.dataset?.id;
      selectedId = id === undefined ? null : Number(id);
    }
  }

  function onWheel(e: WheelEvent) {
    if (!map) return;
    e.preventDefault();
    const rect = svgEl!.getBoundingClientRect();
    const factor = Math.exp(e.deltaY * 0.0015);
    const w = Math.min(Math.max(view.w * factor, 8), map.width * 4);
    const k = w / view.w;
    const px = view.x + ((e.clientX - rect.left) / rect.width) * view.w;
    const py = view.y + ((e.clientY - rect.top) / rect.height) * view.h;
    view = { x: px - (px - view.x) * k, y: py - (py - view.y) * k, w, h: view.h * k };
  }

  function fit() {
    if (map) view = { x: 0, y: 0, w: map.width, h: map.height };
  }

  function zoomTo(p: Province) {
    const [x0, y0, x1, y1] = p.bbox;
    const pad = Math.max(x1 - x0, y1 - y0) * 0.25 + 4;
    const aspect = view.h / view.w;
    let w = x1 - x0 + pad * 2;
    let h = w * aspect;
    if (h < y1 - y0 + pad * 2) {
      h = y1 - y0 + pad * 2;
      w = h / aspect;
    }
    view = { x: (x0 + x1) / 2 - w / 2, y: (y0 + y1) / 2 - h / 2, w, h };
  }
</script>

<svelte:window ondragover={(e) => e.preventDefault()} ondrop={onDrop} />

<main>
  <header>
    <h1>MapTool</h1>
    <label class="button">
      Open image
      <input type="file" accept="image/png,image/bmp" onchange={onFile} hidden />
    </label>
    <button onclick={loadDemo} disabled={busy}>Demo</button>
    <label>
      Smoothing tolerance
      <input type="number" min="0" max="5" step="0.25" bind:value={tolerance} />
    </label>
    <label title="Reject single-pixel exclaves and four-way junctions">
      <input type="checkbox" bind:checked={validate} /> Validate input
    </label>
    <button onclick={() => lastFile && load(lastFile)} disabled={busy || !lastFile}>Re-run</button>
    <button onclick={fit} disabled={!map}>Fit</button>
    <span class="status">{status}</span>
  </header>

  <section class="stage">
    {#if map}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <svg
        bind:this={svgEl}
        viewBox="{view.x} {view.y} {view.w} {view.h}"
        onpointermove={onPointerMove}
        onpointerleave={() => (hoveredId = null)}
        onpointerdown={onPointerDown}
        onpointerup={onPointerUp}
        onwheel={onWheel}
      >
        <g class="provinces">
          {#each map.provinces as p (p.id)}
            <path d={p.path} fill={p.color} fill-rule="evenodd" data-id={p.id} />
          {/each}
        </g>
        <!-- Overlays never take pointer events, so they can't steal hovers. -->
        {#if hovered}
          <path class="hover" d={hovered.path} fill-rule="evenodd" />
        {/if}
        {#if selected}
          <path class="selected" d={selected.path} fill-rule="evenodd" />
        {/if}
      </svg>
    {:else}
      <p class="empty">{status}</p>
    {/if}

    <aside>
      {#if hovered || selected}
        {@const p = hovered ?? selected!}
        <h2>{hovered ? 'Hover' : 'Selected'} · #{p.id}</h2>
        <div class="swatch" style:background={p.color}></div>
        <dl>
          <dt>Color</dt><dd>{p.color}</dd>
          <dt>Pixels</dt><dd>{p.pixelCount.toLocaleString()}</dd>
          <dt>Bounds</dt><dd>{p.bbox.join(', ')}</dd>
        </dl>
        {#if selected}
          <button onclick={() => zoomTo(selected)}>Zoom to selected</button>
        {/if}
      {:else if map}
        <p>Hover a province, click to select. Wheel zooms, drag pans.</p>
      {/if}
    </aside>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    font: 14px/1.4 system-ui, sans-serif;
    background: #14161a;
    color: #e6e8eb;
  }
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 8px 12px;
    background: #1d2026;
    border-bottom: 1px solid #2c3038;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 16px;
    margin: 0;
  }
  button,
  .button {
    background: #2f6fed;
    color: white;
    border: 0;
    border-radius: 4px;
    padding: 5px 10px;
    cursor: pointer;
    font: inherit;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  input[type='number'] {
    width: 4em;
  }
  .status {
    margin-left: auto;
    color: #9aa3ad;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  svg {
    width: 100%;
    height: 100%;
    display: block;
    cursor: crosshair;
    touch-action: none;
    user-select: none;
  }
  .provinces path {
    stroke: rgba(0, 0, 0, 0.35);
    stroke-width: 0.5;
    vector-effect: non-scaling-stroke;
  }
  .hover {
    fill: rgba(255, 255, 255, 0.35);
    pointer-events: none;
  }
  .selected {
    fill: none;
    stroke: #ffd400;
    stroke-width: 2.5;
    vector-effect: non-scaling-stroke;
    pointer-events: none;
  }
  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: #9aa3ad;
    margin: 0;
  }
  aside {
    position: absolute;
    top: 12px;
    right: 12px;
    min-width: 180px;
    padding: 10px 12px;
    background: rgba(29, 32, 38, 0.92);
    border: 1px solid #2c3038;
    border-radius: 6px;
    pointer-events: none;
  }
  aside button {
    pointer-events: auto;
  }
  aside:empty {
    display: none;
  }
  h2 {
    font-size: 13px;
    margin: 0 0 6px;
  }
  .swatch {
    height: 14px;
    border-radius: 3px;
    margin-bottom: 6px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin: 0 0 8px;
  }
  dt {
    color: #9aa3ad;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
</style>
