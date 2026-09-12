import type { ModelOption } from '$lib/contracts/agents';

export function modelPage(models: ModelOption[], query: string, page: number, freeOnly = false, size = 5) {
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const filtered = models.filter(m => (!freeOnly || m.free) && words.every(word => `${m.label} ${m.id}`.toLocaleLowerCase().includes(word)));
  const pages = Math.max(1, Math.ceil(filtered.length / size));
  const current = Math.max(0, Math.min(page, pages - 1));
  return { items: filtered.slice(current * size, (current + 1) * size), count: filtered.length, pages, current };
}
export function shortlistOptions(models: string[], current: string) {
  return [...new Set([...(current ? [current] : []), ...models])].map(id => ({ value: id, label: id === 'default' ? 'Runner default' : id, description: !models.includes(id) ? 'Current selection · retained until you choose another model' : undefined }));
}
