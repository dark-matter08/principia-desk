/**
 * The class builder in the browser preview: an in-memory copy of what the
 * desk stores, a canned draft standing in for the tutor, and the same
 * validator rules the native side holds a draft to, so the editor behaves
 * the same way before the desktop app is attached.
 */
import type { ClassExport, CourseBrief, CourseChecks, CourseDraft, CustomCourseSummary, CustomCourseView, DraftIssue, DraftTopic, QuestionBank, ReviewFinding, SourceCheck } from './ipc';
import { registerCourses, type CourseDefinition } from './catalog';
import { OBJECTION, proseReport } from './prose';

export const STAGES: { id: DraftTopic['curriculum']['phase']; label: string }[] = [
  { id: 'foundations', label: 'Foundations' },
  { id: 'mechanisms', label: 'Core mechanisms' },
  { id: 'production', label: 'Practical application' },
  { id: 'synthesis', label: 'Integration and capstone' },
];
export const MIN_TOPICS = 6;
export const MAX_TOPICS = 60;

const tier = (phase: string) => ({ foundations: 0, mechanisms: 1, production: 2 }[phase] ?? 3);
const words = (text: string) => text.trim().split(/\s+/).filter(Boolean).length;
const hostOf = (url: string) => { try { return new URL(url).hostname.toLowerCase(); } catch { return null; } };
const onHost = (hosts: string[], url: string) => { const host = hostOf(url); return !!host && hosts.some((h) => host === h || host.endsWith(`.${h}`)); };

export function slugify(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48);
}

/** Mirror of `domain::custom::validate`; the native result stays authoritative. */
export function validateDraft(draft: CourseDraft): DraftIssue[] {
  const issues: DraftIssue[] = [];
  const issue = (at: string, message: string) => issues.push({ at, message });
  if (!draft.label.trim()) issue('label', 'give the class a name');
  if (!draft.short_code.trim() || draft.short_code.trim().length > 4) issue('short_code', 'a short code is one to four characters');
  if (words(draft.summary) < 6) issue('summary', 'the summary needs a sentence');
  if (words(draft.outcome) < 15) issue('outcome', 'the outcome names what you will be able to make or do at the end; say it in at least fifteen words');
  if (words(draft.context) < 15) issue('context', 'the context tells the tutor how this subject should be taught; at least fifteen words');
  if (words(draft.environment) < 3) issue('environment', 'name the working environment');
  for (const field of ['summary', 'outcome', 'context'] as const) { const found = proseReport(draft[field]); if (found) issue(field, `${OBJECTION}${found}`); }
  if (!draft.source_hosts.length) issue('source_hosts', 'add at least one documentation host');
  for (const host of draft.source_hosts) if (!host.trim() || /[\/ ]/.test(host) || !host.includes('.')) issue('source_hosts', `${JSON.stringify(host)} is not a hostname`);
  if (draft.entry_points.map((e) => e.id).join(',') !== STAGES.map((s) => s.id).join(',')) issue('entry_points', 'the four stages must be foundations, mechanisms, production and synthesis, in that order');
  if (draft.entry_points.some((e) => !e.label.trim())) issue('entry_points', 'every stage needs a label');
  if (draft.topics.length < MIN_TOPICS) issue('topics', `a course needs at least ${MIN_TOPICS} topics`);
  if (draft.topics.length > MAX_TOPICS) issue('topics', `a course holds at most ${MAX_TOPICS} topics`);
  const slugs = new Set<string>(); const titles = new Set<string>();
  const index = new Map(draft.topics.map((t) => [t.slug, t]));
  for (const topic of draft.topics) {
    const at = `topics/${topic.slug}`;
    if (!topic.slug || !/^[a-z0-9-]+$/.test(topic.slug)) issue(at, 'a topic slug is lowercase letters, digits and hyphens');
    if (slugs.has(topic.slug)) issue(at, 'two topics share this slug'); slugs.add(topic.slug);
    if (!topic.title.trim()) issue(at, 'the topic needs a title');
    else if (titles.has(topic.title.trim().toLowerCase())) issue(at, 'two topics share this title'); else titles.add(topic.title.trim().toLowerCase());
    if (!topic.category.trim()) issue(at, 'give the topic a category');
    const b = topic.curriculum;
    if (!['foundations', 'mechanisms', 'production', 'synthesis', 'elective'].includes(b.phase)) issue(at, 'phase must be foundations, mechanisms, production, synthesis, or elective');
    if (words(b.learner_outcome) < 8) issue(at, 'learner outcome is too vague');
    if (b.mechanisms.filter((m) => m.trim()).length < 2) issue(at, 'at least two named mechanisms are required');
    if (words(b.production_scenario) < 8) issue(at, 'production scenario is too vague');
    if (!b.misconceptions.filter((m) => m.trim()).length) issue(at, 'at least one misconception is required');
    if (words(b.evidence) < 6) issue(at, 'observable evidence is too vague');
    if (words(b.artifact) < 6) issue(at, 'cumulative artifact is too vague');
    if (b.primary_sources.length < 2 || b.primary_sources.some((s) => !/^https?:\/\//.test(s))) issue(at, 'at least two absolute primary-source URLs are required');
    const found = proseReport([topic.title, b.learner_outcome, b.production_scenario, b.evidence, b.artifact, ...b.mechanisms, ...b.misconceptions].join('\n'));
    if (found) issue(at, `${OBJECTION}${found}`);
    for (const p of topic.prereqs) {
      const earlier = index.get(p);
      if (!earlier) issue(at, `prerequisite ${p} is not a topic`);
      else if (tier(earlier.curriculum.phase) > tier(b.phase)) issue(at, `prerequisite ${p} sits in a later stage`);
      if (p === topic.slug) issue(at, 'a topic cannot require itself');
    }
    for (const source of b.primary_sources) if (/^https?:\/\//.test(source) && !onHost(draft.source_hosts, source)) issue(at, `source ${source} is not on one of the course's hosts`);
  }
  for (const stage of STAGES) if (!draft.topics.some((t) => t.curriculum.phase === stage.id && t.curriculum.core)) issue('topics', `the ${stage.id} stage needs at least one core topic`);
  const visited = new Set<string>();
  const visit = (slug: string, visiting: Set<string>): boolean => {
    if (visited.has(slug)) return true;
    if (visiting.has(slug)) return false;
    visiting.add(slug);
    for (const p of index.get(slug)?.prereqs ?? []) if (!visit(p, visiting)) return false;
    visiting.delete(slug); visited.add(slug); return true;
  };
  for (const topic of draft.topics) if (!visit(topic.slug, new Set())) issue(`topics/${topic.slug}`, 'prerequisites go round in a circle');
  return issues;
}

export function blankTopic(phase: DraftTopic['curriculum']['phase'] = 'foundations'): DraftTopic {
  return { slug: '', title: '', category: 'fundamentals', prereqs: [], curriculum: { phase, core: true, learner_outcome: '', mechanisms: ['', ''], production_scenario: '', misconceptions: [''], evidence: '', artifact: '', primary_sources: ['', ''], related_concepts: [] } };
}

function shortCode(title: string): string {
  const code = title.split(/\s+/).map((w) => w.replace(/[^a-z0-9]/gi, '')[0]).filter(Boolean).slice(0, 3).join('').toUpperCase();
  return code || 'MY';
}

export function blankDraft(id: string, brief: CourseBrief): CourseDraft {
  return { id, label: brief.title.trim(), native_label: '', short_code: shortCode(brief.title), title: brief.title.trim(), summary: '', context: '', outcome: brief.outcome.trim(), environment: '', source_hosts: [...brief.trusted_hosts], entry_points: STAGES.map((s) => ({ ...s })), topics: [{ ...blankTopic('foundations'), slug: 'first-topic', title: 'First topic' }] };
}

/** What the tutor would draft for the brief, in the preview. */
export function cannedDraft(id: string, brief: CourseBrief): CourseDraft {
  const hosts = brief.trusted_hosts.length ? [...brief.trusted_hosts] : ['doc.rust-lang.org', 'docs.rs'];
  const [h1, h2] = [hosts[0], hosts[1] ?? hosts[0]];
  const topic = (slug: string, title: string, phase: DraftTopic['curriculum']['phase'], core: boolean, prereqs: string[], category = 'fundamentals'): DraftTopic => ({
    slug, title, category, prereqs,
    curriculum: { phase, core, learner_outcome: `Explain ${title.toLowerCase()} with a worked example and a measured result of your own`, mechanisms: ['the mechanism underneath', 'what the tool does with it'], production_scenario: `A tool in daily use meets ${title.toLowerCase()} on a busy day and someone has to fix it before lunch`, misconceptions: ['it is not magic; there is a rule and it can be observed'], evidence: 'A trace showing the mechanism at work in the tool', artifact: 'A short note with the trace and the fix applied', primary_sources: [`https://${h1}/`, `https://${h2}/`], related_concepts: [] },
  });
  return {
    id, label: brief.title.trim(), native_label: 'your own course', short_code: shortCode(brief.title), title: brief.title.trim(),
    summary: `A self-study course so you can ${brief.outcome.trim().replace(/\.$/, '').toLowerCase()}.`,
    context: 'Teach each idea through something the learner runs and measures on their own machine, comparing each choice with what actually happens, and naming the mechanism before the tool.',
    outcome: `${brief.outcome.trim().replace(/\.$/, '')}, and explain each design choice with evidence from your own runs.`,
    environment: 'A terminal, a text editor and the toolchain the subject needs.',
    source_hosts: hosts,
    entry_points: [{ id: 'foundations', label: 'First steps' }, { id: 'mechanisms', label: 'How it works' }, { id: 'production', label: 'Under real constraints' }, { id: 'synthesis', label: 'The capstone' }],
    topics: [
      topic('setting-up', 'Setting up and running the first program', 'foundations', true, []),
      topic('values-and-types', 'Values, types and the first surprises', 'foundations', true, ['setting-up']),
      topic('control-flow', 'Control flow and early returns', 'foundations', true, ['values-and-types']),
      topic('errors', 'Errors as values, not events', 'mechanisms', true, ['control-flow'], 'mechanisms'),
      topic('modules', 'Modules, visibility and the build', 'mechanisms', true, ['setting-up'], 'mechanisms'),
      topic('io', 'Reading input and writing output', 'mechanisms', false, ['errors'], 'mechanisms'),
      topic('arguments', 'Parsing arguments and giving help', 'production', true, ['errors', 'modules'], 'tools'),
      topic('testing', 'Tests that run in a second', 'production', true, ['modules'], 'tools'),
      topic('performance', 'Measuring before optimising', 'production', false, ['io'], 'tools'),
      topic('release', 'Building a release and shipping it', 'synthesis', true, ['arguments', 'testing'], 'capstone'),
      topic('capstone', 'The capstone tool, end to end', 'synthesis', true, ['release'], 'capstone'),
    ],
  };
}

/** What the desk stores: the review and the fetch are absent until run. */
interface Stored extends CustomCourseView { reviewed: boolean; fetched: boolean; marks: { review_hash: string; read_hash: string } }
const stored = new Map<string, Stored>();

/** Mirror of `domain::custom::merge_reviews`: a fresh read keeps what was settled. */
export function mergeReviews(previous: ReviewFinding[], fresh: ReviewFinding[]): ReviewFinding[] {
  const wordsOf = (m: string) => new Set(m.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length >= 3));
  const same = (a: ReviewFinding, b: ReviewFinding) => {
    if (a.topic !== b.topic) return false;
    if (a.message.trim().toLowerCase() === b.message.trim().toLowerCase()) return true;
    const x = wordsOf(a.message), y = wordsOf(b.message);
    const shared = [...x].filter((w) => y.has(w)).length;
    const all = new Set([...x, ...y]).size;
    return all > 0 && shared * 2 >= all;
  };
  const used = previous.map(() => false);
  const merged = fresh.map((f) => {
    const next: ReviewFinding = { ...f, status: 'open', note: '', carried: false };
    const at = previous.findIndex((old, i) => !used[i] && same(old, next));
    if (at >= 0) { used[at] = true; next.status = previous[at].status; next.note = previous[at].note; }
    return next;
  });
  previous.forEach((old, i) => { if (!used[i]) merged.push({ ...old, carried: true }); });
  return merged;
}

/** The stored draft, hashed the cheap way; only equality matters here. */
function draftHash(draft: CourseDraft): string {
  const text = JSON.stringify(draft);
  let h = 0;
  for (let i = 0; i < text.length; i++) h = (Math.imul(h, 31) + text.charCodeAt(i)) | 0;
  return `${text.length}-${h >>> 0}`;
}

/** Mirror of `domain::custom::checks_of`: what publishing still needs, in order. */
export function checksOf(item: { draft: CourseDraft; review: ReviewFinding[] | null; sources: SourceCheck[] | null; marks: Stored['marks'] }, issues: DraftIssue[]): CourseChecks {
  const hash = draftHash(item.draft);
  const urls = new Set(item.draft.topics.flatMap((t) => t.curriculum.primary_sources));
  const checked = new Set((item.sources ?? []).map((s) => s.url));
  const unchecked = [...urls].filter((u) => !checked.has(u)).length;
  const pending = new Set((item.sources ?? []).filter((s) => s.state !== 'reachable' && !s.accepted && urls.has(s.url)).map((s) => s.url)).size;
  const open = (item.review ?? []).filter((f) => f.status === 'open').length;
  const checks: CourseChecks = { draft_hash: hash, reviewed: item.review !== null, review_current: item.review !== null && item.marks.review_hash === hash, open_findings: open, fetched: item.sources !== null, unchecked_sources: unchecked, pending_sources: pending, read: item.marks.read_hash === hash, blockers: [] };
  const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);
  if (issues.length) checks.blockers.push(`${issues.length} ${plural(issues.length, 'thing', 'things')} to fix in the editor`);
  if (!checks.reviewed) checks.blockers.push('the tutor has not read the draft back');
  else if (open) checks.blockers.push(`${open} ${plural(open, 'finding', 'findings')} from the review still open`);
  if (!checks.fetched) checks.blockers.push('the sources have not been fetched');
  else {
    if (unchecked) checks.blockers.push(`${unchecked} ${plural(unchecked, 'source', 'sources')} added since the last fetch`);
    if (pending) checks.blockers.push(`${pending} unreachable ${plural(pending, 'source', 'sources')} neither replaced nor accepted`);
  }
  if (!checks.read) checks.blockers.push('your own read-through of this version is not confirmed');
  return checks;
}

/** The mock desk registers a program for a published class through this. */
let publishHook: ((course: CourseDefinition) => void) | null = null;
export function onPreviewPublish(hook: (course: CourseDefinition) => void) { publishHook = hook; }
/** The mock desk's state refresh, announced the way the desk announces one. */
let tellHook: (() => void) | null = null;
export function onPreviewTell(hook: () => void) { tellHook = hook; }
const previewTell = () => tellHook?.();

function now() { return new Date().toISOString(); }
/** Reviews and fetches not yet run are kept as `null` in the store; the view shows them as empty lists. */
function view(id: string): CustomCourseView {
  const item = stored.get(id);
  if (!item) throw new Error('this class does not exist');
  const issues = validateDraft(item.draft);
  const { marks: _marks, reviewed: _reviewed, fetched: _fetched, ...rest } = item;
  return { ...rest, issues, checks: checksOf({ draft: item.draft, review: item.reviewed ? item.review : null, sources: item.fetched ? item.sources : null, marks: item.marks }, issues), draft: structuredClone(item.draft), brief: { ...item.brief }, review: item.review.map((f) => ({ ...f })), sources: item.sources.map((s) => ({ ...s })), bank: item.bank ? structuredClone(item.bank) : null, working: item.working ?? null, working_at: item.working_at ?? null };
}

function newId(title: string): string {
  const base = `custom-${slugify(title) || 'class'}`;
  let id = base; let n = 2;
  while (stored.has(id)) id = `${base}-${n++}`;
  return id;
}

export const previewCustom = {
  list: async (): Promise<CustomCourseSummary[]> => [...stored.values()].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).map((c) => ({ id: c.id, label: c.draft.label, version: c.version, status: c.status, origin: c.origin, topics: c.draft.topics.length, updated_at: c.updated_at })),
  get: async (id: string) => view(id),
  create: async (brief: CourseBrief, origin: CustomCourseView['origin']) => {
    if (!brief.title.trim()) throw new Error('give the class a title');
    if (words(brief.outcome) < 5) throw new Error('say what you want to be able to do, in a sentence at least');
    const id = newId(brief.title);
    stored.set(id, { id, version: 0, status: 'draft', origin, brief: { ...brief }, draft: blankDraft(id, brief), issues: [], review: [], sources: [], checks: checksOf({ draft: blankDraft(id, brief), review: null, sources: null, marks: { review_hash: '', read_hash: '' } }, []), bank: null, created_at: now(), updated_at: now(), published_at: null, reviewed: false, fetched: false, marks: { review_hash: '', read_hash: '' } });
    return view(id);
  },
  saveBrief: async (id: string, brief: CourseBrief) => { const item = stored.get(id); if (!item) throw new Error('this class does not exist'); item.brief = { ...brief }; item.updated_at = now(); return view(id); },
  saveDraft: async (id: string, draft: CourseDraft) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    item.draft = structuredClone({ ...draft, id, topics: draft.topics.map((t) => ({ ...t, slug: slugify(t.slug || t.title) })) });
    item.updated_at = now(); return view(id);
  },
  draft: async (id: string) => { const item = stored.get(id); if (!item) throw new Error('this class does not exist'); item.working = 'draft'; await new Promise((r) => setTimeout(r, 1800)); item.draft = cannedDraft(id, item.brief); item.working = null; item.updated_at = now(); return view(id); },
  review: async (id: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    await new Promise((r) => setTimeout(r, 1200));
    const first = item.draft.topics[0]?.slug ?? '';
    const findings: ReviewFinding[] = [
      { severity: 'medium', topic: first, message: 'The first topic assumes a toolchain is installed; a learner starting from nothing has no step for that.', fix: 'Add an installation-and-first-run step before it, or fold one into its lesson outcome.', status: 'open', note: '' },
      { severity: 'low', topic: '', message: 'The capstone stage has no elective; a learner who finishes early has nowhere to go.', fix: 'Add one elective topic that extends the capstone.', status: 'open', note: '' },
    ];
    item.review = mergeReviews(item.review, findings); item.marks.review_hash = draftHash(item.draft); item.reviewed = true; item.updated_at = now(); return view(id);
  },
  fixAll: async (id: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const open = item.review.map((f, i) => (f.status === 'open' ? i : -1)).filter((i) => i >= 0);
    if (!open.length) throw new Error('every finding is already settled');
    // The desk announces each step with a refresh; the preview does the same.
    for (const index of open) { item.working = 'fix'; item.working_at = index; previewTell(); await previewCustom.fixFinding(id, index); item.working = 'fix'; previewTell(); }
    item.working = null; item.working_at = null;
    return view(id);
  },
  fixFinding: async (id: string, index: number) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const finding = item.review[index]; if (!finding) throw new Error('that finding is not in the review');
    item.working = 'fix'; item.working_at = index;
    await new Promise((r) => setTimeout(r, 1400));
    // The preview's tutor makes the smallest change: a prerequisite topic, or an elective.
    if (finding.topic) {
      const at = item.draft.topics.findIndex((t) => t.slug === finding.topic);
      const fresh = { ...blankTopic(item.draft.topics[at]?.curriculum.phase ?? 'foundations'), slug: `${finding.topic}-setup`, title: `Installing the toolchain and running the first program`, category: 'fundamentals' };
      fresh.curriculum = { ...fresh.curriculum, learner_outcome: 'Install the toolchain, run one program and read the version it prints', mechanisms: ['the toolchain on the path', 'what the first run does'], production_scenario: 'A new machine at work needs the toolchain before anything else can be tried today', misconceptions: ['it is installed already'], evidence: 'The version printed by the tool in a terminal', artifact: 'A note with the install steps that worked', primary_sources: [...(item.draft.topics[at]?.curriculum.primary_sources ?? [])] };
      item.draft.topics.splice(Math.max(at, 0), 0, fresh);
      if (at >= 0) item.draft.topics[at + 1].prereqs = [...new Set([...item.draft.topics[at + 1].prereqs, fresh.slug])];
      finding.note = 'by the tutor: added a setup topic before it';
    } else {
      const last = item.draft.topics[item.draft.topics.length - 1];
      item.draft.topics.push({ ...structuredClone(last), slug: `${last.slug}-extended`, title: `${last.title}, extended`, curriculum: { ...structuredClone(last.curriculum), phase: 'elective', core: false } });
      finding.note = 'by the tutor: added an elective that extends the capstone';
    }
    finding.status = 'fixed'; item.working = null; item.working_at = null; item.updated_at = now(); return view(id);
  },
  resolveFinding: async (id: string, index: number, status: ReviewFinding['status'], note: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const finding = item.review[index]; if (!finding) throw new Error('that finding is not in the review');
    finding.status = status; finding.note = note.trim().slice(0, 400); item.updated_at = now(); return view(id);
  },
  acceptSource: async (id: string, url: string, accepted: boolean) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const hits = item.sources.filter((s) => s.url === url); if (!hits.length) throw new Error('that source was not in the last fetch');
    for (const s of hits) s.accepted = accepted && s.state !== 'reachable';
    item.updated_at = now(); return view(id);
  },
  /** The preview's search: the dead page's host index stands in for it. */
  replaceSource: async (id: string, url: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const check = item.sources.find((s) => s.url === url && s.state !== 'reachable' && !s.accepted); if (!check) throw new Error('that source is not one waiting to be settled');
    item.working = 'sources'; previewTell();
    await new Promise((r) => setTimeout(r, 1200));
    const host = item.draft.source_hosts[0] ?? 'docs.example.org';
    const replacement = `https://${host}/${check.topic}/`;
    for (const topic of item.draft.topics) topic.curriculum.primary_sources = topic.curriculum.primary_sources.map((s) => (s === url ? replacement : s));
    for (const s of item.sources) if (s.url === url) { s.url = replacement; s.state = 'reachable'; s.accepted = false; }
    item.working = null; item.updated_at = now(); return view(id);
  },
  replaceAllSources: async (id: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const cited = new Set(item.draft.topics.flatMap((t) => t.curriculum.primary_sources));
    const pending = [...new Set(item.sources.filter((s) => s.state !== 'reachable' && !s.accepted && cited.has(s.url)).map((s) => s.url))];
    if (!pending.length) throw new Error('every source is settled');
    for (const url of pending) await previewCustom.replaceSource(id, url);
    return view(id);
  },
  markRead: async (id: string, read: boolean) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    item.marks.read_hash = read ? draftHash(item.draft) : ''; return view(id);
  },
  verify: async (id: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    await new Promise((r) => setTimeout(r, 900));
    const checks: SourceCheck[] = [];
    for (const topic of item.draft.topics) for (const url of topic.curriculum.primary_sources) { const state = !onHost(item.draft.source_hosts, url) ? 'off-host' : /example\.com|invalid/.test(url) ? 'unreachable' : 'reachable'; checks.push({ topic: topic.slug, url, state, accepted: state !== 'reachable' && item.sources.some((s) => s.url === url && s.accepted) }); }
    item.sources = checks; item.fetched = true; item.updated_at = now(); return view(id);
  },
  writeBank: async (id: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const issues = validateDraft(item.draft);
    if (issues.length) throw new Error(`fix the draft before writing its questions: ${issues[0].message}`);
    await new Promise((r) => setTimeout(r, 1500));
    const stages = ['foundations', 'mechanisms', 'production', 'synthesis'];
    const questions: QuestionBank['questions'] = [];
    for (const stage of stages) {
      const core = item.draft.topics.filter((t) => t.curriculum.phase === stage && t.curriculum.core);
      for (let i = 0; i < 3; i++) {
        const topic = core[i % core.length];
        const n = questions.length + 1;
        questions.push({ id: `${id}-entry-${n}`, criterion: `${id}-criterion-${n}`, competency: `${id}-${topic.slug}`, entry_point: stage, label: `${topic.title.split(' ').slice(0, 3).join(' ')}`, prompt: `In ${topic.title.toLowerCase()}, which statement holds?`, choices: [{ id: '1', text: 'The first plausible mistake' }, { id: '2', text: 'The mechanism as the source describes it' }, { id: '3', text: 'A common misconception' }, { id: '4', text: 'An unrelated fact' }], answer: '2', explanation: 'The source names the mechanism; the others are the misconceptions the lesson warns about.', followup_for: null, source: topic.curriculum.primary_sources[0] ?? '' });
      }
    }
    item.bank = { course_id: id, version: `written-${Date.now()}`, estimated_minutes: 14, scope_note: 'Twelve short questions, three for each stage of this course, written by the tutor from the course\'s own sources.', questions };
    item.updated_at = now(); return view(id);
  },
  voidQuestion: async (id: string, questionId: string, reason: string) => {
    const item = stored.get(id); if (!item?.bank) throw new Error('this class has no question bank');
    const q = item.bank.questions.find((q) => q.id === questionId); if (!q) throw new Error('that question is not in the bank');
    q.voided = true; q.void_reason = reason.trim().slice(0, 400); item.updated_at = now(); return view(id);
  },
  publish: async (id: string) => {
    const item = stored.get(id); if (!item) throw new Error('this class does not exist');
    const issues = validateDraft(item.draft);
    if (issues.length) throw new Error(`the draft is not ready to publish: ${issues[0].message} (${issues[0].at} and ${issues.length - 1} more)`);
    const { blockers } = view(id).checks;
    if (blockers.length) throw new Error(`the class is not ready to publish: ${blockers.join('; ')}`);
    item.version += 1; item.status = 'published'; item.published_at = now(); item.updated_at = now();
    const definition = definitionOf(item);
    registerCourses([definition]);
    publishHook?.(definition);
    return view(id);
  },
  remove: async (id: string) => { const item = stored.get(id); if (!item) throw new Error('this class does not exist'); if (item.status === 'published') throw new Error('a published class keeps its lessons; retire it from its settings instead'); stored.delete(id); },
  export: async (id: string): Promise<ClassExport> => { const item = view(id); const name = `${item.draft.label.replace(/[^a-z0-9 -]/gi, '-').trim() || id}.principia-class.json`; return { path: `~/Documents/Principia Desk/classes/${name}`, file_name: name }; },
  import: async (text: string) => {
    let file: { format?: string; brief?: CourseBrief; draft?: CourseDraft };
    try { file = JSON.parse(text); } catch (error) { throw new Error(`not a class file: ${error}`); }
    if (file.format !== 'principia-class/1' || !file.draft) throw new Error(`this is not a class file the desk understands (${file.format ?? 'no format'})`);
    const brief: CourseBrief = { title: '', outcome: '', background: '', trusted_hosts: [], agent: '', model: '', custom_agent_bin: '', ...(file.brief ?? {}) };
    if (!brief.title.trim()) brief.title = file.draft.label;
    if (words(brief.outcome) < 5) brief.outcome = file.draft.outcome;
    const created = await previewCustom.create(brief, 'import');
    return previewCustom.saveDraft(created.id, { ...file.draft, id: created.id });
  },
  /** The questions a class holds, for the bank strip. */
  bankOf: (id: string) => stored.get(id)?.bank?.questions ?? [],
  /** The published courses, for the preview's catalogue. */
  published: (): CourseDefinition[] => [...stored.values()].filter((c) => c.status === 'published').map(definitionOf),
};

function definitionOf(item: Stored): CourseDefinition {
  const d = item.draft;
  return { id: d.id as CourseDefinition['id'], course_id: d.id, kind: 'engineering', label: d.label, native_label: d.native_label || 'your own course', short_code: d.short_code, title: d.title, summary: d.summary, version: `v${item.version}`, context: d.context, outcome: d.outcome, environment: d.environment, prompt_profile: `custom.${d.id}`, prerequisite_courses: [], source_hosts: [...d.source_hosts], reference_lessons: [], capabilities: ['explanation', 'knowledge_checks', 'code', 'diagrams', 'tutor'], entry_points: d.entry_points.map((e) => ({ ...e })) };
}
