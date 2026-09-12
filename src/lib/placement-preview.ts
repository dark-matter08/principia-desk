/** Browser preview of the native placement contract. Definitions are bundled;
 * this storage is separate from the native learner database. */
import banks from '../../src-tauri/seed/diagnostics.json';
import concepts from '../../src-tauri/seed/concepts.json';
import { courseDefinition } from './catalog';
import { findPreviewEnrollmentDraft, previewEnrollmentOptions } from './enrollment-preview';
import type { EnrollmentDraft, EnrollmentDraftId } from './contracts/enrollment';
import type { AssessmentResponse, AssessmentRoundId } from './contracts/assessments';
import type { AssessmentAttemptId, CriterionResult, DiagnosticView, PathRecommendation, PathTopic } from './contracts/placement';
type Bank = typeof banks.courses[number];
type Question = Bank['questions'][number];
interface Round { id: AssessmentRoundId; questions: Question[]; revision: number; responses: Record<string, AssessmentResponse>; result: CriterionResult[] | null }
interface Attempt { id: AssessmentAttemptId; draft: EnrollmentDraft; bank: Bank; rounds: Round[]; completed: boolean }
const memory = new Map<string, Attempt[]>();
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
function draft(id: EnrollmentDraftId, revision?: number) {
  const value = findPreviewEnrollmentDraft(id);
  if (!value) throw new Error('Enrollment draft not found.');
  if (revision !== undefined && (value.revision !== revision || value.status !== 'draft' || !same(value.course, previewEnrollmentOptions(value.course.course_id).course))) throw new Error('Setup or curriculum changed; save and reload the current draft.');
  return value;
}
const bank = (id: string) => banks.courses.find((b) => b.course_id === id)!;
function history(id: EnrollmentDraftId): Attempt[] {
  const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(`principia:preview:placement:${id}`) : null;
  const values = raw ? JSON.parse(raw) : memory.get(id) ?? [];
  if (!Array.isArray(values)) throw new Error('The saved preview check is unreadable and has been retained.');
  return structuredClone(values);
}
function save(id: EnrollmentDraftId, values: Attempt[]) {
  if (typeof localStorage !== 'undefined') localStorage.setItem(`principia:preview:placement:${id}`, JSON.stringify(values));
  memory.set(id, structuredClone(values));
}
function latest(id: EnrollmentDraftId) { const values = history(id); const attempt = values.at(-1); if (!attempt) throw new Error('Start a diagnostic first.'); return { values, attempt, round: attempt.rounds.at(-1)! }; }
function collect(attempt: Attempt): CriterionResult[] {
  const result: CriterionResult[] = [];
  for (const round of attempt.rounds) for (const row of round.result ?? []) {
    const prior = result.find((r) => r.id === row.id);
    if (!prior) result.push(structuredClone(row));
    else { if (row.verdict !== 'unknown') prior.verdict = row.verdict; prior.evidence.push(...structuredClone(row.evidence)); }
  }
  return result;
}
function followups(attempt: Attempt) {
  if (attempt.completed || attempt.rounds.length !== 1 || !attempt.rounds[0].result) return [];
  const results = collect(attempt);
  return attempt.bank.questions.filter((q) => q.followup_for && results.some((r) => r.id === q.followup_for && r.verdict !== 'passed')).slice(0, 2);
}
function view(id: EnrollmentDraftId, attempt: Attempt): DiagnosticView {
  const round = attempt.rounds.at(-1)!;
  const current = draft(id);
  return {
    attempt_id: attempt.id, round_id: round.id, ordinal: attempt.rounds.length, revision: round.revision,
    questions: round.questions.map(({ id, label, prompt, choices }) => ({ id, label, prompt, choices })),
    responses: Object.fromEntries(round.questions.map((q) => [q.id, round.responses[q.id] ?? { answer: '', status: 'draft' }])),
    criteria: collect(attempt), submitted: round.result !== null, completed: attempt.completed,
    can_follow_up: followups(attempt).length > 0,
    matches_draft: current.status === 'draft' && same(current, attempt.draft) && same(current.course, previewEnrollmentOptions(current.course.course_id).course) && same(bank(current.course.course_id), attempt.bank),
    course: attempt.draft.course, scope_note: attempt.bank.scope_note,
    stages: stageSamples(attempt),
    estimated_minutes: Math.min(attempt.rounds[0].questions.length + 2, attempt.bank.estimated_minutes),
  };
}
function stageSamples(attempt: Attempt) {
  const entries = previewEnrollmentOptions(attempt.draft.course.course_id).entry_points;
  const first = new Set(attempt.rounds[0].questions.map((q) => q.id));
  return entries.flatMap((entry) => {
    const samples = attempt.bank.questions.filter((q) => q.entry_point === entry.id && first.has(q.id)).map((q) => q.label);
    return samples.length ? [{ id: entry.id, label: entry.label, questions: samples.length, samples }] : [];
  });
}
function round(questions: Question[]): Round { return { id: `preview-round-${crypto.randomUUID()}` as AssessmentRoundId, questions: structuredClone(questions), revision: 0, responses: {}, result: null }; }
export async function getPreviewPlacement(id: EnrollmentDraftId) { draft(id); const attempt = history(id).at(-1); return attempt ? view(id, attempt) : null; }
export async function startPreviewPlacement(id: EnrollmentDraftId, revision: number, restart = false) {
  const current = draft(id, revision);
  if (current.configuration.entry.route !== 'diagnostic') throw new Error('Choose the diagnostic entry route first.');
  const values = history(id); const existing = values.at(-1);
  if (existing && (!restart || !existing.completed)) return view(id, existing);
  const definition = bank(current.course.course_id);
  const entries = previewEnrollmentOptions(current.course.course_id).entry_points;
  const goal = current.configuration.goal;
  const target = goal.kind === 'language_level' ? entries.findIndex((e) => e.id === goal.target_level) : entries.length - 1;
  const questions = definition.questions.filter((q) => !q.followup_for && entries.findIndex((e) => e.id === q.entry_point) <= target);
  const attempt: Attempt = { id: `preview-assessment-${crypto.randomUUID()}` as AssessmentAttemptId, draft: structuredClone(current), bank: structuredClone(definition), rounds: [round(questions)], completed: false };
  values.push(attempt); save(id, values); return view(id, attempt);
}
export async function savePreviewPlacement(id: EnrollmentDraftId, roundId: AssessmentRoundId, revision: number, questionId: string, response: AssessmentResponse) {
  const { values, attempt, round } = latest(id);
  const question = round.questions.find((q) => q.id === questionId);
  if (round.id !== roundId || !question || round.result || attempt.completed) throw new Error('The diagnostic round changed or is already submitted.');
  if ((response.status === 'answered' && !response.answer) || (response.answer && !question.choices.some((c) => c.id === response.answer)) || (response.status === 'skipped' && response.answer)) throw new Error('Select an available answer or explicitly skip.');
  if (same(round.responses[questionId], response)) return round.revision;
  if (round.revision !== revision) throw new Error('Answers changed; reload the saved revision.');
  round.responses[questionId] = structuredClone(response); round.revision += 1; save(id, values); return round.revision;
}
export async function submitPreviewPlacement(id: EnrollmentDraftId, roundId: AssessmentRoundId, revision: number) {
  const { values, attempt, round } = latest(id);
  if (round.id !== roundId) throw new Error('Diagnostic round changed.');
  if (round.result) return view(id, attempt);
  if (round.revision !== revision) throw new Error('Answers changed; reload the saved revision.');
  const result: CriterionResult[] = round.questions.map((q) => {
    const response = round.responses[q.id];
    if (!response || response.status === 'draft') throw new Error('Confirm or skip every question.');
    const verdict = response.status === 'skipped' ? 'unknown' : response.answer === q.answer ? 'passed' : 'needs_practice';
    return { id: q.criterion, competency: q.competency, entry_point: q.entry_point, label: q.label, verdict, evidence: [{ question_id: q.id, response, verdict, explanation: q.explanation, expected_answer: q.choices.find((c) => c.id === q.answer)!.text }] };
  });
  round.result = result; if (attempt.rounds.length > 1) attempt.completed = true; save(id, values); return view(id, attempt);
}
export async function continuePreviewPlacement(id: EnrollmentDraftId, roundId: AssessmentRoundId) {
  const { values, attempt } = latest(id);
  if (attempt.rounds.length === 2 && attempt.rounds[0].id === roundId) return view(id, attempt);
  if (attempt.rounds.at(-1)!.id !== roundId) throw new Error('Diagnostic round changed.');
  const selected = followups(attempt);
  if (!selected.length) throw new Error('No further prerequisite questions are available for this check.');
  attempt.rounds.push(round(selected)); save(id, values); return view(id, attempt);
}
export async function finishPreviewPlacement(id: EnrollmentDraftId, roundId: AssessmentRoundId) {
  const { values, attempt, round } = latest(id);
  if (round.id !== roundId || !round.result) throw new Error('Submit the current round before finishing.');
  attempt.completed = true; save(id, values); return view(id, attempt);
}
export async function recommendPreviewPath(id: EnrollmentDraftId, revision: number): Promise<PathRecommendation> {
  const current = draft(id, revision); const course = courseDefinition(current.course.course_id)!;
  const entries = course.entry_points; const entry = current.configuration.entry;
  let start = entry.route === 'manual' ? entries.findIndex((e) => e.id === entry.entry_point) : 0;
  let criteria: CriterionResult[] = []; let attemptId: AssessmentAttemptId | null = null;
  let explanation = entry.route === 'foundations' ? 'Begin with the introductory material. Familiar topics can be revisited or checked later.' : 'Your declared starting point sets this provisional route. Earlier material remains available; this choice does not award grades or mastery.';
  if (entry.route === 'diagnostic') {
    const { attempt } = latest(id); const diagnostic = view(id, attempt);
    if (!attempt.completed) throw new Error('Finish the diagnostic or choose another entry route before reviewing the path.');
    if (!diagnostic.matches_draft) throw new Error('The setup, goal or diagnostic bank changed; complete a new check or choose a manual starting point.');
    criteria = diagnostic.criteria; attemptId = attempt.id;
    for (const [index, stage] of entries.entries()) {
      const rows = criteria.filter((r) => r.entry_point === stage.id);
      if (rows.length !== 3 || !rows.every((r) => r.verdict === 'passed')) break;
      start = Math.min(index + 1, entries.length - 1);
    }
    explanation = 'The check sampled every stage of this course. Your lessons begin at the first stage you did not fully demonstrate, and anything missed earlier becomes a refresher before the work that depends on it.';
  }
  const goal = current.configuration.goal;
  if (goal.kind === 'language_level') start = Math.min(start, entries.findIndex((e) => e.id === goal.target_level));
  const earlier: PathTopic[] = []; const refreshers: PathTopic[] = [];
  if (course.kind === 'engineering') {
    const curriculum = concepts.filter((c) => c.focus === course.id);
    const prior = curriculum.filter((c) => entries.findIndex((e) => e.id === c.curriculum.phase) < start);
    for (const topic of prior) earlier.push({ id: topic.slug, label: topic.title, reason: entry.route === 'manual' && entry.familiar_competencies.includes(topic.slug) ? 'Declared familiar; not assessed.' : 'Earlier material remains available for optional review; no completion credit is awarded.' });
    const dependencies = new Set(curriculum.filter((c) => !prior.includes(c)).flatMap((c) => c.prereqs));
    for (const topic of earlier) if (dependencies.has(topic.id) && !criteria.some((r) => r.competency === topic.id && r.verdict === 'passed')) refreshers.push({ ...topic, reason: 'An upcoming topic depends on this earlier material; check it before dependent work.' });
  } else for (const band of entries.slice(0, start)) earlier.push({ id: band.id, label: `${band.label} introductory material`, reason: 'A provisional band preference; listening, speaking and writing remain separately assessed.' });
  for (const row of criteria) if (row.verdict !== 'passed' && entries.findIndex((e) => e.id === row.entry_point) < start && !refreshers.some((r) => r.id === row.competency)) refreshers.push({ id: row.competency, label: row.label, reason: row.verdict === 'unknown' ? 'This prerequisite sample was skipped and remains unknown.' : 'This sample showed a gap; revisit it before dependent work.' });
  return { id: `preview-recommendation-${id}-${revision}-${attemptId ?? entry.route}`, draft_id: id, draft_revision: revision, course: current.course, route: entry.route, entry_point: entries[start].id, entry_label: entries[start].label, explanation, earlier_topics: earlier, refreshers, criteria, required_outcome: course.outcome, assessment_attempt_id: attemptId };
}
