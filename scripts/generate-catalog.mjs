import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const manifest = JSON.parse(await readFile(new URL('../src-tauri/seed/catalog.json', import.meta.url), 'utf8'));
const union = (kind) => manifest.courses.filter((course) => course.kind === kind).map((course) => JSON.stringify(course.id)).join(' | ');
// A learner's own class carries the custom prefix; its id is only known at runtime.
let generated = `// Generated from src-tauri/seed/catalog.json. Run npm run catalog:generate.\nexport type FocusArea = ${union('engineering')};\nexport type LanguageId = ${union('language')};\nexport type CustomSubjectId = \`custom-\${string}\`;\nexport type ClassroomSubjectId = FocusArea | LanguageId | CustomSubjectId;\n`;
// Hash the exact snapshot native enrollment stores. Object keys are sorted;
// arrays retain their authored order. This is also checked by Rust tests.
const canonical = (value) => Array.isArray(value)
  ? value.map(canonical)
  : value !== null && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]))
    : value;
const concepts = JSON.parse(await readFile(new URL('../src-tauri/seed/concepts.json', import.meta.url), 'utf8'));
const fingerprints = {};
for (const course of manifest.courses) {
  if (!/^[a-z0-9-]+$/.test(course.id) || course.prompt_path !== `prompts/classroom/${course.id}.txt`) throw new Error('Invalid catalog path');
  const references = [];
  for (const slug of course.reference_lessons) {
    if (!/^[a-z0-9-]+$/.test(slug)) throw new Error('Invalid reference slug');
    references.push(JSON.parse(await readFile(new URL(`../src-tauri/seed/fallback_courses/${slug}.json`, import.meta.url), 'utf8')));
  }
  const curriculum = course.kind === 'engineering'
    ? concepts.filter((concept) => concept.focus === course.id)
    : JSON.parse(await readFile(new URL(`../src-tauri/seed/languages/${course.id}.json`, import.meta.url), 'utf8'));
  const prompt = await readFile(new URL(`../src-tauri/${course.prompt_path}`, import.meta.url), 'utf8');
  fingerprints[course.course_id] = createHash('sha256').update(JSON.stringify(canonical({ course, curriculum, prompt, reference_lessons: references }))).digest('hex');
}
generated += `export const COURSE_FINGERPRINTS: Record<FocusArea | LanguageId, string> = ${JSON.stringify(fingerprints, null, 2)};\n`;
// The writing check's catalogue, so the editor's mirror reads for the same tells the desk does.
const prose = JSON.parse(await readFile(new URL('../src-tauri/seed/prose-tells.json', import.meta.url), 'utf8'));
const proseGenerated = `// Generated from src-tauri/seed/prose-tells.json. Run npm run catalog:generate.\nexport const PROSE_SOFT_LIMIT = ${prose.soft_limit};\nexport const PROSE_TELLS: readonly { pattern: string; hard: boolean }[] = ${JSON.stringify(prose.tells)};\n`;
const outputs = [
  [new URL('../src/lib/catalog.generated.ts', import.meta.url), generated, 'Catalog types or curriculum fingerprints'],
  [new URL('../src/lib/prose.generated.ts', import.meta.url), proseGenerated, 'The writing check catalogue'],
];
for (const [destination, content, what] of outputs) {
  if (process.argv.includes('--check')) {
    if (await readFile(destination, 'utf8').catch(() => '') !== content) {
      throw new Error(`${what} are stale: run npm run catalog:generate (${fileURLToPath(destination)})`);
    }
  } else {
    await writeFile(destination, content);
  }
}
