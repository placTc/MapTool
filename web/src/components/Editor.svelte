<script lang="ts">
  import { onMount } from 'svelte';
  import { MapRenderer, type Camera } from '../lib/gl';
  import { fileSafe } from '../lib/files';
  import { biomeList, filters, LEVEL, type MapDocument, type ViewKind } from '../lib/map';
  import BoxToolPanel from './BoxToolPanel.svelte';
  import GroupsPanel from './GroupsPanel.svelte';
  import SaveDialog from './SaveDialog.svelte';
  import SelectionPanel from './SelectionPanel.svelte';
  import StatesPanel from './StatesPanel.svelte';

  interface Props {
    doc: MapDocument;
    name: string;
    /** Load timing, and whether edits have been saved. */
    status: string;
    /** Called after every change to the map's contents, so the app can save them. */
    onedit: () => void;
    /** Called once, when the map is first drawn, with a small preview image. */
    onready: (thumb: string) => void;
    onclose: () => void;
    /** Save the map as a file with this name. */
    ondownload: (filename: string) => void;
  }
  let { doc, name, status, onedit, onready, onclose, ondownload }: Props = $props();

  const biomes = biomeList();
  const F = filters();
  const MAX_K = 30;

  // ---- What is shown
  /** Bumped after every edit; anything read from the document depends on it. */
  let rev = $state(0);
  let viewKind = $state<ViewKind>('provinces');
  const level = $derived(LEVEL[viewKind]);
  /** How the province view is colored: 0 source colors, 1 state, 2 land/sea, 3 biome, 4 country, 5 strategic region. */
  let colorBy = $state(0);

  // ---- What is selected: provinces, and the groups (states, countries, regions) picked as a whole
  interface Selection {
    provinces: Set<number>;
    states: Set<number>;
    countries: Set<number>;
    regions: Set<number>;
  }
  /** In the order of the unit kinds the document uses: 0 province, 1 state, 2 country, 3 region. */
  const KINDS = ['provinces', 'states', 'countries', 'regions'] as const;
  const emptySelection = (): Selection => ({ provinces: new Set(), states: new Set(), countries: new Set(), regions: new Set() });
  let sel = $state.raw<Selection>(emptySelection());

  let hoverProvince = $state<number | null>(null);
  let toast = $state('');
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let saving = $state(false);
  let lastFileName = '';

  // ---- The box tool
  let tool = $state<'pan' | 'box'>('pan');
  let boxMode = $state<'replace' | 'add' | 'remove'>('replace');
  let boxWhole = $state(false);
  let boxTypes = $state<'all' | 'land' | 'sea'>('all');
  let boxSkipInStates = $state(false);
  /** The box being dragged, in stage pixels. */
  let marquee = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  /** The provinces the box would select right now, shown while dragging. */
  let preview = $state.raw<number[] | null>(null);
  let previewKey = $state(0);

  /** The result of the last CSV import, shown until dismissed. */
  let csvReport = $state<{
    file: string;
    rows: number;
    matched: number;
    land: number;
    sea: number;
    unlisted: number;
    hasTypes: boolean;
    hasIds: boolean;
    problemCount: number;
    problems: string[];
  } | null>(null);

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

  /** The selection as `[kind, id, kind, id, ...]`, the form the document takes units in. */
  function selectionUnits(s: Selection): Uint32Array {
    const pairs: number[] = [];
    KINDS.forEach((key, kind) => s[key].forEach((id) => pairs.push(kind, id)));
    return Uint32Array.from(pairs);
  }

  /** Every province that is selected, directly or through a selected state, country or region. */
  const highlighted = $derived.by(() => {
    rev;
    return new Set<number>(doc.provincesOfUnits(selectionUnits(sel)));
  });

  const hoveredGroup = $derived.by(() => {
    rev;
    return hoverProvince === null ? ([] as number[]) : Array.from(doc.groupProvinces(hoverProvince, level));
  });
  /** What lights up: the box's preview while one is being dragged, else what is under the pointer. */
  const hovered = $derived(preview ?? hoveredGroup);
  const hoverKey = $derived(
    preview ? `box${previewKey}` : hoveredGroup.length === 0 ? '' : level === 0 ? `p${hoveredGroup[0]}` : `g${hoveredGroup[0]}:${hoveredGroup.length}`,
  );

  const UNIT_LABELS = ['province', 'state', 'country', 'strategic region'];

  const hoverInfo = $derived.by(() => {
    rev;
    const p = hoverProvince;
    if (p === null) return null;
    const [kind, id] = doc.unitsOf(Uint32Array.of(p), level);
    const s = doc.stateOf(p);
    const c = s >= 0 ? doc.groupOfState(0, s) : -1;
    const unitName = kind === 1 ? doc.stateName(id) : kind === 2 ? doc.groupName(0, id) : kind === 3 ? doc.groupName(1, id) : null;
    return {
      id: p,
      number: doc.provinceNumber(p),
      name: doc.provinceName(p),
      sea: doc.provinceKind(p) === 1,
      biome: biomes[doc.provinceBiome(p)],
      population: doc.provincePopulation(p),
      state: s >= 0 ? doc.stateName(s) : null,
      country: c >= 0 ? doc.groupName(0, c) : null,
      unitName,
      unitLabel: UNIT_LABELS[kind],
    };
  });

  /** The map's own name; the file's name (`name`) stands in until it has one. */
  const mapName = $derived.by(() => {
    rev;
    return doc.mapName;
  });

  // ---- Drawing
  let canvasEl = $state<HTMLCanvasElement>();
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
      renderer?.draw(cam, { stateView: level > 0 });
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
    const mode = viewKind === 'provinces' ? colorBy : ([0, 1, 4, 5] as const)[level];
    renderer!.setPalette(doc.palette(mode, Uint32Array.from(highlighted), Uint32Array.from(hovered)));
    requestDraw();
  });
  $effect(() => {
    if (!ready) return;
    rev;
    renderer!.setStateBorders(level > 0 ? doc.borderIndices(level) : null);
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
  /** Change the selection by some units (`[kind, id, ...]`): replace it, add to it, take from it, or flip them. */
  function applyUnits(units: ArrayLike<number>, how: 'replace' | 'add' | 'remove' | 'toggle') {
    const next: Selection =
      how === 'replace'
        ? emptySelection()
        : { provinces: new Set(sel.provinces), states: new Set(sel.states), countries: new Set(sel.countries), regions: new Set(sel.regions) };
    for (let i = 0; i < units.length; i += 2) {
      const set = next[KINDS[units[i]]];
      const id = units[i + 1];
      if (how === 'remove') set.delete(id);
      else if (how === 'toggle') set.has(id) ? set.delete(id) : set.add(id);
      else set.add(id);
    }
    sel = next;
  }

  const clearSelection = () => (sel = emptySelection());
  const selectedCount = $derived(sel.provinces.size + sel.states.size + sel.countries.size + sel.regions.size);

  /** A click on province `p` (or empty space when null). Ctrl, cmd or shift flips it in the selection. */
  function select(p: number | null, flip: boolean) {
    if (p === null) {
      if (!flip) clearSelection();
      return;
    }
    applyUnits(doc.unitsOf(Uint32Array.of(p), level), flip ? 'toggle' : 'replace');
  }

  /** Pick a state, country or region from its panel. */
  const selectGroup = (kind: number, id: number, flip: boolean) => applyUnits([kind, id], flip ? 'toggle' : 'replace');

  function deleteState(id: number) {
    sel = { ...sel, states: new Set([...sel.states].filter((s) => s !== id)) };
    mutate(() => doc.deleteState(id));
  }

  function deleteGroup(kind: number, id: number) {
    const key = KINDS[kind + 2];
    sel = { ...sel, [key]: new Set([...sel[key]].filter((g) => g !== id)) };
    mutate(() => doc.deleteGroup(kind, id));
  }

  function setView(kind: ViewKind) {
    if (kind === viewKind) return;
    viewKind = kind;
    clearSelection();
    preview = marquee = null;
  }

  // ---- Box selection tools
  const boxFlags = () => (boxSkipInStates ? F.skipInStates : 0) | (boxTypes === 'land' ? F.landOnly : 0) | (boxTypes === 'sea' ? F.seaOnly : 0);

  /** The box in image pixels, from two screen corners. */
  function boxProvinces(ax: number, ay: number, bx: number, by: number): number[] {
    const [x0, y0] = toImage(ax, ay);
    const [x1, y1] = toImage(bx, by);
    return Array.from(doc.provincesInRect(x0, y0, x1, y1, boxWhole, boxFlags()));
  }

  function applyBox(provinces: number[], e: PointerEvent) {
    // The modifier wins over the mode, so a box can add or remove without changing it.
    const how = e.shiftKey ? 'add' : e.altKey ? 'remove' : boxMode;
    applyUnits(doc.unitsOf(Uint32Array.from(provinces), level), how);
  }

  /** Provinces in the selection that are not in any state are kept; the rest are let go. */
  function deselectProvincesInStates() {
    const kept = doc.filterProvinces(Uint32Array.from(sel.provinces), F.skipInStates);
    // A selected state, country or region is made of provinces that are in states.
    sel = { ...emptySelection(), provinces: new Set(kept) };
  }

  function selectUnassigned() {
    sel = { ...emptySelection(), provinces: new Set(doc.unassignedProvinces()) };
  }

  function invertSelection() {
    const chosen = highlighted;
    const rest: number[] = [];
    for (let p = 0; p < doc.len; p++) if (!chosen.has(p)) rest.push(p);
    sel = { ...emptySelection(), provinces: new Set(rest) };
  }

  // ---- Pointer: hover, click to select, drag to pan or to draw a box, wheel to zoom
  const toImage = (clientX: number, clientY: number): [number, number] => {
    const r = canvasEl!.getBoundingClientRect();
    return [cam.x + (clientX - r.left) / cam.k, cam.y + (clientY - r.top) / cam.k];
  };

  function pickAt(clientX: number, clientY: number): number | null {
    const [x, y] = toImage(clientX, clientY);
    const id = doc.pick(x, y);
    return id < 0 ? null : id;
  }

  type Drag = { kind: 'pan'; x: number; y: number; cam: Camera; moved: boolean } | { kind: 'box'; x: number; y: number; moved: boolean };
  let drag: Drag | null = null;
  let spaceDown = false;

  function onPointerDown(e: PointerEvent) {
    if (e.button === 2) return;
    try {
      canvasEl!.setPointerCapture(e.pointerId);
    } catch {
      // The pointer is already gone; dragging still works without capture.
    }
    const pan = tool === 'pan' || e.button === 1 || spaceDown;
    drag = pan ? { kind: 'pan', x: e.clientX, y: e.clientY, cam, moved: false } : { kind: 'box', x: e.clientX, y: e.clientY, moved: false };
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
    if (drag.kind === 'pan') {
      cam = { x: drag.cam.x - dx / drag.cam.k, y: drag.cam.y - dy / drag.cam.k, k: drag.cam.k };
      requestDraw();
    } else {
      const r = canvasEl!.getBoundingClientRect();
      marquee = { x: Math.min(drag.x, e.clientX) - r.left, y: Math.min(drag.y, e.clientY) - r.top, w: Math.abs(dx), h: Math.abs(dy) };
      // Show what the box would select as it is dragged.
      preview = doc.provincesOfUnits(doc.unitsOf(Uint32Array.from(boxProvinces(drag.x, drag.y, e.clientX, e.clientY)), level)) as unknown as number[];
      previewKey++;
    }
  }

  function onPointerUp(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d) return;
    if (d.kind === 'box' && d.moved) {
      applyBox(boxProvinces(d.x, d.y, e.clientX, e.clientY), e);
      marquee = preview = null;
    } else if (!d.moved) {
      select(pickAt(e.clientX, e.clientY), e.ctrlKey || e.metaKey || e.shiftKey);
    }
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

  const typing = (e: KeyboardEvent) => e.target instanceof HTMLElement && ['INPUT', 'TEXTAREA', 'SELECT'].includes(e.target.tagName);

  function onKeyDown(e: KeyboardEvent) {
    if (typing(e) || e.ctrlKey || e.metaKey) return;
    if (e.key === 'Escape') {
      // Escape drops a box being drawn first, and only then the selection.
      if (drag?.kind === 'box') {
        drag = null;
        marquee = preview = null;
      } else {
        clearSelection();
      }
    } else if (e.key === 'b' || e.key === 'B') {
      tool = tool === 'box' ? 'pan' : 'box';
    } else if (e.key === ' ') {
      spaceDown = true;
      e.preventDefault();
    }
  }

  function onKeyUp(e: KeyboardEvent) {
    if (e.key === ' ') spaceDown = false;
  }

  async function importCsv(file: File) {
    let text: string;
    try {
      text = await file.text();
    } catch (e) {
      showError(e);
      return;
    }
    // A refused import throws, changes nothing, and the reason goes to the toast.
    const report = mutate(() => doc.importCsv(text));
    if (!report) return;
    csvReport = {
      file: file.name,
      rows: report.rows,
      matched: report.matched,
      land: report.land,
      sea: report.sea,
      unlisted: report.unlisted,
      hasTypes: report.hasTypes,
      hasIds: report.hasIds,
      problemCount: report.problemCount,
      problems: report.problems(),
    };
    report.free();
  }

  function csvPicked(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0];
    e.currentTarget.value = ''; // let the same file be picked again
    if (f) importCsv(f);
  }

  function save(filename: string) {
    saving = false;
    lastFileName = filename.replace(/\.maptool$/i, '');
    ondownload(filename);
  }

  const defaultFileName = () => lastFileName || fileSafe(mapName || name) || 'map';
  const selectedProvinceList = $derived(Array.from(sel.provinces));
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />

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
      <button class:on={viewKind === 'countries'} onclick={() => setView('countries')}>Countries</button>
      <button class:on={viewKind === 'regions'} onclick={() => setView('regions')}>Regions</button>
    </div>

    {#if viewKind === 'provinces'}
      <label class="inline">
        Color by
        <select bind:value={colorBy}>
          <option value={0}>Source colors</option>
          <option value={1}>State</option>
          <option value={4}>Country</option>
          <option value={5}>Strategic region</option>
          <option value={2}>Land / sea</option>
          <option value={3}>Biome</option>
        </select>
      </label>
    {/if}

    <div class="seg" role="group" aria-label="Tool">
      <button class:on={tool === 'pan'} onclick={() => (tool = 'pan')} title="Drag to pan">Pan</button>
      <button class:on={tool === 'box'} onclick={() => (tool = 'box')} title="Drag a box to select (B)">Box select</button>
    </div>

    <button class="secondary" onclick={fit}>Fit</button>
    <button class="secondary" onclick={() => zoomToProvinces(highlighted)} disabled={highlighted.size === 0}>Zoom to selection</button>
    <label class="button secondary" title="Load a CSV that gives each province's type (land or sea) and ID by its hex color">
      Import CSV
      <input type="file" accept=".csv,.tsv,.txt,text/csv,text/plain" onchange={csvPicked} hidden />
    </label>
    <button class="secondary" onclick={() => (saving = true)} title="Save the map, with everything you have added, as a file">Save as…</button>
    <span class="status">{status}</span>
  </header>

  <div class="body">
    <section class="stage" bind:clientWidth={cw} bind:clientHeight={ch}>
      <canvas
        bind:this={canvasEl}
        class:boxing={tool === 'box'}
        use:wheelAction
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointerleave={() => (hoverProvince = null)}
        oncontextmenu={(e) => e.preventDefault()}
      ></canvas>

      {#if marquee}
        <div class="marquee" style:left="{marquee.x}px" style:top="{marquee.y}px" style:width="{marquee.w}px" style:height="{marquee.h}px"></div>
      {/if}

      {#if hoverInfo}
        <div class="tip">
          {#if level > 0 && hoverInfo.unitName}
            <strong>{hoverInfo.unitName}</strong>
            <span>{hoverInfo.unitLabel} · via province {hoverInfo.name}</span>
          {:else}
            <strong>{hoverInfo.name}</strong>
            <span>
              #{hoverInfo.number} · {hoverInfo.sea ? 'sea' : `land, ${hoverInfo.biome.toLowerCase()}`}{hoverInfo.population !== undefined
                ? ` · pop. ${hoverInfo.population.toLocaleString()}`
                : ''}{hoverInfo.state ? ` · ${hoverInfo.state}` : ''}{hoverInfo.country ? ` · ${hoverInfo.country}` : ''}
            </span>
          {/if}
        </div>
      {/if}

      {#if toast}<div class="toast" role="alert">{toast}</div>{/if}
      {#if csvReport}
        <div class="report" role="dialog" aria-label="CSV import result">
          <h3>Imported {csvReport.file}</h3>
          <p>
            {csvReport.matched.toLocaleString()} of {csvReport.rows.toLocaleString()} rows applied
            {#if csvReport.hasTypes}· {csvReport.land.toLocaleString()} land, {csvReport.sea.toLocaleString()} sea{/if}
            {#if csvReport.hasIds}· IDs set{/if}
          </p>
          {#if csvReport.unlisted > 0}
            <p class="note">{csvReport.unlisted.toLocaleString()} provinces of this map were not in the file and keep what they had.</p>
          {/if}
          {#if csvReport.problemCount > 0}
            <p class="warn">{csvReport.problemCount.toLocaleString()} {csvReport.problemCount === 1 ? 'row was' : 'rows were'} skipped:</p>
            <ul>
              {#each csvReport.problems as p}<li>{p}</li>{/each}
              {#if csvReport.problemCount > csvReport.problems.length}
                <li>…and {(csvReport.problemCount - csvReport.problems.length).toLocaleString()} more</li>
              {/if}
            </ul>
          {/if}
          <button onclick={() => (csvReport = null)}>OK</button>
        </div>
      {/if}
    </section>

    <aside>
      {#if tool === 'box'}
        <BoxToolPanel
          bind:mode={boxMode}
          bind:whole={boxWhole}
          bind:types={boxTypes}
          bind:skipInStates={boxSkipInStates}
          hasSelection={selectedCount > 0}
          ondeselectinstates={deselectProvincesInStates}
          onselectunassigned={selectUnassigned}
          oninvert={invertSelection}
          onclear={clearSelection}
        />
      {/if}

      {#if selectedProvinceList.length > 0}
        <SelectionPanel
          {doc}
          {rev}
          ids={selectedProvinceList}
          {mutate}
          onclear={() => (sel = { ...sel, provinces: new Set() })}
          oncreated={(id) => applyUnits([1, id], 'replace')}
        />
      {:else if selectedCount === 0 && tool !== 'box'}
        <p class="hint">
          Click a province to select it; ctrl-click (or shift-click) adds more. Drag to pan, wheel to zoom, Esc clears, B for box select.
          Switch to States, Countries or Regions to work with whole groups.
        </p>
      {/if}

      <StatesPanel
        {doc}
        {rev}
        selected={sel.states}
        onselect={(id, flip) => selectGroup(1, id, flip)}
        ondelete={deleteState}
        onzoom={zoomToProvinces}
        {mutate}
      />
      <GroupsPanel
        {doc}
        {rev}
        kind={0}
        title="Countries"
        noun="country"
        selected={sel.countries}
        selectedStates={sel.states}
        onselect={(id, flip) => selectGroup(2, id, flip)}
        ondelete={(id) => deleteGroup(0, id)}
        onzoom={zoomToProvinces}
        oncreated={(id) => applyUnits([2, id], 'replace')}
        {mutate}
      />
      <GroupsPanel
        {doc}
        {rev}
        kind={1}
        title="Strategic regions"
        noun="strategic region"
        selected={sel.regions}
        selectedStates={sel.states}
        onselect={(id, flip) => selectGroup(3, id, flip)}
        ondelete={(id) => deleteGroup(1, id)}
        onzoom={zoomToProvinces}
        oncreated={(id) => applyUnits([3, id], 'replace')}
        {mutate}
      />
    </aside>
  </div>
</div>

{#if saving}
  <SaveDialog initial={defaultFileName()} onsave={save} oncancel={() => (saving = false)} />
{/if}

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
  button.secondary,
  label.button.secondary {
    background: #363b45;
    color: inherit;
  }
  label.button {
    border-radius: 4px;
    padding: 5px 11px;
    cursor: pointer;
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
  canvas.boxing {
    cursor: cell;
  }
  .marquee {
    position: absolute;
    border: 1px dashed #ffd400;
    background: rgba(255, 212, 0, 0.12);
    pointer-events: none;
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
  .report {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(560px, 90%);
    max-height: 80%;
    overflow-y: auto;
    background: #1d2026;
    border: 1px solid #363b45;
    border-radius: 8px;
    padding: 14px 18px;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.5);
  }
  .report h3 {
    margin: 0 0 8px;
    font-size: 15px;
  }
  .report p {
    margin: 4px 0;
  }
  .report .note {
    color: #9aa3ad;
  }
  .report .warn {
    color: #ffb86b;
    margin-top: 10px;
  }
  .report ul {
    margin: 4px 0 12px;
    padding-left: 18px;
    font-size: 12px;
    color: #c8ccd2;
    max-height: 220px;
    overflow-y: auto;
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
