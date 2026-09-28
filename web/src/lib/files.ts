/** A name that is safe to use as a file name: characters that no platform allows become `_`. */
export function fileSafe(name: string): string {
  return name.replace(/[\\/:*?"<>|\u0000-\u001f]/g, '_').trim();
}

/** The file's name without its extension. */
export function stemOf(file: string): string {
  return file.replace(/\.[^.]+$/, '');
}
