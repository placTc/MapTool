<script lang="ts">
  interface Props {
    mode: 'replace' | 'add' | 'remove';
    whole: boolean;
    types: 'all' | 'land' | 'sea';
    skipInStates: boolean;
    /** Whether the selection has anything in it. */
    hasSelection: boolean;
    ondeselectinstates: () => void;
    onselectunassigned: () => void;
    oninvert: () => void;
    onclear: () => void;
  }
  let {
    mode = $bindable(),
    whole = $bindable(),
    types = $bindable(),
    skipInStates = $bindable(),
    hasSelection,
    ondeselectinstates,
    onselectunassigned,
    oninvert,
    onclear,
  }: Props = $props();
</script>

<section class="panel">
  <h2>Box selection</h2>
  <p class="hint">Drag a box on the map. Hold <kbd>Shift</kbd> to add or <kbd>Alt</kbd> to remove, whatever the mode. Middle-drag or hold <kbd>Space</kbd> to pan.</p>

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
    <label class="gap"><input type="checkbox" bind:checked={skipInStates} /> Skip provinces already in a state</label>
  </fieldset>

  <div class="actions">
    <button onclick={ondeselectinstates} disabled={!hasSelection} title="Take every province that is already in a state out of the selection">
      Deselect provinces already in states
    </button>
    <button onclick={onselectunassigned} title="Select every province that is in no state">Select all unassigned provinces</button>
    <button onclick={oninvert} title="Select the provinces that are not selected, and the other way round">Invert selection</button>
    <button onclick={onclear} disabled={!hasSelection}>Clear selection</button>
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
  .hint {
    margin: 0;
    color: #9aa3ad;
    font-size: 12px;
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
  .actions {
    display: grid;
    gap: 6px;
  }
  button {
    background: #363b45;
    color: inherit;
    border: 0;
    border-radius: 4px;
    padding: 6px 10px;
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
