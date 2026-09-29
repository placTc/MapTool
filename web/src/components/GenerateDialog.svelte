<script lang="ts">
  interface GenerateOpts {
    landRadius: number;
    waterRadius: number;
    splitSeas: boolean;
    seed: number;
  }

  interface Props {
    busy: boolean;
    status: string;
    onsubmit: (file: File, opts: GenerateOpts) => void;
    oncancel: () => void;
  }
  let { busy, status, onsubmit, oncancel }: Props = $props();

  let file: File | null = $state(null);
  let landRadius = $state(24);
  let waterRadius = $state(24);
  let splitSeas = $state(false);
  // A first "Generate" click should already look varied, not always the same seed-0 layout.
  let seed = $state(Math.floor(Math.random() * 2 ** 32));

  const canSubmit = $derived(!!file && landRadius > 0 && (!splitSeas || waterRadius > 0));

  function picked(e: Event & { currentTarget: HTMLInputElement }) {
    file = e.currentTarget.files?.[0] ?? null;
  }

  function submit(e: Event) {
    e.preventDefault();
    if (file && canSubmit) onsubmit(file, { landRadius, waterRadius, splitSeas, seed });
  }

  function regenerate() {
    seed = Math.floor(Math.random() * 2 ** 32);
    if (file && canSubmit) onsubmit(file, { landRadius, waterRadius, splitSeas, seed });
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !busy && oncancel()} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && !busy && oncancel()}>
  <div class="dialog" role="dialog" aria-label="Generate from border map">
    <form onsubmit={submit}>
      <h3>Generate from a border map</h3>
      <p class="hint">
        A PNG or BMP with white for land, blue (#0000FF) for sea or lakes, and black for a border line to respect
        (its meaning is decided later, in the editor).
      </p>
      <label>
        Border map image
        <input type="file" accept=".png,.bmp,image/png,image/bmp" onchange={picked} disabled={busy} />
      </label>
      <label>
        Land province size
        <input type="number" min="1" step="1" bind:value={landRadius} disabled={busy} />
        <small>Roughly the radius, in pixels, of a generated land province.</small>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={splitSeas} disabled={busy} />
        Split seas into provinces
        <small>Otherwise every connected sea or lake becomes exactly one province.</small>
      </label>
      {#if splitSeas}
        <label>
          Sea/lake province size
          <input type="number" min="1" step="1" bind:value={waterRadius} disabled={busy} />
        </label>
      {/if}
      {#if status}<p class="status" class:error={status.startsWith('Error')}>{status}</p>{/if}
      <div class="actions">
        <button type="button" class="btn btn-secondary" onclick={oncancel} disabled={busy}>Cancel</button>
        <button type="button" class="btn btn-secondary" onclick={regenerate} disabled={busy || !canSubmit}>Regenerate</button>
        <button type="submit" class="btn btn-primary" disabled={busy || !canSubmit}>{busy ? 'Generating…' : 'Generate'}</button>
      </div>
    </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: grid;
    place-items: center;
    z-index: 10;
  }
  .dialog {
    background: #1d2026;
    border: 1px solid #363b45;
    border-radius: 8px;
    padding: 16px 18px;
    width: min(460px, 92vw);
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.5);
  }
  form {
    display: grid;
    gap: 10px;
  }
  h3 {
    margin: 0;
    font-size: 15px;
  }
  label {
    display: grid;
    gap: 4px;
  }
  label.check {
    display: block;
  }
  input[type='number'] {
    width: 6em;
    background: #14161a;
    color: inherit;
    border: 1px solid #363b45;
    border-radius: 4px;
    padding: 6px 8px;
    font: inherit;
  }
  input[type='file'] {
    color: inherit;
  }
  small {
    color: #9aa3ad;
  }
  .status {
    margin: 0;
  }
  .status.error {
    color: #ff8c8c;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
