/**
 * The tools in the tool sidebar. To add one: add an entry here, then give the editor the
 * behavior for its id (what dragging on the map does, and any options panel it needs).
 */
export interface ToolDef {
  id: 'pan' | 'box';
  label: string;
  /** Pressing this key (when not typing in a field) picks the tool. */
  key: string;
  /** What the tool does, shown as a tooltip. */
  hint: string;
  /** The inside of a 24x24 SVG icon, drawn with the current text color. Our own markup, never user input. */
  icon: string;
}

export type ToolId = ToolDef['id'];

export const TOOLS: readonly ToolDef[] = [
  {
    id: 'pan',
    label: 'Pan',
    key: 'p',
    hint: 'Drag to move the map. Clicking still selects. Right-drag pans with any tool.',
    icon: '<path d="M12 3v18M3 12h18M12 3l-3 3M12 3l3 3M12 21l-3-3M12 21l3-3M3 12l3-3M3 12l3 3M21 12l-3-3M21 12l-3 3" />',
  },
  {
    id: 'box',
    label: 'Box',
    key: 'b',
    hint: 'Drag a box to select. Right-drag or middle-drag still pans.',
    icon: '<rect x="4" y="4" width="16" height="16" stroke-dasharray="3 2.5" /><path d="M9 12h6M12 9v6" />',
  },
];
