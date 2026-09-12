import banks from '../../src-tauri/seed/diagnostics.json';
import concepts from '../../src-tauri/seed/concepts.json';
import { courseDefinition } from './catalog';
import type { ClassroomSubjectId } from './catalog.generated';
import { previewClassPath, revisePreviewClassPath } from './class-preview';
import type { AssessmentResponse, AssessmentRoundId } from './contracts/assessments';
import type { UnitChallengeView } from './contracts/challenges';
import type { AssessmentAttemptId, CriterionResult, PathTopic } from './contracts/placement';

type Bank = typeof banks.courses[number];
type Question = Bank['questions'][number];
interface Attempt {
  id: AssessmentAttemptId; course_id: string; unit: string; path_revision: number; round_id: AssessmentRoundId;
  questions: Question[]; revision: number; responses: Record<string, AssessmentResponse>; result: CriterionResult[] | null;
  applied_revision: number | null;
}
const memory = new Map<string, Attempt[]>();
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
function history(courseId: string): Attempt[] {
  const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(`principia:preview:challenge:${courseId}`) : null;
  const values = raw ? JSON.parse(raw) : memory.get(courseId) ?? [];
  if (!Array.isArray(values)) throw new Error('The saved preview challenge is unreadable and has been retained.');
  return structuredClone(values);
}
function save(courseId: string, values: Attempt[]) {
  if (typeof localStorage !== 'undefined') localStorage.setItem(`principia:preview:challenge:${courseId}`, JSON.stringify(values));
  memory.set(courseId, structuredClone(values));
}
const title = (slug: string) => concepts.find((c) => c.slug === slug)?.title ?? slug;
function topics(attempt: Attempt, verdict: (v: CriterionResult['verdict']) => boolean): PathTopic[] {
  const seen = new Set<string>();
  return (attempt.result ?? []).filter((row) => verdict(row.verdict) && !seen.has(row.competency) && seen.add(row.competency))
    .map((row) => ({ id: row.competency, label: title(row.competency), reason: row.verdict === 'passed' ? 'Sample demonstrated in this unit challenge.' : row.verdict === 'unknown' ? 'Skipped in this unit challenge.' : 'Sample needs practice.' }));
}
function view(attempt: Attempt): UnitChallengeView {
  return {
    attempt_id: attempt.id, round_id: attempt.round_id, revision: attempt.revision, course_id: attempt.course_id, unit: attempt.unit,
    unit_label: courseDefinition(attempt.course_id as ClassroomSubjectId)?.entry_points.find((e) => e.id === attempt.unit)?.label ?? attempt.unit,
    path_revision: attempt.path_revision,
    questions: attempt.questions.map(({ id, label, prompt, choices }) => ({ id, label, prompt, choices })),
    responses: Object.fromEntries(attempt.questions.map((q) => [q.id, attempt.responses[q.id] ?? { answer: '', status: 'draft' }])),
    criteria: attempt.result ?? [], submitted: attempt.result !== null,
    demonstrated: topics(attempt, (v) => v === 'passed'), needs_practice: topics(attempt, (v) => v !== 'passed'),
    applied_revision: attempt.applied_revision, estimated_minutes: attempt.questions.length + 1,
  };
}
export async function getPreviewChallenge(courseId: ClassroomSubjectId) { const attempt = history(courseId).at(-1); return attempt ? view(attempt) : null; }
export async function startPreviewChallenge(courseId: ClassroomSubjectId, unit: string, restart = false) {
  const path = previewClassPath(courseId);
  if (!path) throw new Error('Accept a learning path before taking a unit challenge.');
  const values = history(courseId); const existing = values.at(-1);
  if (existing && !existing.result) { if (existing.unit !== unit) throw new Error(`Finish or discard the open ${existing.unit} challenge first.`); return view(existing); }
  if (existing && existing.unit === unit && !restart && existing.applied_revision === null) return view(existing);
  const bank = banks.courses.find((b) => b.course_id === courseId);
  const questions = bank?.questions.filter((q) => !q.followup_for && q.entry_point === unit) ?? [];
  if (!questions.length) throw new Error('No challenge questions are authored for this unit yet.');
  const attempt: Attempt = { id: `preview-challenge-${crypto.randomUUID()}` as AssessmentAttemptId, course_id: courseId, unit, path_revision: path.revision, round_id: `preview-round-${crypto.randomUUID()}` as AssessmentRoundId, questions: structuredClone(questions), revision: 0, responses: {}, result: null, applied_revision: null };
  values.push(attempt); save(courseId, values); return view(attempt);
}
export async function savePreviewChallengeResponse(courseId: ClassroomSubjectId, roundId: AssessmentRoundId, revision: number, questionId: string, response: AssessmentResponse) {
  const values = history(courseId); const attempt = values.at(-1);
  const question = attempt?.questions.find((q) => q.id === questionId);
  if (!attempt || attempt.round_id !== roundId || !question || attempt.result) throw new Error('The challenge round changed or is already submitted.');
  if ((response.status === 'answered' && !response.answer) || (response.answer && !question.choices.some((c) => c.id === response.answer)) || (response.status === 'skipped' && response.answer)) throw new Error('Select an available answer or explicitly skip.');
  if (same(attempt.responses[questionId], response)) return attempt.revision;
  if (attempt.revision !== revision) throw new Error('Answers changed; reload the saved revision.');
  attempt.responses[questionId] = structuredClone(response); attempt.revision += 1; save(courseId, values); return attempt.revision;
}
export async function submitPreviewChallenge(courseId: ClassroomSubjectId, roundId: AssessmentRoundId, revision: number) {
  const values = history(courseId); const attempt = values.at(-1);
  if (!attempt || attempt.round_id !== roundId) throw new Error('Challenge round changed.');
  if (attempt.result) return view(attempt);
  if (attempt.revision !== revision) throw new Error('Answers changed; reload the saved revision.');
  attempt.result = attempt.questions.map((q) => {
    const response = attempt.responses[q.id];
    if (!response || response.status === 'draft') throw new Error('Confirm or skip every question.');
    const verdict = response.status === 'skipped' ? 'unknown' : response.answer === q.answer ? 'passed' : 'needs_practice';
    return { id: q.criterion, competency: q.competency, entry_point: q.entry_point, label: q.label, verdict, evidence: [{ question_id: q.id, response, verdict, explanation: q.explanation, expected_answer: q.choices.find((c) => c.id === q.answer)!.text }] };
  });
  save(courseId, values); return view(attempt);
}
export async function applyPreviewChallenge(courseId: ClassroomSubjectId, attemptId: string, expectedRevision: number) {
  const values = history(courseId); const attempt = values.find((a) => a.id === attemptId);
  if (!attempt || !attempt.result) throw new Error('Submit the challenge before checking out of its topics.');
  if (attempt.applied_revision !== null) { const path = previewClassPath(courseId); if (path && path.revision >= attempt.applied_revision) return path; }
  const demonstrated = topics(attempt, (v) => v === 'passed').map((t) => t.id);
  if (!demonstrated.length) throw new Error('No sample was demonstrated; the unit stays on your route.');
  const path = revisePreviewClassPath({ course_id: courseId, expected_revision: expectedRevision, change: { kind: 'check_out', topics: demonstrated, attempt_id: attemptId } }, title);
  attempt.applied_revision = path.revision; save(courseId, values); return path;
}
