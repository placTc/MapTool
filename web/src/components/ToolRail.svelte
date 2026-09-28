<script lang="ts">
  import { TOOLS, type ToolId } from '../lib/tools';

  interface Props {
    active: ToolId;
    onselect: (id: ToolId) => void;
  }
  let { active, onselect }: Props = $props();
</script>

<div class="rail" role="toolbar" aria-label="Tools" aria-orientation="vertical">
  {#each TOOLS as tool (tool.id)}
    <button
      class:on={active === tool.id}
      aria-label={tool.label}
      aria-pressed={active === tool.id}
      title="{tool.label} ({tool.key.toUpperCase()}): {tool.hint}"
      onclick={() => onselect(tool.id)}
    >
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {@html tool.icon}
      </svg>
      <span>{tool.label}</span>
    </button>
  {/each}
</div>

<style>
  .rail {
    width: 58px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 6px;
    background: #1a1d22;
    border-right: 1px solid #2c3038;
    box-sizing: border-box;
  }
  button {
    display: grid;
    justify-items: center;
    gap: 2px;
    padding: 7px 0 5px;
    background: transparent;
    color: #b6bcc4;
    border: 1px solid transparent;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
  }
  button:hover {
    background: #262a32;
    color: #e6e8eb;
  }
  button.on {
    background: #2f6fed;
    color: white;
  }
  span {
    font-size: 10px;
    line-height: 1;
  }
</style>
