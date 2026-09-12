/**
 * The recovery ladder as the desk documents it everywhere: the key
 * combination, the four console commands, and what stands behind them.
 * The Rust side (`recovery.rs`) names the combination for the platform it
 * runs on and is the source of truth; this copy is the preview's, and what
 * renders before the desk answers.
 */
import type { RecoveryStatus } from '../../ipc';

export const VALVE_PRESSES = 5;
export const VALVE_SECONDS = 10;

export const LADDER: { command: string; what: string }[] = [
  { command: 'unlock', what: 'Explains what will happen and issues a six-character challenge code.' },
  { command: 'confirm <code>', what: 'Type the code back. It proves a person is at the keyboard.' },
  { command: 'phrase <your escape phrase>', what: 'The break-glass phrase you set during setup.' },
  { command: 'release', what: 'Pauses the session with its work intact, breaks the streak, and frees the machine.' },
];

const MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? navigator.userAgent);
const WINDOWS = typeof navigator !== 'undefined' && /Win/.test(navigator.platform ?? navigator.userAgent);

/** The combination as the browser believes this platform names it. */
export function guessCombination(): RecoveryStatus['combination'] {
  if (MAC) {
    return {
      platform: 'macOS',
      label: 'Control + Option + Shift + U',
      keys: [{ glyph: '⌃', name: 'control' }, { glyph: '⌥', name: 'option' }, { glyph: '⇧', name: 'shift' }, { glyph: 'U', name: '' }],
      note: 'Registered with macOS as a system hot key. It works on every Space and over full-screen apps, and needs no permission.',
    };
  }
  return {
    platform: WINDOWS ? 'Windows' : 'Linux',
    label: 'Ctrl + Alt + Shift + U',
    keys: [{ glyph: 'Ctrl', name: '' }, { glyph: 'Alt', name: '' }, { glyph: 'Shift', name: '' }, { glyph: 'U', name: '' }],
    note: WINDOWS
      ? 'Registered with Windows as a system hot key. If another program already holds this combination, registration fails and the desk says so here.'
      : 'Registered with the X11 server. Wayland desktops do not hand global shortcuts to applications; there the desk says so here, and the release token and the escape hatch remain.',
  };
}

/** What the guide shows before, or without, the desk's answer. */
export function guessStatus(registered = false): RecoveryStatus {
  return { combination: guessCombination(), registered, valve_presses: VALVE_PRESSES, valve_seconds: VALVE_SECONDS, ladder: LADDER };
}

/** The combination as it reads on this machine. */
export function shortcutLabel(): string {
  return guessCombination().label;
}
