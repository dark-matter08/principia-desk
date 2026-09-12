import type { AcceptedPath, AcceptPath, PathSummary, RevisePath } from './contracts/classes';
import type { ClassroomSubjectId } from './catalog.generated';
import { findPreviewEnrollmentDraft } from './enrollment-preview';
import { recommendPreviewPath } from './placement-preview';
import { previewClassRecords, savePreviewClassRecords } from './class-preview-store';

export function previewClassPath(courseId: ClassroomSubjectId): AcceptedPath | null {
  return previewClassRecords().filter(r => r.path.recommendation.course.course_id === courseId).at(-1)?.path ?? null;
}
export function previewPathSummary(path: AcceptedPath | null): PathSummary | null {
  if (!path) return null;
  return { ...path.reference, revision: path.revision, entry_point: path.recommendation.entry_point, entry_label: path.recommendation.entry_label, route: path.recommendation.route, earlier_topics: path.recommendation.earlier_topics.length, refreshers: path.recommendation.refreshers.length };
}
export async function acceptPreviewClassPath(input: AcceptPath): Promise<AcceptedPath> {
  // Recommendation evaluation is asynchronous; recheck acceptance on both sides
  // so concurrent clicks cannot create two revisions for the same draft.
  const existing = previewClassRecords().find(r => r.draft.id === input.draft_id);
  if (existing) {
    if (existing.path.recommendation.id !== input.recommendation_id || existing.draft.revision !== input.expected_revision) throw new Error('This draft was accepted with a different recommendation.');
    return existing.path;
  }
  const recommendation = await recommendPreviewPath(input.draft_id, input.expected_revision);
  if (previewClassRecords().some(r => r.draft.id === input.draft_id)) return acceptPreviewClassPath(input);
  if (recommendation.id !== input.recommendation_id) throw new Error('The recommendation changed. Review the current path before accepting it.');
  const draft = findPreviewEnrollmentDraft(input.draft_id)!;
  if (draft.configuration.focus_policy !== 'advisory') throw new Error('Class paths currently support advisory focus. Choose advisory before accepting.');
  const previous = previewClassPath(draft.course.course_id);
  const path: AcceptedPath = {
    reference: { class_id: previous?.reference.class_id ?? `preview-class-${crypto.randomUUID()}`, path_revision_id: `preview-path-${crypto.randomUUID()}`, course_snapshot_fingerprint: draft.course.fingerprint },
    revision: (previous?.revision ?? 0) + 1, configuration: structuredClone(draft.configuration), recommendation, accepted_at: new Date().toISOString(),
  };
  savePreviewClassRecords([...previewClassRecords(), { path, draft: { ...draft, status: 'accepted', accepted_class_id: path.reference.class_id, updated_at: path.accepted_at } }]);
  return path;
}

/** Browser-preview counterpart of the native path revision: same rules, no credit. */
export function revisePreviewClassPath(input: RevisePath, titleFor: (slug: string) => string | undefined): AcceptedPath {
  const records = previewClassRecords();
  const index = records.map((r) => r.path.recommendation.course.course_id as string).lastIndexOf(input.course_id);
  if (index < 0) throw new Error('Accept a learning path before revising it.');
  const current = records[index].path;
  if (current.revision !== input.expected_revision) throw new Error('The path changed. Review the current revision before revising it.');
  const plan = structuredClone(current.recommendation);
  plan.bypassed ??= []; plan.checked ??= []; plan.bridges ??= [];
  plan.declined_bridges ??= [];
  let changed = false;
  if (input.change.kind === 'accept_bridge' || input.change.kind === 'decline_bridge') {
    const { topic, before } = input.change;
    const label = titleFor(topic); const dependent = titleFor(before);
    if (!label || !dependent) throw new Error(`${!label ? topic : before} is not a topic of this course.`);
    if (input.change.kind === 'accept_bridge') {
      if (!plan.bridges.some((t) => t.id === topic)) { plan.bridges.push({ id: topic, label, reason: `Bridge before ${dependent}.` }); changed = true; }
      const declined = plan.declined_bridges.length; plan.declined_bridges = plan.declined_bridges.filter((d) => !(d.topic === topic && d.before === before)); if (declined !== plan.declined_bridges.length) changed = true;
    } else if (!plan.declined_bridges.some((d) => d.topic === topic && d.before === before)) { plan.declined_bridges.push({ topic, before }); changed = true; }
    if (!changed) throw new Error('That change would leave the route as it is.');
    const path: AcceptedPath = { ...current, reference: { ...current.reference, path_revision_id: `preview-path-${crypto.randomUUID()}` }, revision: current.revision + 1, recommendation: plan, accepted_at: new Date().toISOString() };
    savePreviewClassRecords([...records, { path, draft: records[index].draft }]);
    return path;
  }
  for (const slug of input.change.topics) {
    const title = titleFor(slug);
    if (!title) throw new Error(`${slug} is not a topic of this course.`);
    if (input.change.kind === 'bypass') {
      if (plan.bypassed.some((t) => t.id === slug)) continue;
      plan.bypassed.push({ id: slug, label: title, reason: 'Bypassed by choice; not assessed and not counted as coverage.' });
      if (!plan.earlier_topics.some((t) => t.id === slug)) plan.earlier_topics.push({ id: slug, label: title, reason: 'Bypassed by choice; available for voluntary study.' });
      changed = true;
    } else if (input.change.kind === 'check_out') {
      if (plan.checked.some((t) => t.id === slug)) continue;
      plan.checked.push({ id: slug, label: title, reason: `Prior knowledge checked by unit challenge ${input.change.attempt_id}; not completed here.` });
      plan.bypassed = plan.bypassed.filter((t) => t.id !== slug);
      if (!plan.earlier_topics.some((t) => t.id === slug)) plan.earlier_topics.push({ id: slug, label: title, reason: 'Prior knowledge checked; available for voluntary study.' });
      changed = true;
    } else {
      const before = plan.earlier_topics.length + plan.bypassed.length + plan.checked.length;
      plan.earlier_topics = plan.earlier_topics.filter((t) => t.id !== slug);
      plan.bypassed = plan.bypassed.filter((t) => t.id !== slug);
      plan.checked = plan.checked.filter((t) => t.id !== slug);
      if (plan.earlier_topics.length + plan.bypassed.length + plan.checked.length !== before) changed = true;
    }
  }
  if (!changed) throw new Error('That change would leave the route as it is.');
  const path: AcceptedPath = { ...current, reference: { ...current.reference, path_revision_id: `preview-path-${crypto.randomUUID()}` }, revision: current.revision + 1, recommendation: plan, accepted_at: new Date().toISOString() };
  savePreviewClassRecords([...records, { path, draft: records[index].draft }]);
  return path;
}
