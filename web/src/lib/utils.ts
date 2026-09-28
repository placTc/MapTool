/** True when a click should add to or toggle the selection instead of replacing it. */
export function isMultiSelectClick(e: MouseEvent): boolean {
  return e.ctrlKey || e.metaKey || e.shiftKey;
}

/** Select all of an input's or textarea's text, for a click or a Tab into it. */
export function selectOnFocus(e: FocusEvent & { currentTarget: HTMLInputElement }) {
  e.currentTarget.select();
}

/** "12,345 (set on 6)" for a population known on `known` of the selection; "not set" if none. */
export function populationSummary(total: number, known: number): string {
  return known ? `${total.toLocaleString()} (set on ${known})` : 'not set';
}

/** Ask `message`, then run `action` if the user confirms — the shape every delete confirmation shares. */
export function confirmAction(message: string, action: () => void): void {
  if (confirm(message)) action();
}

/** Ids as the Uint32Array the WASM boundary expects. */
export function toUint32Array(ids: Iterable<number>): Uint32Array {
  return Uint32Array.from(ids);
}
