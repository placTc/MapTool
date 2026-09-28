<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Editor from './components/Editor.svelte';
  import StartScreen from './components/StartScreen.svelte';
  import { fileSafe, stemOf } from './lib/files';
  import { isMapFile, loadTimings, openMapBytes, type MapDocument } from './lib/map';
  import * as store from './lib/storage';

  // ---- Settings for new maps (only shown on the start screen), remembered between visits
  const SETTINGS_KEY = 'maptool.settings';
  function savedSettings(): { tolerance: number; validate: boolean } {
    try {
      const s = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? '{}');
      return { tolerance: Number.isFinite(s.tolerance) ? s.tolerance : 1, validate: s.validate !== false };
    } catch {
      return { tolerance: 1, validate: true };
    }
  }
  const initial = savedSettings();
  let tolerance = $state(initial.tolerance);
  let validate = $state(initial.validate);
  $effect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify({ tolerance, validate }));
    } catch {
      // Settings just will not be remembered.
    }
  });

  // ---- State
  let doc = $state.raw<MapDocument | null>(null);
  let docKey = $state(0);
  let mapName = $state('');
  let mapId = '';
  /** A map that is open but not stored yet: stored once it has been drawn (so it has a thumbnail). */
  let pending: { id: string; name: string; file: Uint8Array | null } | null = null;
  let stored = false;

  let busy = $state(false);
  let status = $state('');
  let recents = $state.raw<store.RecentMeta[]>([]);
  let storageOk = $state(true);
  let editorStatus = $state('');

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function refreshRecents() {
    try {
      recents = await store.listRecents();
    } catch (e) {
      status = `Error: could not read the recent maps (${message(e)})`;
    }
  }

  onMount(() => {
    (async () => {
      storageOk = await store.storageAvailable();
      if (storageOk) await refreshRecents();
    })();
    const hidden = () => document.hidden && flushSave();
    document.addEventListener('visibilitychange', hidden);
    window.addEventListener('pagehide', flushSave);
    return () => {
      document.removeEventListener('visibilitychange', hidden);
      window.removeEventListener('pagehide', flushSave);
    };
  });

  // ---- Opening
  const yieldToPaint = () => new Promise((r) => setTimeout(r));
  const nameOf = (file: string) => stemOf(file) || 'Map';
  const safeName = (name: string) => fileSafe(name) || 'Map';

  /** Swap in a new document, saving and releasing the one that was open. */
  async function show(next: MapDocument, name: string, id: string) {
    await flushSave();
    const old = doc;
    doc = next;
    mapName = name;
    mapId = id;
    docKey++;
    editorStatus = '';
    if (old) {
      await tick();
      old.free();
    }
  }

  async function openFile(file: File) {
    if (busy) return;
    busy = true;
    status = 'Reading…';
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const isMap = isMapFile(bytes);
      const id = await store.fileId(bytes, isMap ? 'map' : `t${tolerance}`);
      const name = nameOf(file.name);

      // The same file again: open the saved copy, which has the edits made to it. A copy that cannot
      // be read (saved by another version of the app) is dropped, and the map is built again.
      const saved = storageOk ? await store.loadRecent(id).catch(() => undefined) : undefined;
      let reopened: MapDocument | undefined;
      if (saved) {
        status = 'Opening your saved copy…';
        await yieldToPaint();
        try {
          reopened = await openMapBytes(saved.file, { tolerance, validate });
          if (saved.edits) reopened.setEditsBytes(saved.edits);
        } catch {
          reopened?.free();
          reopened = undefined;
          await store.remove(id).catch(() => {});
        }
      }
      if (saved && reopened) {
        const opened = reopened;
        if (!opened.mapName) opened.setMapName(saved.meta.name);
        pending = null;
        stored = true;
        await show(opened, opened.mapName, id);
        store.touch(id).catch(() => {});
        editorStatus = `Opened your saved copy (${Math.round(loadTimings.wasmMs)} ms)`;
      } else {
        status = isMap ? 'Opening…' : 'Building the map…';
        await yieldToPaint();
        const opened = await openMapBytes(bytes, { tolerance, validate });
        // A new image is named after its file; a saved map keeps the name it was given.
        if (!opened.mapName) opened.setMapName(name);
        pending = { id, name: opened.mapName, file: isMap ? bytes : null };
        stored = false;
        await show(opened, opened.mapName, id);
        editorStatus =
          `${opened.width}×${opened.height}, ${opened.len.toLocaleString()} provinces (${Math.round(loadTimings.wasmMs)} ms)` +
          (saved ? ' · its saved copy was from another version of the app, so it was built again' : '');
      }
      status = '';
    } catch (e) {
      status = `Error: ${message(e)}`;
    } finally {
      busy = false;
    }
  }

  async function openRecent(id: string) {
    if (busy) return;
    busy = true;
    status = 'Opening…';
    try {
      const saved = await store.loadRecent(id);
      if (!saved) throw new Error('that map is no longer stored');
      await yieldToPaint();
      const opened = await openMapBytes(saved.file, { tolerance, validate });
      let note = '';
      if (saved.edits) {
        try {
          opened.setEditsBytes(saved.edits);
        } catch (e) {
          note = ` — its latest edits could not be read (${message(e)})`;
        }
      }
      if (!opened.mapName) opened.setMapName(saved.meta.name);
      pending = null;
      stored = true;
      await show(opened, opened.mapName, id);
      store.touch(id).catch(() => {});
      editorStatus = `${Math.round(loadTimings.wasmMs)} ms${note}`;
      status = '';
    } catch (e) {
      const why = message(e);
      status = /another version of this app/.test(why)
        ? 'Error: this map was saved by another version of the app and cannot be opened. Remove it from the list and open the image again.'
        : `Error: ${why}`;
      await refreshRecents();
    } finally {
      busy = false;
    }
  }

  // ---- Storing the open map
  /** The editor drew the map for the first time: keep it, with its preview, in the recent list. */
  async function onReady(thumb: string) {
    if (!pending || !doc || !storageOk) return;
    const p = pending;
    pending = null;
    const d = doc;
    try {
      const now = Date.now();
      await store.saveNew(
        { id: p.id, name: p.name, openedAt: now, savedAt: now, width: d.width, height: d.height, provinces: d.len, states: d.stateCount, bytes: 0, thumb },
        p.file ?? d.toBytes(),
        d.editsBytes(),
      );
      stored = true;
      editorStatus += ' · saved in this browser';
    } catch (e) {
      editorStatus += ` · could not save in this browser (${message(e)}); use Download map to keep your work`;
    }
  }

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let dirty = false;

  /** States and province details changed: save them soon. They are a few KB, so this is cheap. */
  function onEdit() {
    dirty = true;
    if (!stored) return;
    editorStatus = 'Saving…';
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, 500);
  }

  async function flushSave() {
    clearTimeout(saveTimer);
    if (!dirty || !doc || !stored) return;
    dirty = false;
    try {
      await store.saveEdits(mapId, doc.editsBytes(), doc.stateCount, doc.mapName || mapName);
      editorStatus = 'All changes saved';
    } catch (e) {
      dirty = true;
      editorStatus = `Not saved: ${message(e)}. Use Download map to keep your work`;
    }
  }

  async function closeMap() {
    await flushSave();
    const old = doc;
    doc = null;
    await tick();
    old?.free();
    pending = null;
    stored = false;
    await refreshRecents();
  }

  // ---- Downloads
  function saveFile(bytes: Uint8Array, filename: string) {
    const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'application/octet-stream' }));
    const a = Object.assign(document.createElement('a'), { href: url, download: filename });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }

  /** Save the open map, with the file name the user chose in the Save as dialog. */
  function downloadCurrent(filename: string) {
    if (doc) saveFile(doc.toBytes(), filename);
  }

  async function downloadRecent(id: string) {
    try {
      const saved = await store.loadRecent(id);
      if (!saved) throw new Error('that map is no longer stored');
      // The stored file has the edits it was created with; put the latest ones in.
      const d = await openMapBytes(saved.file, { tolerance, validate });
      try {
        if (saved.edits) d.setEditsBytes(saved.edits);
        saveFile(d.toBytes(), `${safeName(d.mapName || saved.meta.name)}.maptool`);
      } finally {
        d.free();
      }
    } catch (e) {
      status = `Error: ${message(e)}`;
    }
  }

  async function deleteRecent(id: string) {
    const meta = recents.find((r) => r.id === id);
    if (!confirm(`Remove "${meta?.name ?? 'this map'}" from this browser? Download it first if you want to keep its states and province details.`)) return;
    try {
      await store.remove(id);
    } catch (e) {
      status = `Error: ${message(e)}`;
    }
    await refreshRecents();
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    const f = e.dataTransfer?.files[0];
    if (f) openFile(f);
  }
</script>

<svelte:window ondragover={(e) => e.preventDefault()} ondrop={onDrop} />

<main>
  {#if doc}
    {#key docKey}
      <Editor {doc} name={mapName} status={editorStatus} onedit={onEdit} onready={onReady} onclose={closeMap} ondownload={downloadCurrent} />
    {/key}
  {:else}
    <StartScreen
      bind:tolerance
      bind:validate
      {recents}
      {storageOk}
      {busy}
      {status}
      onopen={openFile}
      onopenrecent={openRecent}
      ondownload={downloadRecent}
      ondelete={deleteRecent}
    />
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font: 14px/1.4 system-ui, sans-serif;
    background: #14161a;
    color: #e6e8eb;
  }
  main {
    height: 100vh;
  }
</style>
