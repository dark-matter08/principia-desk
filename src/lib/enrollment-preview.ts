import { courseDefinition } from './catalog';
import { COURSE_FINGERPRINTS } from './catalog.generated';
import type { ClassroomSubjectId, LanguageStrand } from './ipc';
import type { EnrollmentDraft, EnrollmentDraftId, EnrollmentOptions, SaveEnrollmentDraft } from './contracts/enrollment';
import concepts from '../../src-tauri/seed/concepts.json';
import { COURSES } from './catalog';
import { previewClassRecords } from './class-preview-store';

const languageStrands: LanguageStrand[] = ['listening', 'reading', 'spoken_interaction', 'spoken_production', 'writing', 'grammar', 'vocabulary_pragmatics'];
const drafts = new Map<ClassroomSubjectId, EnrollmentDraft>();
const key = (courseId: string) => `principia:preview:enrollment:${courseId}`;

export function previewEnrollmentOptions(courseId: ClassroomSubjectId): EnrollmentOptions {
  const course = courseDefinition(courseId);
  if (!course) throw new Error('Unknown course');
  return {
    course: { course_id: courseId, version: course.version, fingerprint: (COURSE_FINGERPRINTS as Record<string, string>)[courseId] ?? `preview-${courseId}-${course.version}` },
    entry_points: course.entry_points.map((point) => ({ ...point })),
    diagnostic_available: !courseId.startsWith('custom-'),
    familiarity_options: course.kind === 'engineering'
      ? concepts.filter((concept) => concept.focus === courseId).map((concept) => ({ id: concept.slug, label: concept.title, group: concept.curriculum.phase }))
      : languageStrands.map((strand) => ({ id: strand, label: strand.replaceAll('_', ' '), group: null })),
    default_configuration: {
      goal: course.kind === 'engineering' ? { kind: 'course_outcome', note: '' } : { kind: 'language_level', target_level: 'A2', note: '' },
      entry: { route: 'foundations' },
      pace: { session_minutes: 30, weekly_minutes: null },
      tutor: { provider: 'claude', model: 'sonnet', custom_agent_bin: null },
      focus_policy: 'advisory',
    },
  };
}

export function previewEnrollmentDraft(courseId: ClassroomSubjectId): EnrollmentDraft | null {
  if (!courseDefinition(courseId)) throw new Error('Unknown course');
  if (typeof localStorage !== 'undefined') {
    const stored = localStorage.getItem(key(courseId));
    if (stored) {
      const draft = JSON.parse(stored) as EnrollmentDraft;
      return draft.status === 'draft' && !previewClassRecords().some(r => r.draft.id === draft.id) ? draft : null;
    }
  }
  const draft = drafts.get(courseId);
  return draft && !previewClassRecords().some(r => r.draft.id === draft.id) ? structuredClone(draft) : null;
}

export function findPreviewEnrollmentDraft(id: EnrollmentDraftId): EnrollmentDraft | null {
  return previewClassRecords().find(r => r.draft.id === id)?.draft ?? COURSES.map(c => previewEnrollmentDraft(c.id)).find(d => d?.id === id) ?? null;
}

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value !== null && typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, value]) => [key, canonical(value)]));
  return value;
}
function same(left: unknown, right: unknown) { return JSON.stringify(canonical(left)) === JSON.stringify(canonical(right)); }
const byteLength = (value: string) => new TextEncoder().encode(value).length;

export function savePreviewEnrollmentDraft(input: SaveEnrollmentDraft): EnrollmentDraft {
  const options = previewEnrollmentOptions(input.course.course_id);
  if (!same(options.course, input.course)) throw new Error('Curriculum changed; reload course options before saving.');
  const config = input.configuration;
  if (byteLength(config.goal.note) > 4000 || config.goal.kind !== options.default_configuration.goal.kind) throw new Error('Goal does not match this course.');
  if (config.entry.route === 'manual') {
    const entry = config.entry;
    if (!options.entry_points.some((point) => point.id === entry.entry_point)) throw new Error('Unknown starting point.');
    if (new Set(entry.familiar_competencies).size !== entry.familiar_competencies.length || entry.familiar_competencies.some((id) => !options.familiarity_options.some((option) => option.id === id))) throw new Error('Familiarity must reference distinct competencies in this course.');
  }
  if (config.goal.kind === 'language_level') {
    const goal = config.goal;
    const entry = config.entry;
    const target = options.entry_points.findIndex((point) => point.id === goal.target_level);
    const start = entry.route === 'manual' ? options.entry_points.findIndex((point) => point.id === entry.entry_point) : 0;
    if (target < start) throw new Error('Target level must include the chosen starting level.');
  }
  const { session_minutes: minutes, weekly_minutes: weekly } = config.pace;
  if (!Number.isInteger(minutes) || minutes < 10 || minutes > 120 || (weekly !== null && (!Number.isInteger(weekly) || weekly < minutes || weekly > 10080))) throw new Error('Choose a valid study pace.');
  if (!['claude', 'codex', 'cursor', 'gemini', 'deepseek', 'custom', 'anthropic', 'openai', 'google', 'openrouter', 'groq', 'mistral', 'ollama'].includes(config.tutor.provider) || !config.tutor.model || byteLength(config.tutor.model) > 160 || /[\s\x00-\x1f\x7f]/u.test(config.tutor.model) || (config.tutor.provider === 'custom' && !config.tutor.custom_agent_bin?.trim())) throw new Error('Choose a provider and its model.');
  if (config.tutor.custom_agent_bin !== null && (byteLength(config.tutor.custom_agent_bin) > 4096 || config.tutor.custom_agent_bin.includes('\0'))) throw new Error('Invalid custom executable path.');
  const existing = previewEnrollmentDraft(input.course.course_id);
  if (input.id && (!existing || input.id !== existing.id)) throw new Error('Enrollment draft not found.');
  if (existing) {
    if (existing.status !== 'draft') throw new Error('This draft can no longer be changed.');
    if (same(existing.course, input.course) && same(existing.configuration, config)) return existing;
    if (input.id !== existing.id || input.expected_revision !== existing.revision) throw new Error('Enrollment draft changed; reload before saving.');
  } else if (input.expected_revision !== null) throw new Error('A new draft has no prior revision.');
  const now = new Date().toISOString();
  const saved: EnrollmentDraft = {
    id: existing?.id ?? `draft-${crypto.randomUUID().replaceAll('-', '')}` as EnrollmentDraftId,
    course: structuredClone(input.course), configuration: structuredClone(config), status: 'draft',
    revision: (existing?.revision ?? 0) + 1, accepted_class_id: null,
    created_at: existing?.created_at ?? now, updated_at: now,
  };
  if (typeof localStorage !== 'undefined') localStorage.setItem(key(input.course.course_id), JSON.stringify(saved));
  drafts.set(input.course.course_id, structuredClone(saved));
  return saved;
}
