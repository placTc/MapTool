<script lang="ts">
  import { onMount } from 'svelte';
  import { MapRenderer, type Camera } from './lib/gl';
  import { loadMesh, loadTimings, type MapMesh } from './lib/map';

  let mesh = $state.raw<MapMesh | null>(null);
  let status = $state('Drop a PNG/BMP province map here, or pick one.');
  let busy = $state(false);
  let tolerance = $state(1);
  let validate = $state(true);
  let lastFile = $state.raw<Blob | null>(null);

  let hoveredId = $state<number | null>(null);
  let selectedId = $state<number | null>(null);

  interface Info {
    id: number;
    color: string;
    pixelCount: number;
    bbox: number[];
  }
  function infoOf(m: MapMesh, id: number): Info {
    return { id, color: '#' + m.color(id).toString(16).padStart(6, '0'), pixelCount: m.pixelCount(id), bbox: m.bbox(id) as unknown as number[] };
  }
  const hovered = $derived(mesh && hoveredId !== null ? infoOf(mesh, hoveredId) : null);
  const selected = $derived(mesh && selectedId !== null ? infoOf(mesh, selectedId) : null);

  // --- Drawing ---
  // The map lives on the GPU; the camera is three numbers. Nothing here touches
  // the province geometry, so pan and zoom cost one draw per frame.
  const MAX_K = 30;
  let canvasEl = $state<HTMLCanvasElement>();
  let cw = $state(0);
  let ch = $state(0);
  let renderer: MapRenderer | undefined;
  let cam: Camera = { x: 0, y: 0, k: 1 };
  let fitK = 1;
  let drawQueued = false;

  function requestDraw() {
    if (drawQueued) return;
    drawQueued = true;
    requestAnimationFrame(() => {
      drawQueued = false;
      renderer?.draw(cam, hoveredId, selectedId);
    });
  }

  onMount(() => {
    try {
      renderer = new MapRenderer(canvasEl!);
    } catch (e) {
      status = `Error: ${e instanceof Error ? e.message : e}`;
    }
    return () => renderer?.dispose();
  });

  $effect(() => {
    const w = cw;
    const h = ch;
    if (!renderer || w === 0 || h === 0) return;
    renderer.resize(w, h, window.devicePixelRatio || 1);
    requestDraw();
  });

  $effect(() => {
    hoveredId;
    selectedId;
    requestDraw();
  });

  function fitCam(m: MapMesh): Camera {
    const k = Math.min(cw / m.width, ch / m.height);
    fitK = k;
    return { x: (m.width - cw / k) / 2, y: (m.height - ch / k) / 2, k };
  }

  async function load(file: Blob) {
    if (!renderer) return;
    lastFile = file;
    busy = true;
    hoveredId = selectedId = null;
    status = 'Loading…';
    await new Promise((r) => setTimeout(r)); // let the status paint first
    try {
      const t = performance.now();
      const loaded = await loadMesh(file, { tolerance, validate });
      const old = mesh;
      mesh = loaded;
      const upload = performance.now();
      renderer.setMesh(loaded);
      const uploadMs = performance.now() - upload;
      old?.free();
      cam = fitCam(loaded);
      requestDraw();
      status =
        `${loaded.width}×${loaded.height}, ${loaded.len} provinces ` +
        `(${Math.round(performance.now() - t)} ms: ${Math.round(loadTimings.wasmMs)} in WASM, ${Math.round(uploadMs)} GPU upload)`;
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

  // --- Hit testing: the province under a screen point, computed in WASM on the drawn geometry. ---
  function pickAt(clientX: number, clientY: number): number | null {
    if (!mesh) return null;
    const r = canvasEl!.getBoundingClientRect();
    const id = mesh.pick(cam.x + (clientX - r.left) / cam.k, cam.y + (clientY - r.top) / cam.k);
    return id < 0 ? null : id;
  }

  // --- Pointer: hover, click to select, drag to pan ---
  let drag: { x: number; y: number; cam: Camera; moved: boolean } | null = null;

  function onPointerDown(e: PointerEvent) {
    try {
      canvasEl!.setPointerCapture(e.pointerId);
    } catch {
      // The pointer is already gone; dragging still works without capture.
    }
    drag = { x: e.clientX, y: e.clientY, cam, moved: false };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) {
      hoveredId = pickAt(e.clientX, e.clientY);
      return;
    }
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    drag.moved = true;
    cam = { x: drag.cam.x - dx / drag.cam.k, y: drag.cam.y - dy / drag.cam.k, k: drag.cam.k };
    requestDraw();
  }

  function onPointerUp(e: PointerEvent) {
    const wasClick = drag && !drag.moved;
    drag = null;
    if (wasClick) selectedId = pickAt(e.clientX, e.clientY);
  }

  /** Wheel delta in pixels. Browsers report lines (Firefox mouse wheels) or pages too. */
  function wheelPixels(e: WheelEvent): number {
    if (e.deltaMode === WheelEvent.DOM_DELTA_LINE) return e.deltaY * 40;
    if (e.deltaMode === WheelEvent.DOM_DELTA_PAGE) return e.deltaY * ch;
    return e.deltaY;
  }

  function onWheel(e: WheelEvent) {
    if (!mesh) return;
    e.preventDefault();
    const r = canvasEl!.getBoundingClientRect();
    const sx = e.clientX - r.left;
    const sy = e.clientY - r.top;
    const k = Math.min(Math.max(cam.k * Math.exp(-wheelPixels(e) * 0.0015), fitK * 0.5), MAX_K);
    // Keep the image point under the cursor where it is.
    const px = cam.x + sx / cam.k;
    const py = cam.y + sy / cam.k;
    cam = { x: px - sx / k, y: py - sy / k, k };
    requestDraw();
  }

  // Wheel listeners must be non-passive to be allowed to stop the page from scrolling.
  function wheelAction(node: HTMLElement) {
    node.addEventListener('wheel', onWheel, { passive: false });
    return { destroy: () => node.removeEventListener('wheel', onWheel) };
  }

  function fit() {
    if (!mesh) return;
    cam = fitCam(mesh);
    requestDraw();
  }

  function zoomTo(p: Info) {
    const [x0, y0, x1, y1] = p.bbox;
    const pad = Math.max(x1 - x0, y1 - y0) * 0.25 + 4;
    const k = Math.min(cw / (x1 - x0 + pad * 2), ch / (y1 - y0 + pad * 2), MAX_K);
    cam = { x: (x0 + x1) / 2 - cw / k / 2, y: (y0 + y1) / 2 - ch / k / 2, k };
    requestDraw();
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
    <button onclick={fit} disabled={!mesh}>Fit</button>
    <span class="status">{status}</span>
  </header>

  <section class="stage" bind:clientWidth={cw} bind:clientHeight={ch}>
    <canvas
      bind:this={canvasEl}
      use:wheelAction
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointerleave={() => (hoveredId = null)}
    ></canvas>

    {#if !mesh}
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
      {:else if mesh}
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
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    cursor: crosshair;
    touch-action: none;
    user-select: none;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #9aa3ad;
    margin: 0;
    pointer-events: none;
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
