/**
 * A lesson as a PDF, laid out by the desk from the same document the CSV
 * flattens: the reading with its sections, hints, callouts, tables, code and
 * diagrams; the exercise with the learner's own work; the check with its
 * key once the check is done; and the reading list.
 *
 * The layout engine is pdfmake, fed the lesson's Markdown as parsed by the
 * same parser the reader uses, so what prints is what was read. Fonts are
 * the desk's own, embedded from static WOFF instances built for print.
 */
import { marked, type Token, type Tokens } from 'marked';
import type { LessonDocument, QuestionDocument } from '$lib/ipc';
import { CALLOUTS, LESSON_SECTIONS, displayTitle, identify, parseHint, type LessonLevel } from './sections';
import { renderDiagramImage } from './diagram';

type Content = Record<string, unknown> | string;
type Inline = string | Record<string, unknown>;

const PAGE_WIDTH = 595.28;
const MARGIN_X = 54;
const TEXT_WIDTH = PAGE_WIDTH - MARGIN_X * 2;
const COLORS = {
  ink: '#1d1b1e',
  muted: '#5f5e5a',
  faint: '#9a978f',
  rule: '#d9d6cc',
  panel: '#f4f2ec',
  accent: '#b86c0c',
  good: '#0c6b52',
  bad: '#a12b2b',
  violet: '#4b41a6',
};
const CALLOUT_COLORS: Record<string, string> = {
  key: COLORS.accent,
  warn: COLORS.bad,
  try: COLORS.good,
  example: COLORS.violet,
  decision: COLORS.ink,
  evidence: COLORS.muted,
};
const FONT_FILES: Record<string, string> = {
  'inter-regular.woff': '/fonts/pdf/inter-regular.woff',
  'inter-bold.woff': '/fonts/pdf/inter-bold.woff',
  'inter-italic.woff': '/fonts/pdf/inter-italic.woff',
  'inter-bold-italic.woff': '/fonts/pdf/inter-bold-italic.woff',
  'jetbrains-mono-regular.woff': '/fonts/pdf/jetbrains-mono-regular.woff',
  'jetbrains-mono-bold.woff': '/fonts/pdf/jetbrains-mono-bold.woff',
  'fraunces-medium.woff': '/fonts/pdf/fraunces-medium.woff',
  'fraunces-semibold.woff': '/fonts/pdf/fraunces-semibold.woff',
};

let engine: Promise<import('pdfmake/build/pdfmake').PdfMake> | null = null;

/** pdfmake with the desk's fonts loaded, once per session. */
async function loadEngine() {
  if (!engine) {
    engine = (async () => {
      const { default: pdfmake } = await import('pdfmake/build/pdfmake');
      const vfs: Record<string, string> = {};
      await Promise.all(
        Object.entries(FONT_FILES).map(async ([name, url]) => {
          const response = await fetch(url);
          if (!response.ok) throw new Error(`The font ${name} could not be loaded (${response.status}).`);
          vfs[name] = base64(await response.arrayBuffer());
        }),
      );
      pdfmake.addVirtualFileSystem(vfs);
      pdfmake.setUrlAccessPolicy(() => false);
      pdfmake.setFonts({
        Inter: { normal: 'inter-regular.woff', bold: 'inter-bold.woff', italics: 'inter-italic.woff', bolditalics: 'inter-bold-italic.woff' },
        Mono: { normal: 'jetbrains-mono-regular.woff', bold: 'jetbrains-mono-bold.woff', italics: 'jetbrains-mono-regular.woff', bolditalics: 'jetbrains-mono-bold.woff' },
        Display: { normal: 'fraunces-medium.woff', bold: 'fraunces-semibold.woff', italics: 'fraunces-medium.woff', bolditalics: 'fraunces-semibold.woff' },
      });
      return pdfmake;
    })().catch((error) => {
      engine = null;
      throw error;
    });
  }
  return engine;
}

function base64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

/** Lay out the lesson and return the PDF bytes. */
export async function renderLessonPdf(document: LessonDocument): Promise<Uint8Array> {
  const pdfmake = await loadEngine();
  const content: Content[] = [...cover(document)];
  content.push(...(await markdownBlocks(document.markdown, { lesson: true, level: document.level })));
  if (document.language) content.push(...languageBlocks(document));
  if (document.exercise) content.push(...(await exerciseBlocks(document)));
  if (document.questions.length) content.push(...questionBlocks(document));
  if (document.resources.length) content.push(...resourceBlocks(document));
  const definition = {
    pageSize: 'A4',
    pageMargins: [MARGIN_X, 64, MARGIN_X, 64],
    info: { title: document.title, author: 'Principia Desk', subject: `${document.class_label} · ${document.topic}` },
    defaultStyle: { font: 'Inter', fontSize: 10, lineHeight: 1.35, color: COLORS.ink },
    header: (page: number) =>
      page === 1
        ? null
        : {
            columns: [
              { text: document.title, font: 'Mono', fontSize: 7, color: COLORS.faint, margin: [MARGIN_X, 28, 0, 0], width: '*' },
              { text: document.class_label.toUpperCase(), font: 'Mono', fontSize: 7, color: COLORS.faint, alignment: 'right', margin: [0, 28, MARGIN_X, 0], width: 'auto' },
            ],
          },
    footer: (page: number, pages: number) => ({
      columns: [
        { text: 'Principia Desk', font: 'Mono', fontSize: 7, color: COLORS.faint, margin: [MARGIN_X, 24, 0, 0], width: '*' },
        { text: `${page} / ${pages}`, font: 'Mono', fontSize: 7, color: COLORS.faint, alignment: 'right', margin: [0, 24, MARGIN_X, 0], width: 'auto' },
      ],
    }),
    content,
  };
  return pdfmake.createPdf(definition).getBuffer();
}

// --- Cover ------------------------------------------------------------------

function cover(document: LessonDocument): Content[] {
  const meta = [document.class_label, document.topic, document.category].filter(Boolean).join('  ·  ');
  const status = [
    document.date,
    document.level === 'beginner' ? 'written for a beginner' : '',
    document.status,
    document.score == null ? '' : `score ${Math.round(document.score * 100)}%`,
    document.tutor ? `tutor ${document.tutor}` : '',
  ]
    .filter(Boolean)
    .join('  ·  ');
  const blocks: Content[] = [
    { text: meta.toUpperCase(), font: 'Mono', fontSize: 7.5, color: COLORS.accent, characterSpacing: 0.8, margin: [0, 0, 0, 10] },
    { text: displayRuns(document.title), font: 'Display', fontSize: 24, lineHeight: 1.15, margin: [0, 0, 0, 8] },
    { text: status, font: 'Mono', fontSize: 8, color: COLORS.muted, margin: [0, 0, 0, 14] },
    rule(),
  ];
  if (document.research_note) blocks.push(notice('UNVERIFIED · NO DOCUMENTATION WAS RETRIEVED', document.research_note, COLORS.bad));
  if (document.review_notes.length) {
    blocks.push(notice("EDITOR'S NOTES", document.review_notes.map((note) => `• ${note}`).join('\n'), COLORS.accent));
  }
  return blocks;
}

function notice(label: string, body: string, color: string): Content {
  return {
    table: {
      widths: ['*'],
      body: [[{ stack: [{ text: label, font: 'Mono', fontSize: 7, color, characterSpacing: 0.6, margin: [0, 0, 0, 3] }, { text: body, fontSize: 9, color: COLORS.ink, lineHeight: 1.35 }], margin: [8, 6, 8, 6] }]],
    },
    layout: {
      hLineWidth: () => 0.6,
      vLineWidth: (i: number) => (i === 0 ? 2.5 : 0.6),
      hLineColor: () => color,
      vLineColor: () => color,
      paddingLeft: () => 4,
      paddingRight: () => 4,
    },
    margin: [0, 12, 0, 4],
  };
}

function rule(): Content {
  return { canvas: [{ type: 'line', x1: 0, y1: 0, x2: TEXT_WIDTH, y2: 0, lineWidth: 0.6, lineColor: COLORS.rule }], margin: [0, 2, 0, 10] };
}

// --- Markdown ---------------------------------------------------------------

interface BlockOptions {
  /** Decorate the ten lesson sections: numbers, hints, callouts. */
  lesson: boolean;
  /** Names the sections for the learner's level. */
  level?: LessonLevel;
}

async function markdownBlocks(markdown: string, options: BlockOptions): Promise<Content[]> {
  const tokens = marked.lexer(markdown);
  const blocks: Content[] = [];
  let sectionIndex = 0;
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token.type === 'heading') {
      const heading = token as Tokens.Heading;
      if (heading.depth === 1) continue; // the cover carries the title
      if (heading.depth === 2 && options.lesson) {
        sectionIndex += 1;
        const known = identify(heading.text);
        blocks.push({
          columns: [
            { text: String(sectionIndex).padStart(2, '0'), font: 'Mono', fontSize: 8, color: COLORS.accent, width: 22, margin: [0, 6, 0, 0] },
            { text: displayRuns(displayTitle(heading.text, options.level ?? 'standard')), font: 'Display', fontSize: 16, width: '*' },
          ],
          margin: [0, 18, 0, 4],
          ...(known ? { id: `section-${sectionIndex}` } : {}),
        });
        blocks.push(rule());
        // The reading hint: an italic-only paragraph right under the heading.
        const next = tokens[i + 1];
        const hint = next?.type === 'paragraph' ? hintOf(next as Tokens.Paragraph) : null;
        if (hint) {
          blocks.push({
            columns: [
              { text: `~${hint.minutes} min`, font: 'Mono', fontSize: 7.5, color: COLORS.accent, width: 'auto', margin: [0, 1, 8, 0] },
              { text: hint.text, fontSize: 8.5, italics: true, color: COLORS.muted, width: '*' },
            ],
            margin: [0, 0, 0, 8],
          });
          i += 1;
        }
        continue;
      }
      blocks.push({ text: inlineRuns(heading.tokens ?? []), bold: true, fontSize: heading.depth === 2 ? 13 : heading.depth === 3 ? 11 : 10, margin: [0, 12, 0, 4] });
      continue;
    }
    blocks.push(...(await block(token, options)));
  }
  return blocks;
}

function hintOf(paragraph: Tokens.Paragraph): { minutes: number; text: string } | null {
  const inner = paragraph.tokens ?? [];
  if (inner.length !== 1 || inner[0].type !== 'em') return null;
  return parseHint((inner[0] as Tokens.Em).text);
}

async function block(token: Token, options: BlockOptions): Promise<Content[]> {
  switch (token.type) {
    case 'paragraph':
      return [{ text: inlineRuns((token as Tokens.Paragraph).tokens ?? []), margin: [0, 0, 0, 7] }];
    case 'list':
      return [listBlock(token as Tokens.List, options)];
    case 'blockquote':
      return [await calloutBlock(token as Tokens.Blockquote, options)];
    case 'code':
      return [await codeBlock(token as Tokens.Code)];
    case 'table':
      return [tableBlock(token as Tokens.Table)];
    case 'hr':
      return [rule()];
    case 'html':
      return [{ text: (token as Tokens.HTML).text, font: 'Mono', fontSize: 8, color: COLORS.muted, margin: [0, 0, 0, 7] }];
    case 'space':
      return [];
    default:
      return 'text' in token ? [{ text: String((token as { text: string }).text), margin: [0, 0, 0, 7] }] : [];
  }
}

function listBlock(list: Tokens.List, options: BlockOptions): Content {
  const items = list.items.map((item) => {
    const parts: Content[] = [];
    for (const inner of item.tokens ?? []) {
      if (inner.type === 'text') {
        parts.push({ text: inlineRuns((inner as Tokens.Text).tokens ?? [{ type: 'text', raw: inner.raw, text: (inner as Tokens.Text).text } as Tokens.Text]) });
      } else if (inner.type === 'list') {
        parts.push(listBlock(inner as Tokens.List, options));
      } else if (inner.type === 'paragraph') {
        parts.push({ text: inlineRuns((inner as Tokens.Paragraph).tokens ?? []) });
      } else if (inner.type === 'code') {
        parts.push({ text: (inner as Tokens.Code).text, font: 'Mono', fontSize: 8.2, preserveLeadingSpaces: true, margin: [0, 3, 0, 3] });
      }
    }
    return parts.length === 1 ? parts[0] : { stack: parts };
  });
  const key = list.ordered ? 'ol' : 'ul';
  return {
    [key]: items,
    margin: [2, 0, 0, 8],
    ...(list.ordered && typeof list.start === 'number' && list.start > 1 ? { start: list.start } : {}),
    markerColor: COLORS.accent,
  };
}

async function calloutBlock(quote: Tokens.Blockquote, options: BlockOptions): Promise<Content> {
  const inner = quote.tokens ?? [];
  const first = inner[0];
  let label: string | null = null;
  let kind = 'evidence';
  let firstRuns: Inline[] | null = null;
  if (options.lesson && first?.type === 'paragraph') {
    const paragraphTokens = [...((first as Tokens.Paragraph).tokens ?? [])];
    const lead = paragraphTokens[0];
    if (lead?.type === 'strong') {
      const text = (lead as Tokens.Strong).text.replace(/:\s*$/, '').trim();
      const known = CALLOUTS[text.toLowerCase()];
      if (known) {
        label = text.toUpperCase();
        kind = known.kind;
        paragraphTokens.shift();
        // Drop the colon that followed the label.
        const after = paragraphTokens[0];
        if (after?.type === 'text') {
          (after as Tokens.Text).text = (after as Tokens.Text).text.replace(/^:\s*/, '');
          (after as Tokens.Text).raw = (after as Tokens.Text).text;
        }
        firstRuns = inlineRuns(paragraphTokens);
      }
    }
  }
  const color = CALLOUT_COLORS[kind] ?? COLORS.muted;
  const body: Content[] = [];
  if (label) {
    body.push({
      text: [
        { text: ` ${label} `, font: 'Mono', fontSize: 7, color, background: '#ffffff', characterSpacing: 0.6 },
        { text: '  ' },
        ...(firstRuns ?? []),
      ],
      fontSize: 9.5,
    });
    for (const token of inner.slice(1)) body.push(...(await block(token, options)));
  } else {
    for (const token of inner) body.push(...(await block(token, options)));
  }
  return {
    table: { widths: ['*'], body: [[{ stack: body, margin: [8, 6, 8, 4], fillColor: COLORS.panel }]] },
    layout: {
      hLineWidth: () => 0,
      vLineWidth: (i: number) => (i === 0 ? 2.5 : 0),
      vLineColor: () => color,
      paddingLeft: () => 4,
      paddingRight: () => 4,
      paddingTop: () => 0,
      paddingBottom: () => 0,
    },
    margin: [0, 2, 0, 9],
  };
}

async function codeBlock(code: Tokens.Code): Promise<Content> {
  const language = (code.lang ?? '').trim().split(/\s+/)[0].toLowerCase();
  if (language === 'mermaid') {
    const image = await renderDiagramImage(code.text);
    if (image) {
      const width = Math.min(TEXT_WIDTH, image.width * 0.75);
      return { image: image.dataUrl, width, alignment: 'center', margin: [0, 6, 0, 8] };
    }
    return {
      stack: [
        { text: 'DIAGRAM · could not be drawn for print; its source follows', font: 'Mono', fontSize: 7, color: COLORS.muted, margin: [0, 0, 0, 3] },
        { text: code.text, font: 'Mono', fontSize: 7.5, preserveLeadingSpaces: true, color: COLORS.muted },
      ],
      margin: [0, 4, 0, 8],
    };
  }
  return {
    table: {
      widths: ['*'],
      body: [
        [
          {
            stack: [
              ...(language ? [{ text: language.toUpperCase(), font: 'Mono', fontSize: 6.5, color: COLORS.faint, characterSpacing: 0.6, margin: [0, 0, 0, 4] }] : []),
              { text: code.text, font: 'Mono', fontSize: 8, lineHeight: 1.3, preserveLeadingSpaces: true },
            ],
            fillColor: COLORS.panel,
            margin: [8, 6, 8, 6],
          },
        ],
      ],
    },
    layout: {
      hLineWidth: () => 0.5,
      vLineWidth: () => 0.5,
      hLineColor: () => COLORS.rule,
      vLineColor: () => COLORS.rule,
      paddingLeft: () => 2,
      paddingRight: () => 2,
      paddingTop: () => 0,
      paddingBottom: () => 0,
    },
    margin: [0, 2, 0, 9],
  };
}

function tableBlock(table: Tokens.Table): Content {
  const header = table.header.map((cell) => ({ text: inlineRuns(cell.tokens ?? []), bold: true, fontSize: 8, color: COLORS.muted, fillColor: COLORS.panel }));
  const rows = table.rows.map((row) => row.map((cell) => ({ text: inlineRuns(cell.tokens ?? []), fontSize: 9 })));
  return {
    table: { headerRows: 1, widths: table.header.map(() => '*'), body: [header, ...rows] },
    layout: {
      hLineWidth: () => 0.5,
      vLineWidth: () => 0.5,
      hLineColor: () => COLORS.rule,
      vLineColor: () => COLORS.rule,
      paddingTop: () => 4,
      paddingBottom: () => 4,
    },
    margin: [0, 2, 0, 10],
  };
}

/** Inline Markdown tokens as pdfmake text runs. */
function inlineRuns(tokens: Token[], style: Record<string, unknown> = {}): Inline[] {
  const runs: Inline[] = [];
  for (const token of tokens) {
    switch (token.type) {
      case 'text': {
        const inner = (token as Tokens.Text).tokens;
        if (inner && inner.length) runs.push(...inlineRuns(inner, style));
        else runs.push({ text: unescape((token as Tokens.Text).text), ...style });
        break;
      }
      case 'escape':
        runs.push({ text: (token as Tokens.Escape).text, ...style });
        break;
      case 'strong':
        runs.push(...inlineRuns((token as Tokens.Strong).tokens ?? [], { ...style, bold: true }));
        break;
      case 'em':
        runs.push(...inlineRuns((token as Tokens.Em).tokens ?? [], { ...style, italics: true }));
        break;
      case 'del':
        runs.push(...inlineRuns((token as Tokens.Del).tokens ?? [], { ...style, decoration: 'lineThrough' }));
        break;
      case 'codespan':
        runs.push({ text: unescape((token as Tokens.Codespan).text), font: 'Mono', fontSize: 8.5, color: COLORS.violet, ...style });
        break;
      case 'link': {
        const link = token as Tokens.Link;
        runs.push(...inlineRuns(link.tokens ?? [], { ...style, color: COLORS.accent, decoration: 'underline', decorationColor: COLORS.rule, link: link.href }));
        break;
      }
      case 'image':
        runs.push({ text: (token as Tokens.Image).text || '[image]', italics: true, color: COLORS.muted, ...style });
        break;
      case 'br':
        runs.push({ text: '\n', ...style });
        break;
      case 'html':
        runs.push({ text: (token as Tokens.HTML).text, ...style });
        break;
      default:
        if ('text' in token) runs.push({ text: unescape(String((token as { text: string }).text)), ...style });
    }
  }
  return runs;
}

/** A line of Markdown (a prompt, a choice, a hint) as text runs. */
function inlineMarkdown(text: string, style: Record<string, unknown> = {}): Inline[] {
  const tokens = marked.lexer(text.trim());
  const first = tokens.find((token) => token.type === 'paragraph') as Tokens.Paragraph | undefined;
  return first?.tokens?.length ? inlineRuns(first.tokens, style) : [{ text: text.trim(), ...style }];
}

function unescape(text: string): string {
  return text
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

/** Display-font text, with symbols the display face lacks set in the body face. */
function displayRuns(text: string): Inline[] {
  const runs: Inline[] = [];
  let buffer = '';
  let symbol = false;
  const flush = () => {
    if (!buffer) return;
    runs.push(symbol ? { text: buffer, font: 'Inter' } : buffer);
    buffer = '';
  };
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    const isSymbol = code >= 0x2190 && char !== '≈';
    if (isSymbol !== symbol) {
      flush();
      symbol = isSymbol;
    }
    buffer += char;
  }
  flush();
  return runs;
}

// --- Language lessons -------------------------------------------------------

function languageBlocks(document: LessonDocument): Content[] {
  const language = document.language;
  if (!language) return [];
  const blocks: Content[] = [];
  if (language.phrases.length) {
    blocks.push(sectionHeading('Phrases'));
    blocks.push({
      table: {
        headerRows: 0,
        widths: ['*', '*', 'auto'],
        body: language.phrases.map((phrase) => [
          { text: phrase.target, bold: true, fontSize: 9.5 },
          { text: phrase.translation, fontSize: 9.5 },
          { text: phrase.note, fontSize: 8, color: COLORS.muted },
        ]),
      },
      layout: 'lightHorizontalLines',
      margin: [0, 2, 0, 10],
    });
  }
  if (language.dialogue.length) {
    blocks.push(sectionHeading('Dialogue'));
    for (const line of language.dialogue) {
      blocks.push({
        columns: [
          { text: line.speaker.toUpperCase(), font: 'Mono', fontSize: 7, color: COLORS.accent, width: 70, margin: [0, 2, 0, 0] },
          { stack: [{ text: line.target, fontSize: 10 }, { text: line.translation, fontSize: 8.5, color: COLORS.muted, italics: true }], width: '*' },
        ],
        margin: [0, 0, 0, 6],
      });
    }
  }
  for (const [label, text] of [
    ['Speaking prompt', language.speaking_prompt],
    ['Writing prompt', language.writing_prompt],
    ['Listening text', language.listen_text],
  ]) {
    if (!text.trim()) continue;
    blocks.push(sectionHeading(label));
    blocks.push({ text, margin: [0, 0, 0, 8] });
  }
  return blocks;
}

function sectionHeading(text: string): Content {
  return { stack: [{ text: displayRuns(text), font: 'Display', fontSize: 16, margin: [0, 18, 0, 4] }, rule()] };
}

// --- Practice ---------------------------------------------------------------

async function exerciseBlocks(document: LessonDocument): Promise<Content[]> {
  const exercise = document.exercise;
  if (!exercise) return [];
  const blocks: Content[] = [sectionHeading('Practice'), { text: exercise.title, bold: true, fontSize: 12, margin: [0, 0, 0, 6] }];
  if (exercise.deliverable) blocks.push(notice('DONE LOOKS LIKE', exercise.deliverable, COLORS.good));
  blocks.push(...(await markdownBlocks(exercise.instructions, { lesson: false })));
  if (exercise.starter_code) {
    blocks.push({ text: 'STARTER CODE', font: 'Mono', fontSize: 7, color: COLORS.muted, margin: [0, 6, 0, 3] });
    blocks.push(await codeBlock({ type: 'code', raw: '', text: exercise.starter_code, lang: '' } as Tokens.Code));
  }
  if (exercise.hints.length) {
    blocks.push({ text: 'HINTS', font: 'Mono', fontSize: 7, color: COLORS.muted, margin: [0, 6, 0, 3] });
    blocks.push({ ol: exercise.hints.map((hint) => ({ text: inlineMarkdown(hint), fontSize: 9 })), margin: [2, 0, 0, 8], markerColor: COLORS.accent });
  }
  if (exercise.draft?.trim()) {
    blocks.push({ text: 'YOUR WORK', font: 'Mono', fontSize: 7, color: COLORS.muted, margin: [0, 6, 0, 3] });
    blocks.push(await codeBlock({ type: 'code', raw: '', text: exercise.draft, lang: '' } as Tokens.Code));
  }
  if (exercise.reflection?.trim()) {
    blocks.push({ text: 'YOUR REFLECTION', font: 'Mono', fontSize: 7, color: COLORS.muted, margin: [0, 6, 0, 3] });
    blocks.push({ text: exercise.reflection.trim(), fontSize: 9.5, margin: [0, 0, 0, 8] });
  }
  if (exercise.completed) blocks.push({ text: 'Practice marked complete.', font: 'Mono', fontSize: 8, color: COLORS.good, margin: [0, 0, 0, 8] });
  return blocks;
}

// --- Check ------------------------------------------------------------------

function questionBlocks(document: LessonDocument): Content[] {
  const blocks: Content[] = [
    sectionHeading('Check'),
    {
      text: document.answer_key
        ? 'Correct answers and explanations are included: the check has been submitted.'
        : 'The answer key is withheld until the check is submitted on the desk.',
      fontSize: 8.5,
      italics: true,
      color: COLORS.muted,
      margin: [0, 0, 0, 10],
    },
  ];
  for (const question of document.questions) blocks.push(questionBlock(question));
  return blocks;
}

function questionBlock(question: QuestionDocument): Content {
  const letters = 'ABCDEFGH';
  const choices = question.choices.map((choice, index) => {
    const yours = question.your_answer != null && question.your_answer === choice;
    const correct = question.correct_answer != null && question.correct_answer === choice;
    const marks = [yours ? 'your answer' : '', correct ? 'correct' : ''].filter(Boolean).join(' · ');
    return {
      columns: [
        { text: letters[index] ?? '•', font: 'Mono', fontSize: 8, color: correct ? COLORS.good : yours ? COLORS.accent : COLORS.faint, width: 16, margin: [0, 1, 0, 0] },
        {
          text: [
            ...inlineMarkdown(choice, { bold: correct, color: correct ? COLORS.good : yours && question.result === 'incorrect' ? COLORS.bad : COLORS.ink }),
            ...(marks ? [{ text: `   ${marks}`, font: 'Mono', fontSize: 7, color: correct ? COLORS.good : COLORS.accent }] : []),
          ],
          fontSize: 9.5,
          width: '*',
        },
      ],
      margin: [0, 0, 0, 3],
    };
  });
  const stack: Content[] = [
    {
      columns: [
        { text: String(question.position).padStart(2, '0'), font: 'Mono', fontSize: 8, color: COLORS.accent, width: 22, margin: [0, 2, 0, 0] },
        { stack: [{ text: inlineMarkdown(question.prompt, { bold: true }), fontSize: 10.5 }, ...(question.section || question.objective ? [{ text: [question.section, question.objective].filter(Boolean).join('  ·  '), font: 'Mono', fontSize: 7, color: COLORS.faint, margin: [0, 2, 0, 0] }] : [])], width: '*' },
      ],
      margin: [0, 0, 0, 6],
    },
    { stack: choices, margin: [22, 0, 0, 4] },
  ];
  if (question.explanation) {
    stack.push({ text: [{ text: 'Why  ', font: 'Mono', fontSize: 7, color: COLORS.muted }, ...inlineMarkdown(question.explanation, { fontSize: 9, color: COLORS.muted })], margin: [22, 2, 0, 0] });
  }
  return { stack, unbreakable: true, margin: [0, 0, 0, 14] };
}

// --- Reading list -----------------------------------------------------------

function resourceBlocks(document: LessonDocument): Content[] {
  return [
    sectionHeading('Reading list'),
    {
      ul: document.resources.map((resource) => ({
        stack: [
          { text: resource.title || resource.url, bold: true, fontSize: 9.5 },
          { text: resource.url, font: 'Mono', fontSize: 7.5, color: COLORS.accent, link: resource.url, decoration: 'underline', decorationColor: COLORS.rule },
          ...(resource.why ? [{ text: resource.why, fontSize: 8.5, color: COLORS.muted }] : []),
        ],
        margin: [0, 0, 0, 6],
      })),
      margin: [2, 0, 0, 8],
      markerColor: COLORS.accent,
    },
  ];
}

/** The section identities, so a caller can list what the PDF will contain. */
export const SECTION_TITLES = LESSON_SECTIONS.map((section) => section.title);
