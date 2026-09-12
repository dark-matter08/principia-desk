import { isTauri } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';

/** Documentation opens in the system browser from the packaged desktop app. */
export async function openRunnerLink(event: MouseEvent) {
  if (!isTauri()) return;
  event.preventDefault();
  const link = event.currentTarget as HTMLAnchorElement;
  await openUrl(link.href);
}
