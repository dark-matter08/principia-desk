/**
 * The interface's mirror of the desk's writing check (`src-tauri/src/prose.rs`):
 * the same catalogue of tells, read the same way, so the class editor can say
 * what the desk would object to as the learner types. The native result stays
 * authoritative; this one runs between saves.
 */
import { PROSE_SOFT_LIMIT, PROSE_TELLS } from './prose.generated';

export const OBJECTION = 'reads like a machine wrote it: ';

export interface ProseHit {
  name: string;
  count: number;
  hard: boolean;
}

/** Prose and code, so fenced blocks and inline code are left alone. */
function parts(text: string): { code: boolean; text: string }[] {
  const out: { code: boolean; text: string }[] = [];
  let rest = text;
  while (rest) {
    const fence = rest.indexOf('```');
    let tick = rest.indexOf('`');
    if (tick === fence) tick = -1;
    if (fence >= 0 && (tick < 0 || fence < tick)) {
      out.push({ code: false, text: rest.slice(0, fence) });
      const end = rest.indexOf('```', fence + 3);
      if (end < 0) { out.push({ code: false, text: rest.slice(fence, fence + 3) }); rest = rest.slice(fence + 3); }
      else { out.push({ code: true, text: rest.slice(fence, end + 3) }); rest = rest.slice(end + 3); }
    } else if (tick >= 0) {
      out.push({ code: false, text: rest.slice(0, tick) });
      const after = rest.slice(tick + 1);
      const lineEnd = after.indexOf('\n');
      const line = lineEnd < 0 ? after : after.slice(0, lineEnd);
      const close = line.indexOf('`');
      if (close >= 0) { const end = tick + 1 + close + 1; out.push({ code: true, text: rest.slice(tick, end) }); rest = rest.slice(end); }
      else { out.push({ code: false, text: rest.slice(tick, tick + 1) }); rest = rest.slice(tick + 1); }
    } else {
      out.push({ code: false, text: rest });
      rest = '';
    }
  }
  return out;
}

const isWord = (c: string) => /[\p{L}\p{N}']/u.test(c);

function opensSentence(lower: string, at: number): boolean {
  for (let i = at - 1; i >= 0; i--) {
    const c = lower[i];
    if (/\s/.test(c) || '-*>#"(“‘\''.includes(c)) continue;
    return '.!?:;\n'.includes(c);
  }
  return true;
}

function count(lower: string, pattern: string): number {
  const sentenceStart = pattern.startsWith('^');
  const needle = sentenceStart ? pattern.slice(1) : pattern;
  let found = 0;
  let from = 0;
  while (from <= lower.length - needle.length) {
    const at = lower.indexOf(needle, from);
    if (at < 0) break;
    const beforeOk = at === 0 || !isWord(lower[at - 1]);
    const after = lower[at + needle.length];
    const afterOk = after === undefined || !isWord(after);
    if (beforeOk && afterOk && (!sentenceStart || opensSentence(lower, at))) { found++; from = at + needle.length; }
    else from = at + 1;
  }
  return found;
}

const EMOJI = /[\u{1F000}-\u{1FAFF}\u{2600}-\u{27BF}\u{2B50}\u{2B55}\u{231A}\u{231B}\u{23E9}-\u{23FA}]/u;

/** Every tell a text carries outside code, hard ones first. */
export function proseHits(text: string): ProseHit[] {
  const prose = parts(text).filter((p) => !p.code).map((p) => p.text).join('\n');
  const lower = prose.toLowerCase();
  const found: ProseHit[] = [];
  for (const tell of PROSE_TELLS) {
    const n = count(lower, tell.pattern);
    if (n > 0) found.push({ name: tell.pattern.replace(/^\^/, ''), count: n, hard: tell.hard });
  }
  if (EMOJI.test(prose)) found.push({ name: 'emoji', count: 1, hard: true });
  return found.sort((a, b) => Number(b.hard) - Number(a.hard) || b.count - a.count);
}

/** What the desk would report for the text, or null when it reads as written by a person. */
export function proseReport(text: string): string | null {
  const found = proseHits(text);
  const hard = found.filter((h) => h.hard);
  const soft = found.filter((h) => !h.hard);
  const softTotal = soft.reduce((n, h) => n + h.count, 0);
  if (!hard.length && softTotal <= PROSE_SOFT_LIMIT) return null;
  const list = (hits: ProseHit[]) => hits.map((h) => (h.count > 1 ? `"${h.name}" (${h.count}x)` : `"${h.name}"`)).join(', ');
  let reason = hard.length ? list(hard) : '';
  if (softTotal > PROSE_SOFT_LIMIT) reason += `${hard.length ? '; and ' : ''}${softTotal} stock words where at most ${PROSE_SOFT_LIMIT} pass: ${list(soft)}`;
  return reason;
}

/** How many em dashes a text carries outside code; the desk removes them on save. */
export function emDashes(text: string): number {
  return parts(text).filter((p) => !p.code).reduce((n, p) => n + (p.text.match(/—/g)?.length ?? 0), 0);
}
