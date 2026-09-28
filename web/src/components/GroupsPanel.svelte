<script lang="ts">
  import { hex, unhex, type MapDocument } from '../lib/map';
  import { confirmAction, isMultiSelectClick, populationSummary, selectOnFocus, toUint32Array } from '../lib/utils';

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
        tag: kind === 0 ? doc.groupTag(kind, id) : '',
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
  const typed = () => toUint32Array(stateIds);
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
    confirmAction(`Delete the ${noun} "${name}"? Its states stay, but belong to no ${noun}.`, () => ondelete(id));
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
        <button class="btn btn-primary" onclick={create}>Create</button>
      </div>
      {#if list.length}
        <div class="line">
          <select bind:value={target}>
            <option value="">Add to existing {noun}…</option>
            {#each list as g (g.id)}<option value={String(g.id)}>{g.name}</option>{/each}
          </select>
          <button class="btn btn-primary" onclick={add} disabled={target === ''}>Add</button>
        </div>
      {/if}
      {#if grouped > 0}
        <button class="btn btn-secondary" onclick={() => mutate(() => doc.unassignFromGroups(kind, typed()))}>Remove from {noun}</button>
      {/if}
    </div>
  {:else if list.length === 0}
    <p class="empty">
      No {title.toLowerCase()} yet. Select states in the States view (ctrl-click adds more, or drag a box), then create one here.
    </p>
  {/if}

  <ul class="list-cards">
    {#each list as g (g.id)}
      <li class="list-card" class:selected={selected.has(g.id)}>
        <div class="head">
          <input
            class="color"
            type="color"
            value={g.color}
            title="Color"
            onchange={(e) => mutate(() => doc.setGroupColor(kind, g.id, unhex(e.currentTarget.value)))}
          />
          {#if kind === 0}
            <input
              class="tag"
              type="text"
              maxlength="3"
              placeholder="TAG"
              value={g.tag}
              title="Three-letter country tag"
              onchange={(e) => mutate(() => doc.setGroupTag(kind, g.id, e.currentTarget.value))}
            />
          {:else}
            <span class="uid" title="Region ID">#{g.id}</span>
          {/if}
          <input
            class="name"
            type="text"
            value={g.name}
            onchange={(e) => mutate(() => doc.renameGroup(kind, g.id, e.currentTarget.value))}
            onfocus={selectOnFocus}
          />
          <button class="btn btn-secondary pick" onclick={(e) => onselect(g.id, isMultiSelectClick(e))} title="Select this {noun} (ctrl-click adds)">
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
            <dt>Population</dt><dd>{populationSummary(g.population, g.populated)}</dd>
          </dl>
          <div class="actions">
            <button class="btn btn-secondary" onclick={() => onzoom(doc.provincesOfGroup(kind, g.id))} disabled={g.provinces === 0}>Zoom to</button>
            <button class="btn btn-secondary btn-danger" onclick={() => remove(g.id, g.name)}>Delete</button>
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
  .empty {
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
  /* The name field is also `input[type='text']`, so this panel's own text-input styling (above)
     is more specific than the shared `.list-card .name` look and would otherwise win: restate
     transparent here, `!important`, to keep it looking like a plain label rather than a field. */
  .name {
    background: transparent !important;
    border-color: transparent !important;
  }
  .name:hover,
  .name:focus {
    border-color: #363b45 !important;
    background: #14161a !important;
  }
  .tag {
    width: 44px;
    flex: none;
    text-align: center;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  /* Regions show a #id badge here instead of a country's tag; a touch wider than the shared
     default so it lines up with .tag next to it. */
  .list-card .uid {
    width: 44px;
  }
  button {
    padding: 4px 10px;
  }
  .list-card dd {
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
