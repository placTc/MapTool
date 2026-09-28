<script lang="ts">
  import type { ViewKind } from '../lib/map';

  interface Props {
    /** The view the box is used in: it selects that view's objects. */
    layer: ViewKind;
    mode: 'replace' | 'add' | 'remove';
    whole: boolean;
    types: 'all' | 'land' | 'sea';
    skipInStates: boolean;
    /** Whether the selection has anything in it. */
    hasSelection: boolean;
    ondeselectinstates: () => void;
    onselectunassigned: () => void;
    onselectall: () => void;
    oninvert: () => void;
    onclear: () => void;
  }
  let {
    mode = $bindable(),
    whole = $bindable(),
    types = $bindable(),
    skipInStates = $bindable(),
    layer,
    hasSelection,
    ondeselectinstates,
    onselectunassigned,
    onselectall,
    oninvert,
    onclear,
  }: Props = $props();

  const NOUN: Record<ViewKind, string> = { provinces: 'provinces', states: 'states', countries: 'countries', regions: 'strategic regions' };
  const noun = $derived(NOUN[layer]);
</script>

<section class="panel">
  <h2>Box selection</h2>
  {#if layer !== 'provinces'}
    <p class="hint">In this view a box selects <strong>{noun}</strong>: the ones with a province that it {whole ? 'covers' : 'touches'}.</p>
  {/if}
  <p class="hint">Drag a box on the map. Hold <kbd>Shift</kbd> to add or <kbd>Alt</kbd> to remove, whatever the mode. Right-drag, middle-drag or hold <kbd>Space</kbd> to pan.</p>

  <fieldset>
    <legend>When you drop the box</legend>
    <label><input type="radio" bind:group={mode} value="replace" /> Select only what it covers</label>
    <label><input type="radio" bind:group={mode} value="add" /> Add to the selection</label>
    <label><input type="radio" bind:group={mode} value="remove" /> Deselect what it covers</label>
  </fieldset>

  <fieldset>
    <legend>What it covers</legend>
    <label><input type="radio" bind:group={whole} value={false} /> Provinces it touches</label>
    <label><input type="radio" bind:group={whole} value={true} /> Only provinces fully inside it</label>
  </fieldset>

  <fieldset>
    <legend>Only these</legend>
    <label><input type="radio" bind:group={types} value="all" /> Land and sea</label>
    <label><input type="radio" bind:group={types} value="land" /> Land only</label>
    <label><input type="radio" bind:group={types} value="sea" /> Sea only</label>
    <label class="gap" class:off={layer !== 'provinces'} title={layer === 'provinces' ? '' : 'Only applies in the Provinces view'}>
      <input type="checkbox" bind:checked={skipInStates} disabled={layer !== 'provinces'} /> Skip provinces already in a state
    </label>
  </fieldset>

  <div class="actions">
    {#if layer === 'provinces'}
      <button class="btn btn-secondary" onclick={ondeselectinstates} disabled={!hasSelection} title="Take every province that is already in a state out of the selection">
        Deselect provinces already in states
      </button>
      <button class="btn btn-secondary" onclick={onselectunassigned} title="Select every province that is in no state">Select all unassigned provinces</button>
    {/if}
    <button class="btn btn-secondary" onclick={onselectall}>Select all {noun}</button>
    <button class="btn btn-secondary" onclick={oninvert} title="Select the {noun} that are not selected, and the other way round">Invert selection</button>
    <button class="btn btn-secondary" onclick={onclear} disabled={!hasSelection}>Clear selection</button>
  </div>
</section>

<style>
  .panel {
    padding: 12px;
    border-bottom: 1px solid #2c3038;
    display: grid;
    gap: 10px;
  }
  h2 {
    font-size: 14px;
    margin: 0;
  }
  kbd {
    background: #363b45;
    border-radius: 3px;
    padding: 0 4px;
    font: inherit;
    font-size: 11px;
  }
  fieldset {
    border: 1px solid #2c3038;
    border-radius: 6px;
    margin: 0;
    padding: 6px 10px 8px;
    display: grid;
    gap: 3px;
  }
  legend {
    color: #9aa3ad;
    font-size: 12px;
    padding: 0 4px;
  }
  label {
    display: flex;
    gap: 6px;
    align-items: center;
    cursor: pointer;
  }
  label.gap {
    margin-top: 4px;
  }
  label.off {
    opacity: 0.5;
    cursor: default;
  }
  .actions {
    display: grid;
    gap: 6px;
  }
  button {
    padding: 6px 10px;
    text-align: left;
  }
</style>
