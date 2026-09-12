export type Destination = 'today' | 'classes' | 'progress' | 'logs' | 'settings';
export const DESTINATIONS: readonly { id: Destination; label: string }[] = [
  { id: 'today', label: 'Today' },
  { id: 'classes', label: 'Classes' },
  { id: 'progress', label: 'Progress' },
  { id: 'logs', label: 'Logs' },
  { id: 'settings', label: 'Settings' },
];
