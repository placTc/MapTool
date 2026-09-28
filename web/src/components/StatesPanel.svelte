<script lang="ts">
  import { hex, unhex, type MapDocument } from '../lib/map';

  interface Props {
    doc: MapDocument;
    rev: number;
    /** Ids of the selected states. */
    selected: Set<number>;
    /** `toggle` is true when ctrl, cmd or shift was held. */
    onselect: (id: number, toggle: boolean) => void;
    ondelete: (id: number) => void;
    onzoom: (provinces: Iterable<number>) => void;
    mutate: <T>(fn: () => T) => T | undefined;
  }
  let { doc, rev, selected, onselect, ondelete, onzoom, mutate }: Props = $props();

  const list = $derived.by(() => {
    rev;
    return Array.from(doc.stateIds(), (id) => {
      const [provinces, land, sea, pixels, population, populated] = doc.stateStats(id);
      const country = doc.groupOfState(0, id);
      const region = doc.groupOfState(1, id);
      return {
        id,
        name: doc.stateName(id),
        color: hex(doc.stateColor(id)),
        description: doc.stateDescription(id),
        country: country >= 0 ? doc.groupName(0, country) : null,
        region: region >= 0 ? doc.groupName(1, region) : null,
        provinces,
        land,
        sea,
        pixels,
        population,
        populated,
      };
    });
  });

  function remove(id: number, name: string, provinces: number) {
    if (confirm(`Delete "${name}"? Its ${provinces} province${provinces === 1 ? '' : 's'} become unassigned.`)) ondelete(id);
  }
</script>

<section class="panel">
  <header>
    <h2>States</h2>
    <span class="count">{list.length}</span>
  </header>

  {#if list.length === 0}
    <p class="empty">No states yet. Select provinces (ctrl-click adds more), then create one.</p>
  {:else}
    <ul>
      {#each list as s (s.id)}
        <li class:on={selected.has(s.id)}>
          <div class="head">
            <input
              class="color"
              type="color"
              value={s.color}
              title="State color"
              onchange={(e) => mutate(() => doc.setStateColor(s.id, unhex(e.currentTarget.value)))}
            />
            <span class="uid" title="State ID">#{s.id}</span>
            <input
              class="name"
              type="text"
              value={s.name}
              onchange={(e) => mutate(() => doc.renameState(s.id, e.currentTarget.value))}
              onfocus={(e) => e.currentTarget.select()}
            />
            <button class="pick" onclick={(e) => onselect(s.id, e.ctrlKey || e.metaKey || e.shiftKey)} title="Select this state (ctrl-click adds)">
              {s.provinces}
            </button>
          </div>
          {#if selected.has(s.id)}
            <textarea
              rows="2"
              placeholder="Description"
              value={s.description}
              onchange={(e) => mutate(() => doc.setStateDescription(s.id, e.currentTarget.value))}
            ></textarea>
            <dl>
              <dt>Country</dt><dd>{s.country ?? 'none'}</dd>
              <dt>Region</dt><dd>{s.region ?? 'none'}</dd>
              <dt>Provinces</dt><dd>{s.provinces} ({s.land} land, {s.sea} sea)</dd>
              <dt>Area</dt><dd>{s.pixels.toLocaleString()} px</dd>
              <dt>Population</dt><dd>{s.populated ? `${s.population.toLocaleString()} (set on ${s.populated})` : 'not set'}</dd>
            </dl>
            <div class="actions">
              <button onclick={() => onzoom(doc.stateProvinces(s.id))}>Zoom to</button>
              <button class="danger" onclick={() => remove(s.id, s.name, s.provinces)}>Delete</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .panel {
    padding: 12px;
    display: grid;
    gap: 8px;
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
  .count,
  .empty,
  dt {
    color: #9aa3ad;
  }
  .empty {
    margin: 0;
  }
  ul {
    list-style: none;
    margin: 0;
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
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 3px 5px;
    font: inherit;
  }
  textarea {
    background: #14161a;
    color: inherit;
    border: 1px solid #363b45;
    border-radius: 4px;
    padding: 4px 6px;
    font: inherit;
    resize: vertical;
  }
  .name:hover,
  .name:focus {
    border-color: #363b45;
    background: #14161a;
  }
  .uid {
    flex: none;
    width: 40px;
    text-align: center;
    color: #9aa3ad;
    font-size: 12px;
    font-family: ui-monospace, monospace;
  }
  button {
    background: #363b45;
    color: inherit;
    border: 0;
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    font: inherit;
  }
  .pick {
    min-width: 36px;
  }
  li.on .pick {
    background: #2f6fed;
    color: white;
  }
  button.danger:hover {
    background: #8a2f2f;
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
  }
  .actions {
    display: flex;
    gap: 6px;
  }
</style>
