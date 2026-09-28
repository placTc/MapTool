<script lang="ts">
  import { hex, unhex, type MapDocument } from '../lib/map';

  /** A panel for countries or for strategic regions: both are groups of states. */
  interface Props {
    doc: MapDocument;
    rev: number;
    /** 0 for countries, 1 for strategic regions. */
    kind: number;
    /** "Countries" or "Strategic regions". */
    title: string;
    /** "country" or "strategic region". */
    noun: string;
    /** Ids of the selected groups. */
    selected: Set<number>;
    /** Ids of the selected states: what "create" and "add" act on. */
    selectedStates: Set<number>;
    /** `toggle` is true when ctrl, cmd or shift was held. */
    onselect: (id: number, toggle: boolean) => void;
    ondelete: (id: number) => void;
    onzoom: (provinces: Iterable<number>) => void;
    /** A group was created from the selected states. */
    oncreated: (id: number) => void;
    mutate: <T>(fn: () => T) => T | undefined;
  }
  let { doc, rev, kind, title, noun, selected, selectedStates, onselect, ondelete, onzoom, oncreated, mutate }: Props = $props();

  const list = $derived.by(() => {
    rev;
    return Array.from(doc.groupIds(kind), (id) => {
      const [states, provinces, land, sea, pixels, population, populated] = doc.groupStats(kind, id);
      return {
        id,
        name: doc.groupName(kind, id),
        color: hex(doc.groupColor(kind, id)),
        description: doc.groupDescription(kind, id),
        states,
        provinces,
        land,
        sea,
        pixels,
        population,
        populated,
      };
    });
  });

  const stateIds = $derived(Array.from(selectedStates));
  const typed = () => Uint32Array.from(stateIds);
  /** How many of the selected states are already in a group of this kind. */
  const grouped = $derived.by(() => {
    rev;
    return stateIds.filter((s) => doc.groupOfState(kind, s) >= 0).length;
  });

  let newName = $state('');
  let target = $state('');

  function create() {
    const id = mutate(() => doc.createGroup(kind, newName, typed()));
    if (id !== undefined) {
      newName = '';
      oncreated(id);
    }
  }

  function add() {
    if (target === '') return;
    mutate(() => doc.assignToGroup(kind, Number(target), typed()));
  }

  const memberNames = (id: number) => Array.from(doc.groupStates(kind, id), (s) => doc.stateName(s)).join(', ');

  function remove(id: number, name: string) {
    if (confirm(`Delete the ${noun} "${name}"? Its states stay, but belong to no ${noun}.`)) ondelete(id);
  }
</script>

<details class="panel" open>
  <summary>
    <h2>{title}</h2>
    <span class="count">{list.length}</span>
  </summary>

  {#if stateIds.length > 0}
    <div class="assign">
      <p class="lead">{stateIds.length} {stateIds.length === 1 ? 'state' : 'states'} selected</p>
      <div class="line">
        <input type="text" placeholder="New {noun} name" bind:value={newName} onkeydown={(e) => e.key === 'Enter' && create()} />
        <button onclick={create}>Create</button>
      </div>
      {#if list.length}
        <div class="line">
          <select bind:value={target}>
            <option value="">Add to existing {noun}…</option>
            {#each list as g (g.id)}<option value={String(g.id)}>{g.name}</option>{/each}
          </select>
          <button onclick={add} disabled={target === ''}>Add</button>
        </div>
      {/if}
      {#if grouped > 0}
        <button class="secondary" onclick={() => mutate(() => doc.unassignFromGroups(kind, typed()))}>Remove from {noun}</button>
      {/if}
    </div>
  {:else if list.length === 0}
    <p class="empty">
      No {title.toLowerCase()} yet. Select states in the States view (ctrl-click adds more, or drag a box), then create one here.
    </p>
  {/if}

  <ul>
    {#each list as g (g.id)}
      <li class:on={selected.has(g.id)}>
        <div class="head">
          <input
            class="color"
            type="color"
            value={g.color}
            title="Color"
            onchange={(e) => mutate(() => doc.setGroupColor(kind, g.id, unhex(e.currentTarget.value)))}
          />
          <input
            class="name"
            type="text"
            value={g.name}
            onchange={(e) => mutate(() => doc.renameGroup(kind, g.id, e.currentTarget.value))}
            onfocus={(e) => e.currentTarget.select()}
          />
          <button class="pick" onclick={(e) => onselect(g.id, e.ctrlKey || e.metaKey || e.shiftKey)} title="Select this {noun} (ctrl-click adds)">
            {g.states}
          </button>
        </div>
        {#if selected.has(g.id)}
          <textarea
            rows="2"
            placeholder="Description"
            value={g.description}
            onchange={(e) => mutate(() => doc.setGroupDescription(kind, g.id, e.currentTarget.value))}
          ></textarea>
          <dl>
            <dt>States</dt><dd>{g.states}{g.states ? `: ${memberNames(g.id)}` : ''}</dd>
            <dt>Provinces</dt><dd>{g.provinces} ({g.land} land, {g.sea} sea)</dd>
            <dt>Area</dt><dd>{g.pixels.toLocaleString()} px</dd>
            <dt>Population</dt><dd>{g.populated ? `${g.population.toLocaleString()} (set on ${g.populated})` : 'not set'}</dd>
          </dl>
          <div class="actions">
            <button onclick={() => onzoom(doc.provincesOfGroup(kind, g.id))} disabled={g.provinces === 0}>Zoom to</button>
            <button class="danger" onclick={() => remove(g.id, g.name)}>Delete</button>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</details>

<style>
  .panel {
    padding: 12px;
    border-top: 1px solid #2c3038;
  }
  summary {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  h2 {
    font-size: 14px;
    margin: 0;
    display: inline;
  }
  .count,
  .empty,
  dt {
    color: #9aa3ad;
  }
  .empty {
    margin: 8px 0 0;
  }
  .assign {
    display: grid;
    gap: 6px;
    margin: 10px 0;
    padding: 8px;
    background: #14161a;
    border: 1px solid #2c3038;
    border-radius: 6px;
  }
  .lead {
    margin: 0;
    font-weight: 600;
  }
  .line {
    display: flex;
    gap: 6px;
  }
  .line input,
  .line select {
    flex: 1;
    min-width: 0;
  }
  input[type='text'],
  select,
  textarea {
    background: #14161a;
    color: inherit;
    border: 1px solid #363b45;
    border-radius: 4px;
    padding: 4px 6px;
    font: inherit;
  }
  textarea {
    resize: vertical;
  }
  ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  li {
    border: 1px solid #2c3038;
    border-radius: 6px;
    padding: 6px;
    display: grid;
    gap: 6px;
  }
  li.on {
    border-color: #ffd400;
    background: rgba(255, 212, 0, 0.06);
  }
  .head {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .color {
    width: 28px;
    height: 26px;
    padding: 0;
    border: 0;
    background: none;
    flex: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    background: transparent !important;
    border-color: transparent !important;
  }
  .name:hover,
  .name:focus {
    border-color: #363b45 !important;
    background: #14161a !important;
  }
  button {
    background: #2f6fed;
    color: white;
    border: 0;
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    font: inherit;
  }
  button.secondary,
  .pick,
  .actions button {
    background: #363b45;
    color: inherit;
  }
  li.on .pick {
    background: #2f6fed;
    color: white;
  }
  .pick {
    min-width: 36px;
  }
  button.danger:hover {
    background: #8a2f2f;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin: 0;
    font-size: 13px;
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
</style>
