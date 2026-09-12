/** Split into engine-safe pieces on sentence boundaries (falling back to
 * word boundaries for run-on sentences) so no single utterance is long
 * enough to trigger the long-utterance stall bug. */
export function chunkText(text: string, maxLen = 180): string[] {
  const sentences = text.match(/[^.!?]+[.!?]*\s*/g) ?? [text];
  const chunks: string[] = [];
  let current = '';
  for (const raw of sentences) {
    const sentence = raw.trim();
    if (!sentence) continue;
    if (sentence.length > maxLen) {
      if (current) {
        chunks.push(current);
        current = '';
      }
      let piece = '';
      for (const word of sentence.split(' ')) {
        const next = piece ? `${piece} ${word}` : word;
        if (next.length > maxLen && piece) {
          chunks.push(piece);
          piece = word;
        } else {
          piece = next;
        }
      }
      if (piece) chunks.push(piece);
      continue;
    }
    const next = current ? `${current} ${sentence}` : sentence;
    if (next.length > maxLen && current) {
      chunks.push(current);
      current = sentence;
    } else {
      current = next;
    }
  }
  if (current) chunks.push(current);
  return chunks.length > 0 ? chunks : [text];
}
