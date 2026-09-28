<script lang="ts">
  import { fileSafe } from '../lib/files';

  interface Props {
    /** The suggested file name, without the extension. */
    initial: string;
    onsave: (filename: string) => void;
    oncancel: () => void;
  }
  let { initial, onsave, oncancel }: Props = $props();

  const EXTENSION = '.maptool';
  // The dialog is created fresh each time it opens, so the suggestion is only read once.
  // svelte-ignore state_referenced_locally
  let value = $state(initial);
  /** What was typed, made safe, without a `.maptool` the user typed themselves. */
  const stem = $derived(fileSafe(value).replace(/\.maptool$/i, '').trim());
  const filename = $derived(stem ? stem + EXTENSION : '');

  function submit(e: Event) {
    e.preventDefault();
    if (filename) onsave(filename);
  }

  function focusAndSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && oncancel()} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && oncancel()}>
  <div class="dialog" role="dialog" aria-label="Save map as">
  <form onsubmit={submit}>
    <h3>Save map as</h3>
    <label>
      File name
      <span class="field">
        <input type="text" bind:value use:focusAndSelect spellcheck="false" autocomplete="off" />
        <span class="ext">{EXTENSION}</span>
      </span>
    </label>
    <p class="hint">
      {#if filename}Saves <code>{filename}</code>, with the map's states, countries, regions and province details.{:else}Type a file name.{/if}
      Your browser puts it in its download folder, unless it is set to ask where.
    </p>
    <div class="actions">
      <button type="button" class="secondary" onclick={oncancel}>Cancel</button>
      <button type="submit" disabled={!filename}>Save</button>
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
  .field {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input {
    flex: 1;
    background: #14161a;
    color: inherit;
    border: 1px solid #363b45;
    border-radius: 4px;
    padding: 6px 8px;
    font: inherit;
    min-width: 0;
  }
  .ext {
    color: #9aa3ad;
  }
  .hint {
    margin: 0;
    color: #9aa3ad;
    font-size: 12px;
  }
  code {
    color: #e6e8eb;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    background: #2f6fed;
    color: white;
    border: 0;
    border-radius: 4px;
    padding: 6px 14px;
    cursor: pointer;
    font: inherit;
  }
  button.secondary {
    background: #363b45;
    color: inherit;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
