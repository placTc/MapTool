/**
 * Recent maps, kept in the browser's IndexedDB.
 *
 * localStorage cannot hold these: it is limited to a few MB of text, and a saved map is
 * several MB of binary. IndexedDB stores binary blobs on disk with a much larger quota.
 *
 * Three stores keep the big, unchanging part apart from the small, often edited part:
 *   meta   one small record per map, used to list them
 *   data   the saved map file (geometry plus the edits it had when saved)
 *   edits  the latest states and province metadata, rewritten on every edit (a few KB)
 */

export interface RecentMeta {
  id: string;
  name: string;
  /** Milliseconds since the epoch. */
  openedAt: number;
  savedAt: number;
  width: number;
  height: number;
  provinces: number;
  states: number;
  /** Stored size in bytes. */
  bytes: number;
  /** A small JPEG data URL. */
  thumb: string;
}

const DB_NAME = 'maptool';
const DB_VERSION = 1;
/** Older maps beyond this many are deleted. */
export const MAX_RECENT = 10;

let opened: Promise<IDBDatabase> | undefined;

function open(): Promise<IDBDatabase> {
  opened ??= new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') return reject(new Error('IndexedDB is not available'));
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      db.createObjectStore('meta', { keyPath: 'id' });
      db.createObjectStore('data');
      db.createObjectStore('edits');
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error('cannot open the browser database'));
    req.onblocked = () => reject(new Error('the browser database is blocked by another tab'));
  });
  // A failed open must be retried next time, not remembered.
  opened.catch(() => (opened = undefined));
  return opened;
}

function wrap<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

function done(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error('the browser refused to store the map (out of space?)'));
  });
}

const blob = (bytes: Uint8Array) => new Blob([bytes as BlobPart]);
const bytesOf = async (b: Blob | undefined) => (b ? new Uint8Array(await b.arrayBuffer()) : undefined);

/** Whether the browser lets us store anything (it may not, for example in some private windows). */
export async function storageAvailable(): Promise<boolean> {
  try {
    await open();
    return true;
  } catch {
    return false;
  }
}

/** Recent maps, newest first. Reads only the small records, never the map data. */
export async function listRecents(): Promise<RecentMeta[]> {
  const db = await open();
  const all = await wrap<RecentMeta[]>(db.transaction('meta').objectStore('meta').getAll());
  return all.sort((a, b) => b.openedAt - a.openedAt);
}

export async function hasRecent(id: string): Promise<boolean> {
  const db = await open();
  return (await wrap(db.transaction('meta').objectStore('meta').getKey(id))) !== undefined;
}

/** Store a new map. Then drops the oldest ones beyond `MAX_RECENT`. */
export async function saveNew(meta: RecentMeta, file: Uint8Array, edits: Uint8Array): Promise<void> {
  const db = await open();
  const tx = db.transaction(['meta', 'data', 'edits'], 'readwrite');
  tx.objectStore('meta').put({ ...meta, bytes: file.length + edits.length });
  tx.objectStore('data').put(blob(file), meta.id);
  tx.objectStore('edits').put(blob(edits), meta.id);
  await done(tx);
  await evict();
}

/** The stored map file and its latest edits. */
export async function loadRecent(id: string): Promise<{ file: Uint8Array; edits?: Uint8Array; meta: RecentMeta } | undefined> {
  const db = await open();
  const tx = db.transaction(['meta', 'data', 'edits']);
  const [meta, file, edits] = await Promise.all([
    wrap<RecentMeta | undefined>(tx.objectStore('meta').get(id)),
    wrap<Blob | undefined>(tx.objectStore('data').get(id)),
    wrap<Blob | undefined>(tx.objectStore('edits').get(id)),
  ]);
  if (!meta || !file) return undefined;
  return { file: (await bytesOf(file))!, edits: await bytesOf(edits), meta };
}

/** Save the latest edits of a stored map. Small, so it is cheap to call after every change. */
export async function saveEdits(id: string, edits: Uint8Array, states: number, name: string): Promise<void> {
  const db = await open();
  const tx = db.transaction(['meta', 'edits'], 'readwrite');
  const metas = tx.objectStore('meta');
  const meta = await wrap<RecentMeta | undefined>(metas.get(id));
  if (!meta) {
    tx.abort();
    return;
  }
  metas.put({ ...meta, savedAt: Date.now(), states, name });
  tx.objectStore('edits').put(blob(edits), id);
  await done(tx);
}

/** Move a map to the top of the list. */
export async function touch(id: string): Promise<void> {
  const db = await open();
  const tx = db.transaction('meta', 'readwrite');
  const meta = await wrap<RecentMeta | undefined>(tx.objectStore('meta').get(id));
  if (meta) tx.objectStore('meta').put({ ...meta, openedAt: Date.now() });
  await done(tx);
}

export async function remove(id: string): Promise<void> {
  const db = await open();
  const tx = db.transaction(['meta', 'data', 'edits'], 'readwrite');
  for (const store of ['meta', 'data', 'edits']) tx.objectStore(store).delete(id);
  await done(tx);
}

async function evict(): Promise<void> {
  const all = await listRecents();
  for (const old of all.slice(MAX_RECENT)) await remove(old.id);
}

/**
 * An id for a file's contents plus the settings it was built with, so opening the same
 * image again finds the saved copy (with its edits) instead of building it twice.
 */
export async function fileId(bytes: Uint8Array, salt: string): Promise<string> {
  let digest: Uint8Array;
  try {
    digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes as BufferSource));
  } catch {
    // crypto.subtle only exists in secure contexts (https or localhost): fall back to FNV-1a.
    let h = 0x811c9dc5;
    for (let i = 0; i < bytes.length; i++) h = Math.imul(h ^ bytes[i], 0x01000193) >>> 0;
    digest = new Uint8Array(new Uint32Array([h, bytes.length]).buffer);
  }
  const hex = Array.from(digest.slice(0, 12), (b) => b.toString(16).padStart(2, '0')).join('');
  return `${hex}-${salt}`;
}
