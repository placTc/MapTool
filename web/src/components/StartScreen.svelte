<script lang="ts">
  import type { RecentMeta } from '../lib/storage';

  interface Props {
    tolerance: number;
    validate: boolean;
    recents: RecentMeta[];
    /** False when the browser does not let us store anything. */
    storageOk: boolean;
    busy: boolean;
    status: string;
    onopen: (file: File) => void;
    onopenrecent: (id: string) => void;
    ondownload: (id: string) => void;
    ondelete: (id: string) => void;
  }
  let {
    tolerance = $bindable(),
    validate = $bindable(),
    recents,
    storageOk,
    busy,
    status,
    onopen,
    onopenrecent,
    ondownload,
    ondelete,
  }: Props = $props();

  function picked(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0];
    e.currentTarget.value = ''; // let the same file be picked again
    if (f) onopen(f);
  }

  const size = (b: number) => (b >= 1e6 ? (b / 1e6).toFixed(1) + ' MB' : Math.max(1, Math.round(b / 1e3)) + ' KB');
  const when = (t: number) => new Date(t).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
</script>

<div class="start">
  <h1>MapTool</h1>
  <p class="lead">Open a province map (PNG or BMP with one solid color per province), or a map saved from here.</p>

  <section class="card">
    <div class="open">
      <label class="button" class:disabled={busy}>
        {busy ? 'Working…' : 'Open a map'}
        <input type="file" accept=".png,.bmp,.maptool,image/png,image/bmp" onchange={picked} disabled={busy} hidden />
      </label>
      <span class="hint">or drop a file anywhere on this page</span>
    </div>

    <fieldset disabled={busy}>
      <legend>Settings for new maps</legend>
      <label>
        Smoothing tolerance
        <input type="number" min="0" max="5" step="0.25" bind:value={tolerance} />
        <small>0 keeps exact pixel edges; higher is smoother with fewer points</small>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={validate} />
        Validate input
        <small>rejects single-pixel exclaves and four-way junctions</small>
      </label>
    </fieldset>

    {#if status}<p class="status" class:error={status.startsWith('Error')}>{status}</p>{/if}
  </section>

  <section class="recents">
    <h2>Recent maps</h2>
    {#if !storageOk}
      <p class="note">This browser is not letting the page store maps, so there is no recent list. You can still download your work from the editor.</p>
    {:else if recents.length === 0}
      <p class="note">Maps you open are kept here, with your states and province details.</p>
    {:else}
      <ul>
        {#each recents as r (r.id)}
          <li>
            <button class="thumb" onclick={() => onopenrecent(r.id)} disabled={busy} title="Open {r.name}">
              <img src={r.thumb} alt="" />
            </button>
            <div class="info">
              <strong>{r.name}</strong>
              <span>{r.width}×{r.height} · {r.provinces.toLocaleString()} provinces · {r.states} {r.states === 1 ? 'state' : 'states'}</span>
              <span>{when(r.openedAt)} · {size(r.bytes)}</span>
              <div class="actions">
                <button onclick={() => onopenrecent(r.id)} disabled={busy}>Open</button>
                <button class="secondary" onclick={() => ondownload(r.id)} disabled={busy}>Download</button>
                <button class="secondary danger" onclick={() => ondelete(r.id)} disabled={busy}>Remove</button>
              </div>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .start {
    max-width: 880px;
    margin: 0 auto;
    padding: 32px 20px 60px;
    overflow: auto;
    height: 100%;
    box-sizing: border-box;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 26px;
  }
  h2 {
    font-size: 15px;
    margin: 28px 0 10px;
  }
  .lead,
  .note,
  .hint,
  small {
    color: #9aa3ad;
  }
  .lead {
    margin: 0 0 18px;
  }
  .card {
    background: #1d2026;
    border: 1px solid #2c3038;
    border-radius: 8px;
    padding: 16px;
  }
  .open {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
  }
  .button,
  button {
    background: #2f6fed;
    color: white;
    border: 0;
    border-radius: 5px;
    padding: 7px 14px;
    cursor: pointer;
    font: inherit;
  }
  .button.disabled,
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.secondary {
    background: #363b45;
  }
  button.danger:hover:not(:disabled) {
    background: #8a2f2f;
  }
  fieldset {
    border: 1px solid #2c3038;
    border-radius: 6px;
    display: grid;
    gap: 10px;
    padding: 10px 14px 14px;
    margin: 0;
  }
  legend {
    color: #9aa3ad;
    padding: 0 6px;
  }
  label {
    display: grid;
    gap: 2px;
  }
  label.check {
    display: block;
  }
  input[type='number'] {
    width: 5em;
  }
  .status {
    margin: 12px 0 0;
  }
  .status.error {
    color: #ff8c8c;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 12px;
  }
  li {
    display: flex;
    gap: 14px;
    background: #1d2026;
    border: 1px solid #2c3038;
    border-radius: 8px;
    padding: 10px;
  }
  .thumb {
    padding: 0;
    background: #14161a;
    border: 1px solid #2c3038;
    width: 200px;
    flex: none;
    display: block;
    line-height: 0;
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    display: block;
  }
  .info {
    display: grid;
    gap: 3px;
    align-content: start;
    min-width: 0;
  }
  .info span {
    color: #9aa3ad;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
</style>
