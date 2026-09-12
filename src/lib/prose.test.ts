import { describe, expect, it } from 'vitest';
import { emDashes, proseHits, proseReport } from './prose';

describe('the writing check mirror', () => {
  it('fails a hard tell on one occurrence and soft ones on a pile', () => {
    expect(proseReport('The buffer is flushed when it fills.')).toBeNull();
    expect(proseReport("Let's delve into how the buffer works.")).toContain('"delve"');
    expect(proseReport('This is not only fast but also robust.')).toContain('"not only"');
    expect(proseReport('A robust, comprehensive and crucial design.')).toBeNull();
    expect(proseReport('A robust, comprehensive, crucial and pivotal design.')).toContain('4 stock words');
  });
  it('reads on word boundaries, sentence starts and outside code', () => {
    expect(proseReport('The crucially named table and the pivotally placed row.')).toBeNull();
    expect(proseHits('Overall latency fell.').some((h) => h.name === 'overall')).toBe(true);
    expect(proseHits('The overall latency fell.').some((h) => h.name === 'overall')).toBe(false);
    expect(proseHits('Done. In conclusion, it works.').some((h) => h.name === 'in conclusion' && h.hard)).toBe(true);
    expect(proseHits('`delve`\n```\ndelve\n```\n')).toEqual([]);
    expect(proseHits('a stray ` tick then delve').some((h) => h.name === 'delve')).toBe(true);
    expect(proseHits('Great work 🚀').some((h) => h.name === 'emoji')).toBe(true);
    expect(emDashes('a — b\n```\nc — d\n```')).toBe(1);
  });
});
