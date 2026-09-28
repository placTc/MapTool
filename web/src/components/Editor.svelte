<script lang="ts">
  import { onMount } from 'svelte';
  import { MapRenderer, type Camera } from '../lib/gl';
  import { biomeList, type MapDocument } from '../lib/map';
  import SelectionPanel from './SelectionPanel.svelte';
  import StatesPanel from './StatesPanel.svelte';

  interface Props {
    doc: MapDocument;
    name: string;
    /** Load timing, and whether edits have been saved. */
    status: string;
    /** Called after every change to states or province details, so the app can save them. */
    onedit: () => void;
    /** Called once, when the map is first drawn, with a small preview image. */
    onready: (thumb: string) => void;
    onclose: () => void;
    ondownload: () => void;
  }
  let { doc, name, status, onedit, onready, onclose, ondownload }: Props = $props();

  const biomes = biomeList();
  const MAX_K = 30;

  // ---- What is shown and selected
  /** Bumped after every edit; anything read from the document depends on it. */
  let rev = $state(0);
  let viewKind = $state<'provinces' | 'states'>('provinces');
  /** How the province view is colored: 0 source colors, 1 by state, 2 by type, 3 by biome. */
  let colorBy = $state(0);
  let selection = $state.raw(new Set<number>());
  let selectedStates = $state.raw(new Set<number>());
  let hoverProvince = $state<number | null>(null);
  let toast = $state('');
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function showError(e: unknown) {
    toast = e instanceof Error ? e.message : String(e);
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ''), 6000);
  }

  /** Run an edit; on success refresh everything that depends on the document and let the app save it. */
  function mutate<T>(fn: () => T): T | undefined {
    try {
      const result = fn();
      rev++;
      onedit();
      return result;
    } catch (e) {
      showError(e);
      return undefined;
    }
  }

  /** Provinces that are selected, directly or through a selected state. */
  const highlighted = $derived.by(() => {
    rev;
    const out = new Set(selection);
    for (const s of selectedStates) {
      try {
        for (const p of doc.stateProvinces(s)) out.add(p);
      } catch {
        // The state was deleted; the selection is cleaned up by whoever deleted it.
      }
    }
    return out;
  });

  /** What lights up under the pointer: the province, or in the state view its whole state. */
  const hovered = $derived.by(() => {
    rev;
    if (hoverProvince === null) return [] as number[];
    return viewKind === 'states' ? Array.from(doc.groupProvinces(hoverProvince)) : [hoverProvince];
  });
  const hoverKey = $derived(hovered.length === 0 ? '' : viewKind === 'states' ? `g${hovered[0]}:${hovered.length}` : `p${hovered[0]}`);

  const hoverInfo = $derived.by(() => {
    rev;
    const p = hoverProvince;
    if (p === null) return null;
    const s = doc.stateOf(p);
    return {
      id: p,
      name: doc.provinceName(p),
      sea: doc.provinceKind(p) === 1,
      biome: biomes[doc.provinceBiome(p)],
      population: doc.provincePopulation(p),
      state: s >= 0 ? doc.stateName(s) : null,
    };
  });

  /** The map's own name; the file's name (`name`) stands in until it has one. */
  const mapName = $derived.by(() => {
    rev;
    return doc.mapName;
  });

  // ---- Drawing
  let canvasEl = $state<HTMLCanvasElement>();
  let stageEl = $state<HTMLElement>();
  let cw = $state(0);
  let ch = $state(0);
  let ready = $state(false);
  let renderer: MapRenderer | undefined;
  let cam: Camera = { x: 0, y: 0, k: 1 };
  let fitK = 1;
  let drawQueued = false;
  let announced = false;

  function requestDraw() {
    if (drawQueued) return;
    drawQueued = true;
    requestAnimationFrame(() => {
      drawQueued = false;
      renderer?.draw(cam, { stateView: viewKind === 'states' });
    });
  }

  onMount(() => {
    try {
      renderer = new MapRenderer(canvasEl!);
      renderer.setDocument(doc);
      ready = true;
    } catch (e) {
      showError(e);
    }
    return () => renderer?.dispose();
  });

  function fitCam(): Camera {
    const k = Math.min(cw / doc.width, ch / doc.height);
    fitK = k;
    return { x: (doc.width - cw / k) / 2, y: (doc.height - ch / k) / 2, k };
  }

  // Size the canvas, and fit the map the first time it has a size.
  $effect(() => {
    const w = cw;
    const h = ch;
    if (!ready || w === 0 || h === 0) return;
    renderer!.resize(w, h, window.devicePixelRatio || 1);
    if (!announced) {
      announced = true;
      cam = fitCam();
      // After the effects below have set the palette, draw once and take the thumbnail.
      requestAnimationFrame(() => onready(renderer!.snapshot(cam, doc.width, doc.height, 240)));
    }
    requestDraw();
  });

  // The document tells us the colors and outlines; the renderer only draws them.
  $effect(() => {
    if (!ready) return;
    rev;
    renderer!.setPalette(doc.palette(viewKind === 'states' ? 1 : colorBy, Uint32Array.from(highlighted), Uint32Array.from(hovered)));
    requestDraw();
  });
  $effect(() => {
    if (!ready) return;
    rev;
    renderer!.setStateBorders(viewKind === 'states' ? doc.stateBorderIndices() : null);
    requestDraw();
  });
  $effect(() => {
    if (!ready) return;
    rev;
    renderer!.setOutline('selection', highlighted.size ? doc.boundaryIndices(Uint32Array.from(highlighted)) : null);
    requestDraw();
  });
  $effect(() => {
    if (!ready) return;
    hoverKey;
    rev;
    renderer!.setOutline('hover', hovered.length ? doc.boundaryIndices(Uint32Array.from(hovered)) : null);
    requestDraw();
  });

  // ---- Selecting
  const toggled = (set: Set<number>, id: number) => {
    const next = new Set(set);
    if (!next.delete(id)) next.add(id);
    return next;
  };

  function clearSelection() {
    selection = new Set();
    selectedStates = new Set();
  }

  /** A click on province `p` (or empty space when null). Ctrl, cmd or shift adds to the selection. */
  function select(p: number | null, add: boolean) {
    if (p === null) {
      if (!add) clearSelection();
      return;
    }
    const state = viewKind === 'states' ? doc.stateOf(p) : -1;
    if (state >= 0) {
      // State view: a state is picked as a whole.
      selectedStates = add ? toggled(selectedStates, state) : new Set([state]);
      if (!add) selection = new Set();
    } else {
      selection = add ? toggled(selection, p) : new Set([p]);
      if (!add) selectedStates = new Set();
    }
  }

  function selectState(id: number, add: boolean) {
    selectedStates = add ? toggled(selectedStates, id) : new Set([id]);
    if (!add) selection = new Set();
  }

  function deleteState(id: number) {
    selectedStates = new Set([...selectedStates].filter((s) => s !== id));
    mutate(() => doc.deleteState(id));
  }

  function stateCreated(id: number) {
    selection = new Set();
    selectedStates = new Set([id]);
  }

  function setView(kind: 'provinces' | 'states') {
    if (kind === viewKind) return;
    viewKind = kind;
    clearSelection();
  }

  // ---- Pointer: hover, click to select, drag to pan, wheel to zoom
  function pickAt(clientX: number, clientY: number): number | null {
    const r = canvasEl!.getBoundingClientRect();
    const id = doc.pick(cam.x + (clientX - r.left) / cam.k, cam.y + (clientY - r.top) / cam.k);
    return id < 0 ? null : id;
  }

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
      hoverProvince = pickAt(e.clientX, e.clientY);
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
    if (wasClick) select(pickAt(e.clientX, e.clientY), e.ctrlKey || e.metaKey || e.shiftKey);
  }

  /** Wheel delta in pixels. Browsers report lines (Firefox mouse wheels) or pages too. */
  function wheelPixels(e: WheelEvent): number {
    if (e.deltaMode === WheelEvent.DOM_DELTA_LINE) return e.deltaY * 40;
    if (e.deltaMode === WheelEvent.DOM_DELTA_PAGE) return e.deltaY * ch;
    return e.deltaY;
  }

  function onWheel(e: WheelEvent) {
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
    cam = fitCam();
    requestDraw();
  }

  function zoomToProvinces(ids: Iterable<number>) {
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const id of ids) {
      const b = doc.bbox(id);
      x0 = Math.min(x0, b[0]);
      y0 = Math.min(y0, b[1]);
      x1 = Math.max(x1, b[2]);
      y1 = Math.max(y1, b[3]);
    }
    if (!isFinite(x0)) return;
    const pad = Math.max(x1 - x0, y1 - y0) * 0.15 + 4;
    const k = Math.min(cw / (x1 - x0 + pad * 2), ch / (y1 - y0 + pad * 2), MAX_K);
    cam = { x: (x0 + x1) / 2 - cw / k / 2, y: (y0 + y1) / 2 - ch / k / 2, k };
    requestDraw();
  }

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLElement && ['INPUT', 'TEXTAREA', 'SELECT'].includes(e.target.tagName);
    if (e.key === 'Escape' && !typing) clearSelection();
  }

  const selectedList = $derived(Array.from(selection));
</script>

<svelte:window onkeydown={onKey} />

<div class="editor">
  <header>
    <button class="secondary" onclick={onclose} title="Back to the start screen">← Maps</button>
    <input
      class="title"
      type="text"
      aria-label="Map name"
      title="Click to rename the map"
      placeholder={name}
      value={mapName}
      onchange={(e) => mutate(() => doc.setMapName(e.currentTarget.value))}
      onfocus={(e) => e.currentTarget.select()}
      onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
    />

    <div class="seg" role="group" aria-label="View">
      <button class:on={viewKind === 'provinces'} onclick={() => setView('provinces')}>Provinces</button>
      <button class:on={viewKind === 'states'} onclick={() => setView('states')}>States</button>
    </div>

    {#if viewKind === 'provinces'}
      <label class="inline">
        Color by
        <select bind:value={colorBy}>
          <option value={0}>Source colors</option>
          <option value={1}>State</option>
          <option value={2}>Land / sea</option>
          <option value={3}>Biome</option>
        </select>
      </label>
    {/if}

    <button class="secondary" onclick={fit}>Fit</button>
    <button class="secondary" onclick={() => zoomToProvinces(highlighted)} disabled={highlighted.size === 0}>Zoom to selection</button>
    <button class="secondary" onclick={ondownload} title="Save the map, with its states and province details, as a file">Download map</button>
    <span class="status">{status}</span>
  </header>

  <div class="body">
    <section class="stage" bind:this={stageEl} bind:clientWidth={cw} bind:clientHeight={ch}>
      <canvas
        bind:this={canvasEl}
        use:wheelAction
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointerleave={() => (hoverProvince = null)}
      ></canvas>

      {#if hoverInfo}
        <div class="tip">
          {#if viewKind === 'states' && hoverInfo.state}
            <strong>{hoverInfo.state}</strong>
            <span>state · via province {hoverInfo.name}</span>
          {:else}
            <strong>{hoverInfo.name}</strong>
            <span>
              #{hoverInfo.id} · {hoverInfo.sea ? 'sea' : `land, ${hoverInfo.biome.toLowerCase()}`}{hoverInfo.population !== undefined
                ? ` · pop. ${hoverInfo.population.toLocaleString()}`
                : ''}{hoverInfo.state ? ` · ${hoverInfo.state}` : ''}
            </span>
          {/if}
        </div>
      {/if}

      {#if toast}<div class="toast" role="alert">{toast}</div>{/if}
    </section>

    <aside>
      {#if selectedList.length > 0}
        <SelectionPanel {doc} {rev} ids={selectedList} {mutate} onclear={() => (selection = new Set())} oncreated={stateCreated} />
      {:else if selectedStates.size === 0}
        <p class="hint">
          Click a province to select it; ctrl-click (or shift-click) adds more. Drag to pan, wheel to zoom, Esc clears.
        </p>
      {/if}
      <StatesPanel {doc} {rev} selected={selectedStates} onselect={selectState} ondelete={deleteState} onzoom={zoomToProvinces} {mutate} />
    </aside>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 8px 12px;
    background: #1d2026;
    border-bottom: 1px solid #2c3038;
    flex-wrap: wrap;
  }
  .title {
    width: 200px;
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 4px 7px;
    font: inherit;
    font-weight: 600;
  }
  .title:hover,
  .title:focus {
    border-color: #363b45;
    background: #14161a;
  }
  .status {
    margin-left: auto;
    color: #9aa3ad;
  }
  button {
    background: #2f6fed;
    color: white;
    border: 0;
    border-radius: 4px;
    padding: 5px 11px;
    cursor: pointer;
    font: inherit;
  }
  button.secondary {
    background: #363b45;
    color: inherit;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .seg {
    display: flex;
  }
  .seg button {
    background: #363b45;
    color: inherit;
    border-radius: 0;
  }
  .seg button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .seg button:last-child {
    border-radius: 0 4px 4px 0;
  }
  .seg button.on {
    background: #2f6fed;
    color: white;
  }
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
    color: #9aa3ad;
  }
  select {
    background: #14161a;
    color: inherit;
    border: 1px solid #363b45;
    border-radius: 4px;
    padding: 4px 6px;
    font: inherit;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
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
  .tip {
    position: absolute;
    left: 12px;
    bottom: 12px;
    background: rgba(29, 32, 38, 0.92);
    border: 1px solid #2c3038;
    border-radius: 6px;
    padding: 6px 10px;
    display: grid;
    pointer-events: none;
    max-width: 60%;
  }
  .tip span {
    color: #9aa3ad;
    font-size: 12px;
  }
  .toast {
    position: absolute;
    left: 50%;
    top: 12px;
    transform: translateX(-50%);
    background: #5a2323;
    border: 1px solid #8a2f2f;
    border-radius: 6px;
    padding: 8px 14px;
    max-width: 80%;
  }
  aside {
    width: 320px;
    flex: none;
    overflow-y: auto;
    background: #1a1d22;
    border-left: 1px solid #2c3038;
  }
  .hint {
    color: #9aa3ad;
    margin: 0;
    padding: 12px;
    border-bottom: 1px solid #2c3038;
  }
</style>
