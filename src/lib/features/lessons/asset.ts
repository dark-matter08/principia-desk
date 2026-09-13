/**
 * A file the desk wrote (a voiced segment, a rendered preview) as an address
 * the webview may load: Tauri's asset protocol on the desk, the path as it
 * is in the browser preview, where nothing is served anyway.
 */
import { convertFileSrc } from '@tauri-apps/api/core';

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function assetUrl(path: string): string {
  return isTauri ? convertFileSrc(path) : path;
}
