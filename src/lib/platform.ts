/**
 * The platform the desk is running on, as the webview reports it. The native
 * side is the authority for anything that matters (the recovery combination
 * comes from `recovery_status`); this is for copy that reads differently per
 * platform before, or without, an answer.
 */
export type Platform = 'macos' | 'windows' | 'linux';

const hint = typeof navigator !== 'undefined' ? `${navigator.platform ?? ''} ${navigator.userAgent ?? ''}` : '';

export const PLATFORM: Platform = /Mac|iPhone|iPad/i.test(hint) ? 'macos' : /Win/i.test(hint) ? 'windows' : 'linux';

export const PLATFORM_NAME: Record<Platform, string> = { macos: 'macOS', windows: 'Windows', linux: 'Linux' };

/** What the platform calls switching applications: the key the lock talks about. */
export const SWITCH_KEY: Record<Platform, string> = { macos: '⌘Tab', windows: 'Alt+Tab', linux: 'Alt+Tab' };

/** Where a saved provider key lives on this platform (`keychain.rs`). */
export const STORE_NAME: Record<Platform, string> = { macos: 'the macOS Keychain', windows: 'the Windows Credential Manager', linux: "the desktop's secret service" };
