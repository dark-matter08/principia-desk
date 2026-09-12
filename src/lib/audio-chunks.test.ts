import { describe, expect, it } from 'vitest';
import { chunkText } from './audio-chunks';

describe('narration chunks', () => {
  it('keeps short sentences before a long sentence and its following sentence', () => {
    const text = `First. ${'a longer explanation '.repeat(20).trim()}. Last.`;
    const chunks = chunkText(text, 90);
    expect(chunks.join(' ')).toBe(text);
    expect(chunks.every((chunk) => chunk.length <= 90)).toBe(true);
  });

  it('preserves sentence order across consecutive oversized sentences', () => {
    const text = `Intro. ${'middle '.repeat(30).trim()}. ${'ending '.repeat(30).trim()}! Done.`;
    expect(chunkText(text, 70).join(' ')).toBe(text);
  });
});
