<script lang="ts">
  /**
   * The profile as a thing you can carry: one archive out, one archive in.
   * An export is the whole record (classes, paths, lessons, study times,
   * progress) as a single JSON file; an import replaces this profile with
   * that file's, after a backup, and says what it holds before doing so.
   */
  import { Archive, Download, Upload, FolderOpen, Check } from 'lucide-svelte';
  import { api, type ArchiveSummary, type ExportResult, type ImportResult } from '$lib/ipc';
  import { app } from '$lib/stores.svelte';
  import { confirmDialog } from '$lib/components/dialog.svelte';
  import NodeCard from '$lib/components/NodeCard.svelte';

  let busy = $state('');
  let error = $state('');
  let exported = $state<ExportResult | null>(null);
  let imported = $state<ImportResult | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);

  const when = (iso: string) => {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  };
  const count = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
  const describe = (s: ArchiveSummary) =>
    `${count(s.classes, 'class', 'classes')}, ${count(s.lessons, 'lesson')}, ${count(s.schedules, 'study time')}; ${s.rows.toLocaleString()} rows in ${s.tables} tables, written by ${s.app_version} on ${when(s.exported_at)}.`;

  async function exportNow() {
    busy = 'export'; error = ''; imported = null;
    try {
      exported = await api.exportProfile();
      app.notify(`Exported to ${exported.path}`, { label: 'Reveal', run: () => void api.revealExport(exported!.path) });
    } catch (e) { error = String(e); }
    finally { busy = ''; }
  }

  async function importFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    busy = 'inspect'; error = ''; exported = null;
    let document = '';
    let summary: ArchiveSummary;
    try {
      document = await file.text();
      summary = await api.inspectArchive(document);
    } catch (e) { error = String(e); busy = ''; return; }
    busy = '';
    const ok = await confirmDialog(
      'Replace this profile with the archive?',
      `The file holds ${describe(summary)} Everything on this desk is replaced by it: the classes, their paths, every lesson and study time. The current record is backed up first, so it can be put back.`,
      { confirm: 'Replace the profile', danger: true },
    );
    if (!ok) return;
    busy = 'import';
    try {
      imported = await api.importProfile(document);
      await app.refresh();
      app.notify(`Profile imported: ${count(imported.summary.classes, 'class', 'classes')}, ${count(imported.summary.lessons, 'lesson')}. The previous record is kept as a backup.`);
    } catch (e) { error = String(e); }
    finally { busy = ''; }
  }
</script>

<NodeCard Icon={Archive} name="profile" badge="export · import" badgeTone="muted">
  <div class="head"><strong>Your data, on the move</strong><p>Everything this desk knows is one file: the classes, their starting points and paths, every lesson with its answers and notes, the study times and the progress. Export it to keep a copy or to carry it to another machine; import it there and the desk continues where this one left off. Keys stay out of it.</p></div>

  <div class="ways">
    <section class="way">
      <span class="art" aria-hidden="true"><Download size={16} /></span>
      <div class="text">
        <strong>Export the profile</strong>
        <p>Writes <code>principia-desk-&lt;date&gt;.json</code> under <code>Documents/Principia Desk</code>. The file carries a checksum, so a damaged copy is refused rather than half-applied.</p>
        {#if exported}
          <p class="done mono"><Check size={11} /> {exported.path} · {describe(exported.summary)}</p>
        {/if}
      </div>
      <div class="keys">
        <button type="button" class="cta mono-cta" disabled={!!busy} onclick={exportNow}><Download size={12} /> {busy === 'export' ? 'writing…' : 'Export'}</button>
        {#if exported}<button type="button" class="ghost mono-ghost small" onclick={() => void api.revealExport(exported!.path)}><FolderOpen size={11} /> reveal</button>{/if}
      </div>
    </section>

    <section class="way">
      <span class="art" aria-hidden="true"><Upload size={16} /></span>
      <div class="text">
        <strong>Import a profile</strong>
        <p>Choose an archive from another desk. The desk says what the file holds and asks before replacing anything; the record it replaces is backed up beside the database first. An archive must come from the same database version as this build.</p>
        {#if imported}
          <p class="done mono"><Check size={11} /> {imported.rows.toLocaleString()} rows restored · the previous record is at {imported.backup_path}</p>
        {/if}
      </div>
      <div class="keys">
        <button type="button" class="ghost mono-ghost" disabled={!!busy || !!app.state?.focus} onclick={() => fileInput?.click()}><Upload size={12} /> {busy === 'inspect' ? 'reading…' : busy === 'import' ? 'importing…' : 'Choose a file'}</button>
        <input bind:this={fileInput} type="file" accept=".json,application/json" hidden onchange={importFile} />
      </div>
    </section>
  </div>
  {#if app.state?.focus}<p class="note mono">Finish, pause or skip the active session before importing a profile.</p>{/if}
  {#if error}<p class="error mono" role="alert">{error}</p>{/if}
</NodeCard>

<style>
  .head { margin-bottom: 14px; } .head strong { display: block; font-size: 14px; font-weight: 500; } .head p { margin: 5px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; max-width: 76ch; }
  .ways { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  .way { display: grid; grid-template-columns: 38px minmax(0, 1fr); grid-template-rows: auto auto; gap: 8px 12px; padding: 14px; background: var(--bg); border: 1px solid var(--node-border); border-radius: var(--radius-panel); }
  .art { display: grid; place-items: center; width: 38px; height: 38px; border-radius: var(--radius-control); background: var(--surface-2); border: 1px solid var(--node-border); color: var(--accent); }
  .text { min-width: 0; } .text strong { display: block; font-size: 12.5px; font-weight: 500; } .text p { margin: 4px 0 0; font-size: 10.5px; line-height: 1.6; color: var(--muted); }
  .text code { font-family: var(--font-mono); font-size: 10px; color: var(--fg); }
  .keys { grid-column: 2; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .done { display: flex; align-items: flex-start; gap: 6px; margin-top: 8px !important; color: var(--led-ok) !important; font-size: 9.5px !important; overflow-wrap: anywhere; }
  .note { margin: 10px 0 0; font-size: 9.5px; color: var(--muted); }
  .error { color: var(--led-err); font-size: 10.5px; margin-top: 10px; overflow-wrap: anywhere; }
  @media (max-width: 760px) { .ways { grid-template-columns: 1fr; } }
</style>
