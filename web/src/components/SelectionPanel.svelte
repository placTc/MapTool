<script lang="ts">
  import { biomeList, hex, type MapDocument } from '../lib/map';

  interface Props {
    doc: MapDocument;
    /** Bumped after every edit, so values read from the document are refreshed. */
    rev: number;
    /** The selected provinces. */
    ids: number[];
    mutate: <T>(fn: () => T) => T | undefined;
    onclear: () => void;
    /** A state was created from the selection. */
    oncreated: (id: number) => void;
  }
  let { doc, rev, ids, mutate, onclear, oncreated }: Props = $props();

  // The last biome is Sea: only sea provinces have it, and it cannot be picked.
  const landBiomes = biomeList().slice(0, -1);
  const typed = () => Uint32Array.from(ids);

  const single = $derived(ids.length === 1 ? ids[0] : null);
  /**
   * What the single-province editor shows. It depends on `rev`, so a change made elsewhere
   * (a CSV import, say) shows up while the province stays selected.
   */
  const detail = $derived.by(() => {
    rev;
    if (single === null) return null;
    return {
      number: doc.provinceNumber(single),
      ownName: doc.provinceOwnName(single) ?? '',
      description: doc.provinceDescription(single),
      population: doc.provincePopulation(single)?.toString() ?? '',
      color: hex(doc.color(single)),
    };
  });
  const kinds = $derived.by(() => {
    rev;
    return new Set(ids.map((i) => doc.provinceKind(i)));
  });
  const allSea = $derived(kinds.size === 1 && kinds.has(1));
  /** The biomes of the land provinces in the selection; sea provinces are locked to Sea. */
  const biomeSet = $derived.by(() => {
    rev;
    return new Set(ids.filter((i) => doc.provinceKind(i) === 0).map((i) => doc.provinceBiome(i)));
  });
  const stateSet = $derived.by(() => {
    rev;
    return new Set(ids.map((i) => doc.stateOf(i)));
  });
  const pixels = $derived.by(() => {
    rev;
    return ids.reduce((sum, i) => sum + doc.pixelCount(i), 0);
  });
  const populated = $derived.by(() => {
    rev;
    let total = 0;
    let known = 0;
    for (const i of ids) {
      const p = doc.provincePopulation(i);
      if (p !== undefined) {
        total += p;
        known++;
      }
    }
    return { total, known };
  });
  const states = $derived.by(() => {
    rev;
    return Array.from(doc.stateIds(), (id) => ({ id, name: doc.stateName(id) }));
  });
  const anyAssigned = $derived(!stateSet.has(-1) || stateSet.size > 1);

  const stateLabel = $derived.by(() => {
    if (stateSet.size > 1) return 'several states, or none';
    const only = [...stateSet][0];
    return only === -1 ? 'none' : doc.stateName(only);
  });

  let newName = $state('');
  let target = $state('');

  function createState() {
    const id = mutate(() => doc.createState(newName, typed()));
    if (id !== undefined) {
      newName = '';
      oncreated(id);
    }
  }

  function addToState() {
    if (target === '') return;
    mutate(() => doc.assignToState(Number(target), typed()));
  }

  function commitPopulation(id: number, text: string) {
    const cleaned = text.replace(/[\s,_']/g, '');
    mutate(() => doc.setProvincePopulation(id, cleaned === '' ? undefined : Number(cleaned)));
  }
</script>

<section class="panel">
  <header>
    <h2>{ids.length} {ids.length === 1 ? 'province' : 'provinces'} selected</h2>
    <button class="link" onclick={onclear}>Clear</button>
  </header>

  {#if single !== null && detail}
    {#key single}
      <div class="fields">
        <label>
          Name
          <input
            type="text"
            placeholder={String(detail.number)}
            value={detail.ownName}
            onchange={(e) => mutate(() => doc.setProvinceName(single, e.currentTarget.value))}
          />
          <small>Province #{detail.number}. Left blank, the number is its name.</small>
        </label>
        <div class="row">
          <span class="label">Color</span>
          <span class="swatch" style:background={detail.color}></span>
          <code>{detail.color}</code>
          <small>in the image</small>
        </div>
        <label>
          Description
          <textarea rows="3" value={detail.description} onchange={(e) => mutate(() => doc.setProvinceDescription(single, e.currentTarget.value))}></textarea>
        </label>
        <label>
          Population
          <input
            type="text"
            inputmode="numeric"
            placeholder="not set"
            value={detail.population}
            onchange={(e) => commitPopulation(single, e.currentTarget.value)}
          />
        </label>
      </div>
    {/key}
  {:else}
    <p class="sum">
      {pixels.toLocaleString()} px{populated.known ? ` · population ${populated.total.toLocaleString()} (set on ${populated.known})` : ''}
    </p>
  {/if}

  <div class="row">
    <span class="label">Type</span>
    <div class="seg">
      <button class:on={kinds.size === 1 && kinds.has(0)} onclick={() => mutate(() => doc.setKind(typed(), 0))}>Land</button>
      <button class:on={kinds.size === 1 && kinds.has(1)} onclick={() => mutate(() => doc.setKind(typed(), 1))}>Sea</button>
    </div>
    {#if kinds.size > 1}<small>mixed</small>{/if}
  </div>

  <div class="row">
    <span class="label">Biome</span>
    {#if allSea}
      <select disabled title="Sea provinces always have the Sea biome"><option>Sea</option></select>
      <small>locked for sea</small>
    {:else}
      <select
        value={biomeSet.size === 1 ? String([...biomeSet][0]) : ''}
        onchange={(e) => mutate(() => doc.setBiome(typed(), Number(e.currentTarget.value)))}
      >
        {#if biomeSet.size > 1}<option value="" disabled>mixed</option>{/if}
        {#each landBiomes as b, i}<option value={String(i)}>{b}</option>{/each}
      </select>
      {#if kinds.size > 1}<small>sea provinces stay Sea</small>{/if}
    {/if}
  </div>

  <div class="row"><span class="label">State</span><span>{stateLabel}</span></div>

  <div class="states">
    <div class="line">
      <input type="text" placeholder="New state name" bind:value={newName} onkeydown={(e) => e.key === 'Enter' && createState()} />
      <button onclick={createState}>Create state</button>
    </div>
    {#if states.length}
      <div class="line">
        <select bind:value={target}>
          <option value="">Add to existing state…</option>
          {#each states as s (s.id)}<option value={String(s.id)}>{s.name}</option>{/each}
        </select>
        <button onclick={addToState} disabled={target === ''}>Add</button>
      </div>
    {/if}
    {#if anyAssigned}
      <button class="secondary" onclick={() => mutate(() => doc.unassign(typed()))}>Remove from state</button>
    {/if}
  </div>
</section>

<style>
  .panel {
    padding: 12px;
    border-bottom: 1px solid #2c3038;
    display: grid;
    gap: 10px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  h2 {
    font-size: 14px;
    margin: 0;
  }
  .fields {
    display: grid;
    gap: 8px;
  }
  label {
    display: grid;
    gap: 3px;
  }
  small,
  .sum {
    color: #9aa3ad;
    font-size: 12px;
    margin: 0;
  }
  input[type='text'],
  textarea,
  select {
    background: #14161a;
    color: inherit;
    border: 1px solid #363b45;
    border-radius: 4px;
    padding: 5px 7px;
    font: inherit;
    min-width: 0;
  }
  textarea {
    resize: vertical;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .label {
    width: 52px;
    color: #9aa3ad;
  }
  .swatch {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    border: 1px solid #363b45;
  }
  code {
    font-family: ui-monospace, monospace;
  }
  .seg {
    display: flex;
  }
  .seg button {
    border-radius: 0;
  }
  .seg button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .seg button:last-child {
    border-radius: 0 4px 4px 0;
  }
  button {
    background: #363b45;
    color: inherit;
    border: 0;
    border-radius: 4px;
    padding: 5px 11px;
    cursor: pointer;
    font: inherit;
  }
  button.on,
  .states .line button:not(:disabled) {
    background: #2f6fed;
    color: white;
  }
  button.secondary {
    background: #363b45;
    color: inherit;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.link {
    background: none;
    color: #7fa8ff;
    padding: 0;
  }
  .states {
    display: grid;
    gap: 8px;
    border-top: 1px solid #2c3038;
    padding-top: 10px;
  }
  .line {
    display: flex;
    gap: 6px;
  }
  .line input,
  .line select {
    flex: 1;
  }
</style>
