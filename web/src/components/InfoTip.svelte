<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** What the icon is for, read out by screen readers. */
    label: string;
    /** The explanation shown in the card. */
    children: Snippet;
  }
  let { label, children }: Props = $props();

  const id = `info-${Math.random().toString(36).slice(2, 9)}`;
  let hovered = $state(false);
  let focused = $state(false);
  const open = $derived(hovered || focused);
  let wrap = $state<HTMLElement>();
  let pop = $state<HTMLElement>();
  let pos = $state({ left: 0, top: 0 });

  /** Put the card under the icon, kept inside the window. */
  function place() {
    if (!wrap || !pop) return;
    const r = wrap.getBoundingClientRect();
    const w = pop.offsetWidth;
    pos = { left: Math.min(Math.max(8, r.left + r.width / 2 - w / 2), window.innerWidth - w - 8), top: r.bottom };
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      e.stopPropagation();
      hovered = focused = false;
      (document.activeElement as HTMLElement | null)?.blur();
    }
  }
</script>

<svelte:window onresize={place} />

<!-- The card is a child of the wrapper, so moving the pointer from the icon into the card keeps it open. -->
<span
  class="wrap"
  bind:this={wrap}
  onpointerenter={() => {
    place();
    hovered = true;
  }}
  onpointerleave={() => (hovered = false)}
  onfocusin={() => {
    place();
    focused = true;
  }}
  onfocusout={() => (focused = false)}
  {onkeydown}
  role="presentation"
>
  <button type="button" class="icon" aria-label={label} aria-describedby={id}>i</button>
  <div {id} class="pop" class:open role="tooltip" bind:this={pop} style:left="{pos.left}px" style:top="{pos.top}px">
    <div class="card">{@render children()}</div>
  </div>
</span>

<style>
  .wrap {
    display: inline-flex;
    align-items: center;
  }
  .icon {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid #4a505b;
    background: #262a32;
    color: #b6bcc4;
    font: italic 700 12px/1 Georgia, serif;
    cursor: help;
  }
  .icon:hover,
  .icon:focus-visible {
    background: #2f6fed;
    border-color: #2f6fed;
    color: white;
    outline: none;
  }
  .pop {
    position: fixed;
    z-index: 20;
    width: min(400px, calc(100vw - 16px));
    /* Room for the card to sit a little below the icon without a gap the pointer would fall through. */
    padding-top: 8px;
    visibility: hidden;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .pop.open {
    visibility: visible;
    opacity: 1;
  }
  .card {
    background: #1d2026;
    border: 1px solid #363b45;
    border-radius: 8px;
    padding: 12px 14px;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.5);
    font-size: 12.5px;
    line-height: 1.45;
    color: #d3d7dc;
    font-style: normal;
    font-weight: 400;
  }
  .card :global(h4) {
    margin: 0 0 6px;
    font-size: 13px;
    color: #e6e8eb;
  }
  .card :global(p) {
    margin: 6px 0;
  }
  .card :global(ul) {
    margin: 6px 0;
    padding-left: 18px;
  }
  .card :global(code),
  .card :global(pre) {
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .card :global(pre) {
    margin: 6px 0;
    padding: 6px 8px;
    background: #14161a;
    border: 1px solid #2c3038;
    border-radius: 5px;
    overflow-x: auto;
    user-select: all;
  }
  .card :global(.muted) {
    color: #9aa3ad;
  }
</style>
