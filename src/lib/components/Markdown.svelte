<script module lang="ts">
  import { marked, type Tokens } from 'marked';

  // Registered once per module load, not per component instance. Mermaid
  // fences render into a placeholder the instance script fills in after
  // mount — marked itself never touches the browser DOM.
  marked.use({
    renderer: {
      code({ text, lang }: Tokens.Code) {
        const language = (lang || '').trim().split(/\s+/)[0].toLowerCase();
        if (language === 'mermaid') {
          const encoded = encodeURIComponent(text);
          return `<div class="mermaid-block" data-mermaid="${encoded}" role="img" aria-label="diagram (rendering)"><p class="mermaid-status mono">rendering diagram…</p></div>`;
        }
        return `<figure class="code-frame"><figcaption class="code-label mono">${escapeHtml(language || 'code')}</figcaption><pre><code>${escapeHtml(text)}</code></pre></figure>`;
      },
    },
  });

  let mermaidMod: Promise<typeof import('mermaid')> | null = null;
  function loadMermaid() {
    if (!mermaidMod) {
      mermaidMod = import('mermaid').then((mod) => {
        mod.default.initialize({
          startOnLoad: false,
          securityLevel: 'strict',
          theme: 'dark',
          fontFamily: 'inherit',
        });
        return mod;
      });
    }
    return mermaidMod;
  }

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }
</script>

<script lang="ts">
  import DOMPurify from 'dompurify';
  import { tick } from 'svelte';
  import { decorateLesson, type LessonLevel, type LessonSection } from '../features/lessons/sections';

  let {
    markdown = '',
    locked = false,
    inline = false,
    compact = false,
    lesson = false,
    level = 'standard',
    onsections,
  }: {
    markdown: string;
    locked?: boolean;
    inline?: boolean;
    compact?: boolean;
    /** Decorate the ten lesson sections: icons, reading hints, callouts. */
    lesson?: boolean;
    /** Names the sections for the learner's level. */
    level?: LessonLevel;
    /** Told the sections found each time a lesson renders, for a section map. */
    onsections?: (sections: LessonSection[]) => void;
  } = $props();

  let container = $state<HTMLElement | undefined>(undefined);

  const html = $derived(
    DOMPurify.sanitize(
      (inline && !compact
        ? (marked.parseInline(markdown, { async: false }) as string)
        : (marked.parse(markdown, { async: false }) as string)) || '',
      { ADD_TAGS: ['foreignObject'], ADD_ATTR: ['data-mermaid'] }
    )
  );

  $effect(() => {
    // Re-run whenever `html` changes — new diagram placeholders appear
    // each time the markdown source changes (new course, new question).
    void html;
    enhance();
  });

  async function enhance() {
    await tick();
    const root = container;
    if (!root) return;
    if (lesson) onsections?.(decorateLesson(root, level));
    await renderDiagrams(root);
  }

  async function renderDiagrams(root: HTMLElement) {
    const blocks = Array.from(
      root.querySelectorAll<HTMLElement>('.mermaid-block:not([data-rendered])')
    );
    if (blocks.length === 0) return;
    let mermaid: typeof import('mermaid').default;
    try {
      mermaid = (await loadMermaid()).default;
    } catch {
      for (const block of blocks) {
        block.dataset.rendered = '1';
        showSource(block, 'diagram renderer unavailable');
      }
      return;
    }
    for (const block of blocks) {
      block.dataset.rendered = '1';
      const source = decodeURIComponent(block.dataset.mermaid ?? '');
      const id = `mmd-${Math.random().toString(36).slice(2)}`;
      try {
        const { svg } = await mermaid.render(id, source);
        block.innerHTML = svg;
        block.setAttribute('aria-label', 'diagram');
      } catch {
        showSource(block, 'diagram could not be rendered; showing source');
      }
    }
  }

  function showSource(block: HTMLElement, note: string) {
    const source = decodeURIComponent(block.dataset.mermaid ?? '');
    block.setAttribute('aria-label', note);
    block.innerHTML = `<p class="mermaid-status mono">${escapeHtml(note)}</p><pre class="mermaid-fallback mono">${escapeHtml(source)}</pre>`;
  }

  function interceptLinks(node: HTMLElement) {
    const handler = (e: Event) => {
      const a = (e.target as HTMLElement).closest('a');
      if (a) {
        e.preventDefault();
        // Links never navigate the kiosk webview; resources open post-session.
      }
    };
    node.addEventListener('click', handler);
    return { destroy: () => node.removeEventListener('click', handler) };
  }
</script>

<div
  class="md"
  class:inline
  class:compact
  class:locked
  class:lesson
  bind:this={container}
  use:interceptLinks
>
  {@html html}
</div>

<style>
  .md {
    user-select: text;
    /* The reader sets --reading-font from its text-size control. */
    font-size: var(--reading-font, 16px);
    line-height: 1.75;
  }
  .md.inline {
    display: inline;
    line-height: inherit;
    font-size: inherit;
  }
  .md.inline :global(p) {
    display: inline;
    margin: 0;
  }
  .md.compact {
    font-size: inherit;
    line-height: inherit;
    min-width: 0;
  }
  .md.compact :global(p) {
    margin: 0 0 0.65em;
  }
  .md.compact :global(p:last-child),
  .md.compact :global(pre:last-child),
  .md.compact :global(ul:last-child),
  .md.compact :global(ol:last-child) {
    margin-bottom: 0;
  }
  .md.compact :global(pre) {
    margin: 0.75em 0;
    padding: 12px 14px;
    border-radius: var(--radius-control);
    line-height: 1.55;
    max-width: 100%;
  }
  .md.compact :global(pre code) {
    display: block;
    width: max-content;
    min-width: 100%;
    font-size: 0.82em;
    white-space: pre;
  }
  .md.compact :global(ul),
  .md.compact :global(ol) {
    margin: 0.65em 0;
  }
  .md :global(h1),
  .md :global(h2),
  .md :global(h3) {
    font-family: var(--font-display);
    font-weight: 500;
    margin: 1.6em 0 0.5em;
    line-height: 1.3;
  }
  .md :global(h2) {
    font-size: 1.5em;
    border-bottom: 1px solid var(--border);
    padding-bottom: 8px;
  }
  .md :global(h3) {
    font-size: 1.18em;
  }
  .md :global(h4) {
    font-size: 1em;
    margin: 1.4em 0 0.4em;
  }
  .md :global(p) {
    margin: 0 0 1em;
  }
  .md :global(code) {
    font-family: var(--font-mono);
    font-size: 0.88em;
    background: var(--surface);
    padding: 2px 6px;
    border-radius: var(--radius-control);
  }
  .md :global(pre) {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    padding: 16px;
    overflow-x: auto;
  }
  .md :global(pre code) {
    background: none;
    padding: 0;
  }
  .md :global(.code-frame) {
    margin: 1em 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    background: var(--surface);
    overflow: hidden;
    max-width: 100%;
  }
  .md.compact :global(.code-frame) {
    margin: 0.75em 0;
    border-radius: var(--radius-control);
  }
  .md :global(.code-label) {
    display: block;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    color: var(--faint);
    font-size: 10px;
    line-height: 1.2;
    letter-spacing: 0.7px;
    text-transform: uppercase;
  }
  .md :global(.code-frame pre) {
    margin: 0;
    border: 0;
    border-radius: 0;
  }
  .md :global(a) {
    color: var(--accent);
    text-decoration: none;
    border-bottom: 1px dashed var(--accent);
    cursor: default;
  }
  .md :global(blockquote) {
    border-left: 3px solid var(--accent);
    margin: 1em 0;
    padding: 4px 0 4px 18px;
    color: var(--muted);
  }
  .md :global(ul),
  .md :global(ol) {
    padding-left: 1.4em;
  }
  .md :global(li) {
    margin-bottom: 0.4em;
  }
  .md :global(table) {
    border-collapse: collapse;
    margin: 1em 0;
  }
  .md :global(th),
  .md :global(td) {
    border: 1px solid var(--border);
    padding: 8px 12px;
    text-align: left;
  }
  .md :global(.mermaid-block) {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    margin: 1.2em 0;
    padding: 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-panel);
    overflow-x: auto;
  }
  .md :global(.mermaid-block svg) {
    max-width: 100%;
  }
  .md :global(.mermaid-status) {
    font-size: 11px;
    color: var(--faint);
    margin: 0;
  }
  .md :global(.mermaid-fallback) {
    width: 100%;
    font-size: 12px;
    white-space: pre-wrap;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    padding: 10px;
    margin: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .md :global(.mermaid-block) {
      transition: none;
    }
  }

  /* --- Lesson shape: numbered, iconed sections with a reading hint ------- */
  /* The lesson opens with its title as a heading; the shell already shows it. */
  .md.lesson :global(> h1:first-child) {
    display: none;
  }
  .md.lesson :global(h2.lesson-heading) {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 2.2em;
    padding-bottom: 10px;
    scroll-margin-top: 16px;
  }
  .md.lesson :global(h2.lesson-heading:first-of-type) {
    margin-top: 0.8em;
  }
  .md.lesson :global(.lesson-heading-mark) {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px 7px 8px;
    border: 1px solid var(--node-border, var(--border));
    border-radius: var(--radius-detail, 8px);
    background: var(--surface);
    color: var(--accent);
    font-family: var(--font-mono);
  }
  .md.lesson :global(.lesson-heading-index) {
    font-size: 10px;
    letter-spacing: 1px;
    color: var(--faint);
  }
  .md.lesson :global(.lesson-heading-title) {
    min-width: 0;
  }
  .md.lesson :global(.lesson-hint) {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 14px;
    margin: -2px 0 1.1em;
    padding: 8px 12px;
    border-left: 2px solid var(--accent);
    border-radius: 0 var(--radius-control, 6px) var(--radius-control, 6px) 0;
    background: color-mix(in srgb, var(--accent) 7%, transparent);
    font-size: 0.82em;
    line-height: 1.5;
    color: var(--muted);
  }
  .md.lesson :global(.lesson-hint-time) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--accent);
    font-size: 0.85em;
    letter-spacing: 0.6px;
    white-space: nowrap;
  }
  .md.lesson :global(.lesson-icon) {
    flex: none;
    vertical-align: -0.15em;
  }

  /* Point form breathes: lists carry the lesson, so they get room. */
  .md.lesson :global(ul),
  .md.lesson :global(ol) {
    padding-left: 1.5em;
    margin: 0.4em 0 1.1em;
  }
  .md.lesson :global(li) {
    margin-bottom: 0.45em;
    line-height: 1.6;
  }
  .md.lesson :global(li::marker) {
    color: var(--accent);
  }
  .md.lesson :global(li > ul),
  .md.lesson :global(li > ol) {
    margin: 0.35em 0 0.2em;
  }
  .md.lesson :global(p) {
    margin: 0 0 0.9em;
  }
  .md.lesson :global(table) {
    width: 100%;
    font-size: 0.9em;
    line-height: 1.45;
  }
  .md.lesson :global(th) {
    background: var(--surface);
    font-family: var(--font-mono);
    font-size: 0.78em;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .md.lesson :global(.lesson-caption) {
    margin: -0.6em 0 1.4em;
    text-align: center;
    font-size: 0.82em;
    color: var(--muted);
  }

  /* Callouts: the points that matter, told apart by kind. */
  .md.lesson :global(blockquote.lesson-callout) {
    --callout: var(--accent);
    margin: 1.1em 0;
    padding: 12px 16px 12px 16px;
    border: 1px solid color-mix(in srgb, var(--callout) 35%, var(--border));
    border-left: 3px solid var(--callout);
    border-radius: var(--radius-panel, 10px);
    background: color-mix(in srgb, var(--callout) 6%, var(--surface));
    color: var(--fg);
    font-size: 0.95em;
  }
  .md.lesson :global(blockquote.lesson-callout p) {
    margin: 0;
  }
  .md.lesson :global(blockquote.lesson-callout p + p) {
    margin-top: 0.5em;
  }
  .md.lesson :global(.lesson-callout-label) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 6px;
    padding: 2px 8px 2px 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--callout) 18%, transparent);
    color: var(--callout);
    font-family: var(--font-mono);
    font-size: 0.72em;
    font-weight: 500;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    vertical-align: 0.1em;
  }
  .md.lesson :global(blockquote.lesson-callout-warn) {
    --callout: var(--bad-fg);
  }
  .md.lesson :global(blockquote.lesson-callout-try) {
    --callout: var(--ok-fg);
  }
  .md.lesson :global(blockquote.lesson-callout-example) {
    --callout: var(--warn-fg);
  }
  .md.lesson :global(blockquote.lesson-callout-decision) {
    --callout: var(--fg);
  }
  .md.lesson :global(blockquote.lesson-callout-evidence) {
    --callout: var(--muted);
  }
</style>
