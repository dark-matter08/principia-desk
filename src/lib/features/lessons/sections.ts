/**
 * The reader's knowledge of a lesson's shape.
 *
 * A generated lesson has ten fixed sections; each opens with a reading hint
 * (`*~2 min · what to look for*`) and marks its important points with
 * labelled callouts (`> **Key idea:** …`). The tutor writes those as plain
 * Markdown so the gate can check them; the reader turns them into headings
 * with icons and minutes, hint rows, callout cards and a section map. Older
 * lessons without hints or callouts still render, only plainer.
 */
import { iconSvg } from './icons';

export interface SectionIdentity {
  title: string;
  icon: string;
  /** What the section is for, shown in the section map. */
  purpose: string;
}

export const LESSON_SECTIONS: SectionIdentity[] = [
  { title: 'Why this matters', icon: 'target', purpose: 'the stakes' },
  { title: 'The simple version', icon: 'lightbulb', purpose: 'an analogy and its limit' },
  { title: 'Core mechanics', icon: 'cog', purpose: 'the mechanism, derived' },
  { title: 'Mental model', icon: 'brain', purpose: 'a model you can check' },
  { title: 'Runnable experiment', icon: 'flask-conical', purpose: 'see it for yourself' },
  { title: 'Production architecture lens', icon: 'server', purpose: 'where it lives in a real system' },
  { title: 'Trade-offs and failure modes', icon: 'scale', purpose: 'decisions and what breaks' },
  { title: 'Migration and observability', icon: 'route', purpose: 'change it safely, watch it' },
  { title: 'Practical exercise', icon: 'hammer', purpose: 'what you will build' },
  { title: 'Key takeaways', icon: 'bookmark-check', purpose: 'what to remember' },
];

/**
 * How the fixed sections read for someone starting from scratch. The
 * Markdown keeps the canonical headings so the gate and the exports agree;
 * the reader and the PDF show these instead when a lesson is beginner-level.
 */
export const BEGINNER_TITLES: Record<string, string> = {
  'Why this matters': 'Why this matters',
  'The simple version': 'The simple version',
  'Core mechanics': 'How it works',
  'Mental model': 'The picture to keep in your head',
  'Runnable experiment': 'Try it yourself',
  'Production architecture lens': 'Where you will meet this in real work',
  'Trade-offs and failure modes': 'Common mistakes and how to spot them',
  'Migration and observability': 'Checking what happened, and undoing it',
  'Practical exercise': 'What you will build',
  'Key takeaways': 'Remember',
};

export type LessonLevel = 'beginner' | 'standard';

/** The heading to show for a canonical section title at a level. */
export function displayTitle(title: string, level: LessonLevel): string {
  if (level !== 'beginner') return title;
  const canonical = identify(title)?.title;
  return (canonical && BEGINNER_TITLES[canonical]) || title;
}

export const CALLOUTS: Record<string, { icon: string; kind: string }> = {
  'key idea': { icon: 'key-round', kind: 'key' },
  'watch out': { icon: 'triangle-alert', kind: 'warn' },
  'try it': { icon: 'play', kind: 'try' },
  example: { icon: 'quote', kind: 'example' },
  decision: { icon: 'git-fork', kind: 'decision' },
  evidence: { icon: 'book-open', kind: 'evidence' },
};

export interface SectionHint {
  minutes: number;
  text: string;
}

export interface LessonSection {
  /** Index within the lesson's headings, 0-based. */
  index: number;
  id: string;
  /** The canonical heading, as written in the Markdown. */
  title: string;
  /** The heading as shown for the lesson's level. */
  display: string;
  icon: string;
  purpose: string;
  hint: SectionHint | null;
  element: HTMLElement;
}

/** Parse `~2 min · what to look for`, in its common spellings. */
export function parseHint(text: string): SectionHint | null {
  const match = text
    .trim()
    .match(/^[~≈]?\s*(\d+)\s*(?:minutes|mins|min)\b\s*[·\-–—:•|]?\s*(.+)$/i);
  if (!match) return null;
  return { minutes: Number(match[1]), text: match[2].trim() };
}

export function identify(title: string): SectionIdentity | null {
  const wanted = title.trim().toLowerCase();
  return LESSON_SECTIONS.find((section) => section.title.toLowerCase() === wanted) ?? null;
}

function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

/**
 * Decorate rendered lesson HTML in place and describe its sections. Runs
 * again whenever the Markdown changes; the host replaces the HTML each time,
 * so nothing here needs to be undone.
 */
export function decorateLesson(root: HTMLElement, level: LessonLevel = 'standard'): LessonSection[] {
  const sections: LessonSection[] = [];
  const headings = Array.from(root.querySelectorAll<HTMLHeadingElement>('h2'));
  headings.forEach((heading, index) => {
    const title = heading.textContent?.trim() ?? '';
    const identity = identify(title);
    const display = displayTitle(title, level);
    const id = `lesson-section-${slug(title) || index}`;
    heading.id = id;
    heading.classList.add('lesson-heading');
    if (identity) {
      heading.classList.add('lesson-heading-known');
      heading.innerHTML = `<span class="lesson-heading-mark">${iconSvg(identity.icon, 18)}<span class="lesson-heading-index mono">${String(index + 1).padStart(2, '0')}</span></span><span class="lesson-heading-title">${escape(display)}</span>`;
    }

    // The hint: an italic-only paragraph right under the heading.
    let hint: SectionHint | null = null;
    const next = heading.nextElementSibling;
    if (next instanceof HTMLParagraphElement && next.children.length === 1 && next.firstElementChild?.tagName === 'EM' && next.textContent?.trim() === next.firstElementChild.textContent?.trim()) {
      const parsed = parseHint(next.firstElementChild.textContent ?? '');
      if (parsed) {
        hint = parsed;
        const row = document.createElement('div');
        row.className = 'lesson-hint';
        row.setAttribute('role', 'note');
        row.innerHTML = `<span class="lesson-hint-time mono">${iconSvg('clock', 12)} ~${parsed.minutes} min</span><span class="lesson-hint-text">${escape(parsed.text)}</span>`;
        next.replaceWith(row);
      }
    }
    if (hint) heading.dataset.minutes = String(hint.minutes);

    sections.push({
      index,
      id,
      title,
      display,
      icon: identity?.icon ?? 'book-open',
      purpose: identity?.purpose ?? '',
      hint,
      element: heading,
    });
  });

  for (const quote of Array.from(root.querySelectorAll<HTMLQuoteElement>('blockquote'))) {
    const strong = quote.querySelector('p:first-child > strong:first-child');
    const label = strong?.textContent?.replace(/:\s*$/, '').trim().toLowerCase() ?? '';
    const callout = CALLOUTS[label];
    if (!callout || !strong) continue;
    quote.classList.add('lesson-callout', `lesson-callout-${callout.kind}`);
    strong.classList.add('lesson-callout-label');
    strong.innerHTML = `${iconSvg(callout.icon, 13)}<span>${escape(strong.textContent?.replace(/:\s*$/, '') ?? '')}</span>`;
    // The colon after the label reads as noise once the label is a chip.
    const after = strong.nextSibling;
    if (after?.nodeType === Node.TEXT_NODE) after.textContent = (after.textContent ?? '').replace(/^:\s*/, ' ');
  }

  // Captions: a short paragraph right after a diagram.
  for (const block of Array.from(root.querySelectorAll<HTMLElement>('.mermaid-block'))) {
    const caption = block.nextElementSibling;
    if (caption instanceof HTMLParagraphElement && (caption.textContent?.length ?? 0) <= 220) {
      caption.classList.add('lesson-caption');
    }
  }
  return sections;
}

function escape(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** The minutes the hints add up to, for the map's headline. */
export function readingMinutes(sections: LessonSection[]): number {
  return sections.reduce((sum, section) => sum + (section.hint?.minutes ?? 0), 0);
}
