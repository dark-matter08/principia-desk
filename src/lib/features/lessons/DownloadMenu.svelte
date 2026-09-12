<script lang="ts">
  import { api, type ProgressEntry } from '$lib/ipc';
  import { app } from '$lib/stores.svelte';
  import { Download, FileText, Table2 } from 'lucide-svelte';
  import { renderLessonPdf } from './pdf';

  let {
    source,
    ownerId,
    compact = false,
  }: {
    source: ProgressEntry['source'];
    ownerId: string;
    /** The lesson header's small mono style rather than the ghost button. */
    compact?: boolean;
  } = $props();

  let open = $state(false);
  let busy = $state<'' | 'pdf' | 'csv'>('');
  let root = $state<HTMLElement | undefined>(undefined);

  function keyNote(answerKey: boolean, verb: string) {
    return answerKey ? ' and the answer key' : `; the answer key joins once you ${verb} the check`;
  }

  async function pdf() {
    if (busy) return;
    busy = 'pdf';
    open = false;
    try {
      const document = await api.getLessonDocument(source, ownerId);
      const bytes = await renderLessonPdf(document);
      const saved = await api.saveLessonPdf(source, ownerId, bytes);
      app.notify(`Saved ${saved.file_name} in ${document.class_label} with ${saved.questions} questions${keyNote(saved.answer_key, 'submit')}.`, {
        label: 'Show file',
        run: () => void api.revealExport(saved.path),
      });
    } catch (error) {
      app.error = String(error);
    } finally {
      busy = '';
    }
  }

  async function csv() {
    if (busy) return;
    busy = 'csv';
    open = false;
    try {
      const saved = await api.exportLessonCsv(source, ownerId);
      app.notify(`Saved ${saved.file_name} with ${saved.questions} questions${keyNote(saved.answer_key, 'submit')}.`, {
        label: 'Show file',
        run: () => void api.revealExport(saved.path),
      });
    } catch (error) {
      app.error = String(error);
    } finally {
      busy = '';
    }
  }

  function onDocumentClick(event: MouseEvent) {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }
</script>

<svelte:document onclick={onDocumentClick} />

<div class="download" class:compact bind:this={root}>
  <button
    type="button"
    class={compact ? 'chat-button' : 'ghost mono-ghost'}
    aria-haspopup="menu"
    aria-expanded={open}
    disabled={!!busy}
    title="Save this lesson to your documents"
    onclick={() => (open = !open)}
  >
    <Download size={compact ? 12 : 13} /> {busy === 'pdf' ? 'laying out…' : busy === 'csv' ? 'saving…' : 'download'}
  </button>
  {#if open}
    <div class="menu" role="menu">
      <button type="button" role="menuitem" onclick={pdf}><FileText size={13} /><span><strong>Lesson as PDF</strong><small>The reading, your work and the check, in the desk's type.</small></span></button>
      <button type="button" role="menuitem" onclick={csv}><Table2 size={13} /><span><strong>Questions as CSV</strong><small>One row per section, resource and question, for a spreadsheet or a deck.</small></span></button>
    </div>
  {/if}
</div>

<style>
  .download { position: relative; display: inline-flex; }
  .download > button { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 30;
    min-width: 300px;
    padding: 6px;
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    background: var(--surface);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.3);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .menu button {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    padding: 9px 10px;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .menu button:hover, .menu button:focus-visible { background: var(--surface-2); outline: none; }
  .menu button :global(svg) { flex: none; margin-top: 2px; color: var(--accent); }
  .menu strong { display: block; font-size: 12px; font-weight: 500; }
  .menu small { display: block; margin-top: 2px; font-size: 10.5px; line-height: 1.4; color: var(--muted); }
</style>
