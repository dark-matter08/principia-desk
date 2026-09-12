import { previewConfiguration, savePreviewConfiguration, previewRunners, previewModels, previewLocal, rememberPreviewModel, desktopRequired } from './features/runners/preview';
import type { FocusPolicy, CurriculumConceptView, RouteSummary, ExecutionRun, ExecutionLogLine, PracticeBankView, BankQuestion } from './ipc';
import type { RevisePath } from './contracts/classes';
import type { AgentPolicy, RunnerId } from './contracts/agents';
import { previewEnrollmentOptions, previewEnrollmentDraft, savePreviewEnrollmentDraft } from './enrollment-preview';
import { getPreviewPlacement, startPreviewPlacement, savePreviewPlacement, submitPreviewPlacement, continuePreviewPlacement, finishPreviewPlacement, recommendPreviewPath } from './placement-preview';
import type { SaveEnrollmentDraft } from './contracts/enrollment';
import type { AcceptedPath, AcceptPath } from './contracts/classes';
import { acceptPreviewClassPath, previewClassPath, previewPathSummary, revisePreviewClassPath } from './class-preview';
import { applyPreviewChallenge, getPreviewChallenge, savePreviewChallengeResponse, startPreviewChallenge, submitPreviewChallenge } from './challenge-preview';
import { guessStatus } from './features/recovery/ladder';
import { previewCustom, onPreviewPublish, onPreviewTell } from './custom-preview';
import { scheduleConflicts, conflictMessage, type ScheduleCandidate, type ConflictScope } from './features/classes/schedule-conflicts';
import { COURSES, courseDefinition } from './catalog';
import seedConcepts from '../../src-tauri/seed/concepts.json';
/**
 * Demo-mode API: used automatically when the frontend runs outside Tauri
 * (plain `vite dev` in a browser). Lets you develop and screenshot every
 * screen without the Rust backend. Same shapes as ipc.ts.
 */
import type {
  AppStateView,
  ProgressQuery,
  ProgressEntry,
  ProgressLesson,
  ChatMessage,
  DashboardView,
  ExerciseView,
  CefrLevel,
  ClassroomPlanView,
  ClassroomProgramView,
  ClassroomSessionStart,
  ClassroomSlotView,
  ClassroomSubjectId,
  CurriculumBrief,
  CurriculumMapView,
  EngineeringLessonView,
  EngineeringSessionResult,
  FocusArea,
  LanguageId,
  LanguageLessonView,
  LanguageProgramView,
  LanguageSessionResult,
  LessonDocument,
  LessonFileResult,
  PlannedSlot,
  RecoveryReply,
  RecoveryStatus,
  SearchProvider,
  SearchResult,
  SearchSettingsView,
  SessionPlan,
  StudyPulse,
} from './ipc';

/** A believable half year of study for the preview: weekday sessions with a few gaps and a live streak. */
function mockPulse(): StudyPulse {
  const today = new Date();
  today.setHours(12, 0, 0, 0);
  const weekday = (today.getDay() + 6) % 7;
  const first = new Date(today);
  first.setDate(today.getDate() - weekday - 7 * 25);
  const days: StudyPulse['days'] = [];
  let seed = 7;
  const random = () => { seed = (seed * 9301 + 49297) % 233280; return seed / 233280; };
  for (let cursor = new Date(first); cursor <= today; cursor.setDate(cursor.getDate() + 1)) {
    const dow = (cursor.getDay() + 6) % 7;
    const ago = Math.round((today.getTime() - cursor.getTime()) / 86400000);
    const studied = ago < 6 ? true : dow < 5 ? random() > 0.28 : random() > 0.72;
    const completed = studied ? (random() > 0.75 ? 2 : 1) : 0;
    days.push({ date: cursor.toISOString().slice(0, 10), completed, minutes: completed * (random() > 0.5 ? 30 : 45), classes: completed ? (completed > 1 ? ['SD', 'LB'] : ['SD']) : [] });
  }
  const studyDays = days.filter((day) => day.completed > 0).length;
  const week = days.slice(-(weekday + 1));
  return {
    today: days[days.length - 1].date,
    streak: 6,
    longest_streak: 14,
    study_days: studyDays,
    completed_sessions: days.reduce((sum, day) => sum + day.completed, 0),
    days,
    week_minutes: week.reduce((sum, day) => sum + day.minutes, 0),
    week_target_minutes: 210,
    week_sessions: week.reduce((sum, day) => sum + day.completed, 0),
    pass_rate: 0.82,
  };
}

let mockSearchProvider: SearchProvider = 'none';
let mockSearxngUrl = 'http://127.0.0.1:8899';
let mockBraveKey = false;
let mockTavilyKey = false;
function mockSearch(): SearchSettingsView {
  const available = mockSearchProvider === 'searxng' || (mockSearchProvider === 'brave' && mockBraveKey) || (mockSearchProvider === 'tavily' && mockTavilyKey);
  const why = mockSearchProvider === 'none' ? "Web search is off. Lessons use only the curriculum's own sources."
    : mockSearchProvider === 'searxng' ? `SearXNG is answering at ${mockSearxngUrl}.`
    : available ? `${mockSearchProvider === 'brave' ? 'Brave Search' : 'Tavily'} key is set.` : `No ${mockSearchProvider === 'brave' ? 'Brave Search' : 'Tavily'} key is set.`;
  return { provider: mockSearchProvider, searxng_url: mockSearxngUrl, default_searxng_url: 'http://127.0.0.1:8899', brave_key_set: mockBraveKey, tavily_key_set: mockTavilyKey, available, why };
}

// References and course metadata share the native authoring inputs. Browser
// demos should never label an event-loop lesson as Bash or System Design.
interface ReferenceLesson {
  slug: string;
  title: string;
  markdown: string;
  resources: EngineeringLessonView['resources'];
  exercise: EngineeringLessonView['exercise'];
  questions: { prompt: string; kind: string; choices: string[] | null; correct_answer: string; explanation: string; section: string; learning_objective: string }[];
}
const referenceFiles = import.meta.glob<ReferenceLesson>('../../src-tauri/seed/fallback_courses/*.json', { eager: true, import: 'default' });
// The bundled example of the point-form shape the tutor is asked for; `?shaped`
// in the preview URL opens it so the reader's decorations can be seen.
const shapedLessons = import.meta.glob<string>('../../src-tauri/seed/lessons/*.md', { eager: true, query: '?raw', import: 'default' });
const SHAPED_EXERCISE: NonNullable<EngineeringLessonView['exercise']> = {
  title: 'A partition timeline for one more route',
  instructions:
    '**You will produce:** a partition timeline for one route of the checkout scenario, with its policy defended.\n\n### Steps\n1. Pick either the profile email field or the avatar URL and write its normal-path request in one line.\n2. Cut the replication link mid-request and trace what each region answers, in the sequence-diagram style of the lesson.\n3. Decide the branch for the route, name the mechanism that enforces it and who owns it.\n4. State the measurable signal that would make you switch the branch.\n\n### Done when\n- The timeline names the detector and shows both branches at the cut.\n- The policy names a mechanism and an owning team.\n- The revisit signal is a number you could put on a dashboard.\n- The result reads as one row of the capstone consistency table.',
  starter_code: null,
  deliverable: 'One route traced through a partition, with its branch, mechanism, owner and revisit signal, ready to paste into the capstone table.',
  hints: ['Start from the request that crosses the failed link.', 'Ask what the client sees on each branch; that is the policy made visible.', 'The revisit signal is usually the partition rate or the oversell count.'],
};
function referenceFor(focus: FocusArea): ReferenceLesson {
  // A learner's own class has no bundled lesson; the preview borrows one.
  const slug = courseDefinition(focus)?.reference_lessons[0] ?? courseDefinition('system-design')!.reference_lessons[0];
  const lesson = Object.values(referenceFiles).find((lesson) => lesson.slug === slug);
  if (!lesson) throw new Error(`Missing reference lesson for ${focus}`);
  return lesson;
}

type Handler = (payload: unknown) => void;
const listeners = new Map<string, Handler[]>();

export function mockEmit(name: string, payload: unknown) {
  for (const h of listeners.get(name) ?? []) h(payload);
}

export function mockListen(name: string, handler: Handler): () => void {
  const arr = listeners.get(name) ?? [];
  arr.push(handler);
  listeners.set(name, arr);
  return () => listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== handler));
}

const COURSE_MD = `## Why this matters

Every async API in JavaScript, \`fetch\`, \`setTimeout\`, DOM events, promises, funnels through the same scheduling machinery. Misunderstanding which queue runs when is how you ship subtle ordering bugs, starvation under load, and "it works in the test but hangs in prod" failures. The event loop is the mental model that ties runtime, browser, and Node together.

## The simple version

Think of the event loop as a chef who always finishes every add-on ticket for the dish in front of them (microtasks) before ever glancing at the next new order that walked in (macrotasks).

\`\`\`mermaid
flowchart LR
  A[Run current macrotask] --> B{Microtask spike empty?}
  B -- no --> C[Run one microtask]
  C --> B
  B -- yes --> D[Optional render]
  D --> E[Take next macrotask]
\`\`\`

Where this breaks down: a real chef eventually moves on regardless. The event loop does not, a microtask that keeps enqueueing more microtasks can starve macrotasks (and rendering) indefinitely.

## Core mechanics

### Macrotasks vs microtasks

The event loop drains **one macrotask** (timer callback, I/O completion, user event), then **all pending microtasks** (promise reactions, \`queueMicrotask\`), then may render. Microtasks always run before the next macrotask, so a chain of \`Promise.then\` can starve timers if you recurse without yielding.

\`\`\`js
console.log('sync');
setTimeout(() => console.log('macro'), 0);
Promise.resolve().then(() => console.log('micro'));
// sync → micro → macro
\`\`\`

### The call stack and host APIs

JS runs on a single call stack per agent. Host environments enqueue work: the timer thread schedules macrotasks; the network layer resolves fetch promises as microtasks. Your code never "blocks the loop" with promises, it blocks with **synchronous** CPU work on the stack.

### \`await\` and continuation scheduling

\`await\` suspends an async function and resumes via a microtask when the operand settles. That means async/await ordering matches promise \`.then\` ordering, and errors propagate through the same microtask turn unless you \`await\` inside try/catch.

## Trade-offs and failure modes

- Microtask storms: unbounded \`queueMicrotask\` recursion prevents paint and timer delivery.
- \`setTimeout(fn, 0)\` is not "run next", it is "run after current macrotask **and** all microtasks."
- In browsers, \`requestAnimationFrame\` runs before paint; confusing it with microtasks breaks frame-aligned work.

## Interview framing

State the loop as: run script → macrotask → microtask checkpoint (repeat). Give the sync/micro/macro log ordering example, then explain starvation. Close with where \`await\` schedules, microtask, same turn as the resolving promise.`;

const MOCK_COURSE_ID = 42;

const MOCK_EXERCISE: Omit<ExerciseView, 'draft'> = {
  course_id: MOCK_COURSE_ID,
  classroom_session_id: null,
  study_session_id: null,
  title: 'Trace and tame a microtask storm',
  instructions:
    'Write a tiny script that logs a numbered tag for each callback so you can see the exact order the event loop runs things in.\n\n1. Log a `sync-start` tag.\n2. Schedule a `setTimeout(..., 0)` that logs a `macrotask` tag.\n3. Chain two `.then()` calls off a resolved promise, each logging a `microtask` tag.\n4. Log a `sync-end` tag.\n5. Run it and annotate which line ran in which "wave" (sync, microtask checkpoint, macrotask).',
  starter_code:
    "let n = 0;\nconst tag = (label) => console.log(`${n++} ${label}`);\n\ntag('sync-start');\nsetTimeout(() => tag('macrotask'), 0);\nPromise.resolve().then(() => tag('microtask-1')).then(() => tag('microtask-2 (nested)'));\ntag('sync-end');\n",
  deliverable:
    'A numbered console log with your own annotation (macrotask/microtask) next to each line, plus one sentence on where the nested .then() landed.',
  hints: [
    'Run the sync lines first on paper, what fires before any callback gets a chance to run at all?',
    'Both microtask callbacks are on the SAME queue as any other promise reaction, they drain completely before the timer fires.',
    'The second .then() is only enqueued once the first one runs, so it lands in a later microtask checkpoint, not the same one.',
  ],
  completed: false,
  reflection: '',
};

let mockExerciseDraft: string | null = null;
let mockExerciseCompleted = false;
let mockExerciseReflection = '';

// Session-only chat: in-memory per course id, cleared the same way the real
// backend clears it (completion, skip, extension, new session).
const mockChatThreads = new Map<number, ChatMessage[]>();
const mockClassroomChatThreads = new Map<number, ChatMessage[]>();
const mockClassroomExerciseDrafts = new Map<number, string>();
const mockClassroomExerciseCompletions = new Map<
  number,
  { completed: boolean; reflection: string }
>();
function clearMockChatThreads() {
  mockChatThreads.clear();
  mockClassroomChatThreads.clear();
}

const params =
  typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams();
const jump = params.get('step');
const skippedPreview = params.has('skipped');

let setupCompleted = false;
let mockPaused = params.has('paused');
let mockKioskLevel = (params.get('kiosk') ?? 'hard') as AppStateView['kiosk_level'];
let mockModel = (params.get('model') ?? 'opus') as AppStateView['model'];
let mockAgent = (params.get('agent') ?? 'claude') as AppStateView['agent'];
let mockCustomBin = '';
let mockSelectedFocus: FocusArea = 'javascript';
let mockDeepseekKeyConfigured = params.has('deepseekKey');
let mockActiveLanguage: LanguageLessonView | null = null;
let mockLanguageSessionId = 900;
const mockLanguageSettings: Record<
  LanguageId,
  {
    enabled: boolean;
    startLevel: CefrLevel;
    currentLevel: CefrLevel;
    targetLevel: CefrLevel;
    weeklyMinutes: number;
    sessionMinutes: number;
  }
> = {
  german: {
    enabled: params.get('program') === 'german',
    startLevel: 'A1',
    currentLevel: 'A1',
    targetLevel: 'A2',
    weeklyMinutes: 210,
    sessionMinutes: 30,
  },
  italian: {
    enabled: params.get('program') === 'italian',
    startLevel: 'A1',
    currentLevel: 'A1',
    targetLevel: 'A2',
    weeklyMinutes: 210,
    sessionMinutes: 30,
  },
};
const CLASSROOM_CATALOG = COURSES.map((course) => ({
  id: course.id, kind: course.kind, label: course.label,
  native: course.native_label, short: course.short_code,
}));

const requestedClass = (params.get('class') ?? params.get('program')) as
  | ClassroomSubjectId
  | null;
const mockClassroomSettings = Object.fromEntries(
  CLASSROOM_CATALOG.map((item) => [
    item.id,
    {
      enabled:
        requestedClass === item.id ||
        (item.kind === 'language' && mockLanguageSettings[item.id as LanguageId].enabled),
      agent: mockAgent,
      model: mockModel,
      customBin: '',
      sessionMinutes: 30,
      learningGoal: '',
      targetWeeklyMinutes: 0,
      focusPolicy: 'advisory' as FocusPolicy,
    },
  ]),
) as Record<
  ClassroomSubjectId,
  {
    enabled: boolean;
    agent: AppStateView['agent'];
    model: AppStateView['model'];
    customBin: string;
    sessionMinutes: number;
    learningGoal: string;
    targetWeeklyMinutes: number;
    focusPolicy: FocusPolicy;
  }
>;
let mockClassroomSlots: ClassroomSlotView[] = requestedClass
  ? [
      {
        id: 801,
        subject_id: requestedClass,
        label:
          CLASSROOM_CATALOG.find((item) => item.id === requestedClass)?.label ?? requestedClass,
        short_code:
          CLASSROOM_CATALOG.find((item) => item.id === requestedClass)?.short ?? 'CL',
        kind:
          CLASSROOM_CATALOG.find((item) => item.id === requestedClass)?.kind ?? 'engineering',
        hour: 7,
        minute: 30,
        weekdays: [1, 2, 3, 4, 5, 6], durations: {}, starts: {},
        enabled: true,
        owed: params.has('classDue') || params.has('languageDue'),
        next_fire_at: new Date(Date.now() + 4 * 60 * 60 * 1000).toISOString().slice(0, 19),
        in_progress: false,
        source: 'manual',
      occurrence_id: null,
      disposition: null,
      },
    ]
  : [];
let mockActiveEngineering: EngineeringLessonView | null = null;
let mockEngineeringSessionId = 1000;

function addDays(days: number) {
  const date = new Date();
  date.setDate(date.getDate() + days);
  return date.toISOString().slice(0, 10);
}

function mockLanguageProgram(language: LanguageId): LanguageProgramView {
  const settings = mockLanguageSettings[language];
  const isGerman = language === 'german';
  return {
    language,
    label: isGerman ? 'German' : 'Italian',
    native_label: isGerman ? 'Deutsch' : 'Italiano',
    enabled: settings.enabled,
    start_level: settings.startLevel,
    current_level: settings.currentLevel,
    target_level: settings.targetLevel,
    start_date: new Date().toISOString().slice(0, 10),
    target_date: addDays(settings.targetLevel === 'A1' ? 30 : 90),
    weekly_minutes: settings.weeklyMinutes,
    session_minutes: settings.sessionMinutes,
    completed_steps: settings.enabled ? 7 : 0,
    required_steps: 30,
    progress: settings.enabled ? 7 / 30 : 0,
    total_completed_steps: settings.enabled ? 7 : 0,
    total_required_steps: settings.targetLevel === 'A1' ? 30 : 90,
    recommended_weekly_minutes: isGerman ? 700 : 420,
    pace_status: 'commitment_gap',
    pace_message:
      `The ${settings.targetLevel} timeline needs more total weekly practice than the current commitment. App sessions are only one part.`,
    milestones: [
      { level: 'A1', target_date: addDays(30), reached: false },
      { level: 'A2', target_date: addDays(90), reached: false },
      { level: 'B1', target_date: addDays(270), reached: false },
      { level: 'B2', target_date: addDays(540), reached: false },
    ],
    skills: [
      { id: 'listening', label: 'Listening', score: 0.62, encounters: 4 },
      { id: 'reading', label: 'Reading', score: 0.74, encounters: 5 },
      { id: 'spoken_interaction', label: 'Spoken interaction', score: 0.48, encounters: 2 },
      { id: 'spoken_production', label: 'Spoken production', score: 0.51, encounters: 2 },
      { id: 'writing', label: 'Writing', score: 0.58, encounters: 3 },
      { id: 'grammar', label: 'Grammar', score: 0.68, encounters: 6 },
      {
        id: 'vocabulary_pragmatics',
        label: 'Vocabulary pragmatics',
        score: 0.71,
        encounters: 7,
      },
    ],
    official_sources: isGerman
      ? [
          {
            title: 'Goethe-Institut — German examinations',
            url: 'https://www.goethe.de/en/spr/prf.html',
          },
        ]
      : [
          {
            title: 'Centro CILS',
            url: 'https://cils.unistrasi.it/',
          },
        ],
  };
}

function mockLanguageLesson(language: LanguageId): LanguageLessonView {
  const german = language === 'german';
  const target = german ? 'Deutsch' : 'Italiano';
  return {
    session_id: String(mockLanguageSessionId),
    runtime: 'legacy',
    lifecycle: 'in_progress',
    revision: 0,
    checkpoint: null,
    check: null,
    outcome: null,
    language,
    label: german ? 'German' : 'Italian',
    native_label: target,
    level: 'A1',
    unit_slug: german ? 'de-a1-greetings-introductions' : 'it-a1-greetings',
    phase: 1,
    phase_requirement: null,
    phase_label: 'notice and understand',
    title: german ? 'Greetings and Introductions' : 'Saluti e presentazioni',
    scenario: german
      ? 'You arrive at a language course in Berlin and meet the teacher.'
      : 'You arrive at an Italian course in Rome and meet the teacher.',
    can_do: german
      ? 'I can greet people, introduce myself, and ask basic personal questions.'
      : 'I can greet people, introduce myself, and ask how someone is.',
    markdown: german
      ? `## Mission

You arrive at a language course in Berlin. Your goal is not to recite a vocabulary list: it is to complete a first meeting politely.

## Model dialogue

- **Lehrerin:** Guten Morgen! Wie heißen Sie?
  _Good morning! What is your name?_
- **Lernende:** Ich heiße Maria Santos.
  _My name is Maria Santos._
- **Lehrerin:** Woher kommen Sie?
  _Where are you from?_

## Grammar in service of the task

Use **ich heiße** for your name and **ich komme aus** for your origin. German keeps the verb in second position in these statements.

## Pragmatics

Use **Sie** with an unfamiliar adult until invited to use **du**. The distinction is social, not merely grammatical.`
      : `## Mission

You arrive at a language course in Rome. Complete a polite first meeting instead of translating isolated words.

## Model dialogue

- **Insegnante:** Buongiorno! Come si chiama?
  _Good morning! What is your name?_
- **Studente:** Mi chiamo Marco. Piacere.
  _My name is Marco. Nice to meet you._
- **Insegnante:** Di dove è?
  _Where are you from?_

## Grammar in service of the task

Use **mi chiamo** to introduce your name. Subject pronouns are often omitted because the verb ending carries the person.

## Pragmatics

Use **Lei** and the third-person verb with unfamiliar adults; **tu** belongs with peers and friends.`,
    phrases: german
      ? [
          { target: 'Wie heißen Sie?', translation: 'What is your name?', note: 'Formal' },
          { target: 'Ich komme aus …', translation: 'I am from …', note: 'Origin' },
        ]
      : [
          { target: 'Come si chiama?', translation: 'What is your name?', note: 'Formal' },
          { target: 'Mi chiamo …', translation: 'My name is …', note: 'Introduction' },
        ],
    dialogue: german
      ? [
          { speaker: 'Lehrerin', target: 'Guten Morgen! Wie heißen Sie?', translation: 'Good morning! What is your name?' },
          { speaker: 'Lernende', target: 'Ich heiße Maria Santos.', translation: 'My name is Maria Santos.' },
        ]
      : [
          { speaker: 'Insegnante', target: 'Buongiorno! Come si chiama?', translation: 'Good morning! What is your name?' },
          { speaker: 'Studente', target: 'Mi chiamo Marco. Piacere.', translation: 'My name is Marco. Nice to meet you.' },
        ],
    questions: [
      {
        id: 1,
        prompt: german ? 'What does “Guten Morgen” mean?' : 'What does “Buongiorno” mean?',
        choices: ['Good morning', 'Good night', 'Please', 'Thank you'],
        strand: 'vocabulary_pragmatics',
      },
      {
        id: 2,
        prompt: german ? 'Complete: Ich ___ Maria.' : 'Complete: Mi ___ Maria.',
        choices: german ? ['heiße', 'komme', 'bin aus', 'habe'] : ['chiamo', 'sono di', 'ho', 'stai'],
        strand: 'grammar',
      },
      {
        id: 3,
        prompt: german ? 'Which form is polite with a stranger?' : 'Which form is polite with a stranger?',
        choices: german ? ['Wie heißen Sie?', 'Wie heißt du?', 'Tschüss du!', 'Wo wohnst du?'] : ['Come si chiama?', 'Come ti chiami?', 'Ciao tu!', 'Dove abiti tu?'],
        strand: 'spoken_interaction',
      },
      {
        id: 4,
        prompt: german ? 'How do you say “I am from …”?' : 'How do you say “My name is …”?',
        choices: german ? ['Ich komme aus …', 'Ich heiße aus …', 'Ich habe aus …', 'Ich bin Name …'] : ['Mi chiamo …', 'Sono chiamo …', 'Ho nome …', 'Io chiamare …'],
        strand: 'spoken_production',
      },
      {
        id: 5,
        prompt: german ? 'What social distinction do Sie and du express?' : 'What social distinction do Lei and tu express?',
        choices: ['Formality and relationship', 'Past and present', 'Singular and plural only', 'Written and spoken language'],
        strand: 'vocabulary_pragmatics',
      },
    ],
    speaking_prompt: german
      ? 'Introduce yourself with your name, origin, and a polite closing.'
      : 'Introduce yourself with your name, origin, and a polite closing.',
    writing_prompt: german
      ? 'Write six short sentences introducing yourself to a course teacher.'
      : 'Write six short sentences introducing yourself to a course teacher.',
    listen_text: german
      ? 'Guten Morgen! Wie heißen Sie? Ich heiße Maria Santos. Woher kommen Sie?'
      : 'Buongiorno! Come si chiama? Mi chiamo Marco. Piacere. Di dove è?',
    speech_locale: german ? 'de-DE' : 'it-IT',
    estimated_minutes: 30,
    status: 'in_progress',
  };
}

const restoredPaths = new Map<ClassroomSubjectId, string>();
function applyPreviewPath(path: AcceptedPath | null) {
  if (!path) return;
  const id = path.recommendation.course.course_id;
  if (restoredPaths.get(id) === path.reference.path_revision_id) return;
  const { tutor, pace, goal } = path.configuration;
  const scheduled = mockClassroomSlots.some(slot => slot.subject_id === id && slot.enabled);
  Object.assign(mockClassroomSettings[id], { enabled: scheduled, agent: tutor.provider, model: tutor.model, customBin: tutor.custom_agent_bin ?? '', sessionMinutes: pace.session_minutes, learningGoal: goal.note, targetWeeklyMinutes: pace.weekly_minutes ?? mockClassroomSettings[id].targetWeeklyMinutes });
  if ((id === 'german' || id === 'italian') && goal.kind === 'language_level') {
    const settings = mockLanguageSettings[id];
    Object.assign(settings, { enabled: scheduled, currentLevel: path.recommendation.entry_point, targetLevel: goal.target_level, sessionMinutes: pace.session_minutes, weeklyMinutes: pace.weekly_minutes ?? settings.weeklyMinutes });
  }
  restoredPaths.set(id, path.reference.path_revision_id);
}
/** Preview counterpart of the native route summary: same denominators, no credit. */
function previewRoute(subjectId: ClassroomSubjectId, acceptedPath: AcceptedPath | null): RouteSummary | null {
  const plan = acceptedPath?.recommendation;
  if (!acceptedPath || !plan) return null;
  const listed = (list: { id: string }[] | undefined, slug: string) => !!list?.some((topic) => topic.id === slug);
  const concepts = seedConcepts.filter((concept) => concept.focus === subjectId);
  const setAside = (slug: string) => listed(plan.earlier_topics, slug) || listed(plan.bypassed, slug) || listed(plan.checked, slug);
  const core = concepts.filter((concept) => concept.curriculum.core);
  const order = ['foundations', 'mechanisms', 'production', 'synthesis', 'elective'];
  const bridge = plan.bridges?.[0];
  const candidate = bridge ? concepts.find((concept) => concept.slug === bridge.id) : [...core].sort((a, b) => order.indexOf(a.curriculum.phase) - order.indexOf(b.curriculum.phase) || a.tier - b.tier).find((concept) => !setAside(concept.slug));
  return {
    revision: acceptedPath.revision,
    entry_label: plan.entry_label,
    required_total: core.filter((concept) => !setAside(concept.slug)).length,
    required_done: 0,
    coverage_total: core.length,
    coverage_done: 0,
    demonstrated: (plan.checked?.length ?? 0) + plan.criteria.filter((row) => row.verdict === 'passed').length,
    needs_review: plan.refreshers.length + (plan.bridges?.length ?? 0),
    next: candidate ? { slug: candidate.slug, title: candidate.title, reason: bridge ? `Bridge lesson before ${bridge.reason.replace(/^Bridge before /, '').replace(/\.$/, '')}.` : 'Next required topic on your accepted route.' } : null,
    stale: false,
  };
}

function mockClassroomProgram(subjectId: ClassroomSubjectId): ClassroomProgramView {
  const acceptedPath = previewClassPath(subjectId);
  applyPreviewPath(acceptedPath);
  const catalog = CLASSROOM_CATALOG.find((item) => item.id === subjectId)!;
  const settings = mockClassroomSettings[subjectId];
  const languageProgress =
    catalog.kind === 'language' ? mockLanguageProgram(subjectId as LanguageId) : null;
  return {
    subject_id: subjectId,
    accepted_path: previewPathSummary(acceptedPath),
    kind: catalog.kind,
    label: catalog.label,
    native_label: catalog.native,
    short_code: catalog.short,
    enabled: settings.enabled && mockClassroomSlots.some(slot => slot.subject_id === subjectId && slot.enabled),
    agent: settings.agent,
    model: settings.model,
    custom_agent_bin: settings.customBin,
    prompt_profile: `classroom.${subjectId}`,
    prompt_version: 'v1',
    session_minutes: settings.sessionMinutes,
    learning_goal: settings.learningGoal,
    focus_policy: settings.focusPolicy,
    route: previewRoute(subjectId, acceptedPath),
    review_due: 0,
    target_weekly_minutes: settings.targetWeeklyMinutes,
    progress: languageProgress?.progress ?? 0,
    progress_label:
      languageProgress
        ? `${languageProgress.completed_steps} / ${languageProgress.required_steps} evidence steps in ${languageProgress.current_level}`
        : `0 / ${seedConcepts.filter((concept) => concept.focus === subjectId && concept.curriculum.core).length} core concepts practiced`,
    completed: false,
    language_progress: languageProgress,
  };
}

function mockEngineeringLesson(subjectId: FocusArea): EngineeringLessonView {
  const catalog = CLASSROOM_CATALOG.find((item) => item.id === subjectId)!;
  const reference = referenceFor(subjectId);
  const shaped = params.has('shaped') ? Object.values(shapedLessons)[0] : undefined;
  const concept = seedConcepts.find((concept) => concept.slug === reference.slug)!;
  return {
    session_id: String(mockEngineeringSessionId),
    runtime: 'legacy',
    lifecycle: 'in_progress',
    revision: 0,
    checkpoint: null,
    check: null,
    outcome: null,
    subject_id: subjectId,
    label: catalog.label,
    short_code: catalog.short,
    title: shaped ? 'CAP in practice: choosing partition behaviour, not a permanent label' : reference.title,
    concept_slug: reference.slug,
    concept_title: concept.title,
    category: concept.category,
    curriculum: concept.curriculum as CurriculumBrief,
    prerequisites: concept.prereqs,
    session_index: 1,
    why_now: 'Browser preview: a bundled reference lesson from this course. Native selection follows curriculum eligibility.',
    markdown: shaped ?? reference.markdown,
    resources: reference.resources,
    review_notes: [],
    level: params.has('beginner') ? 'beginner' : 'standard',
    research_note: params.has('unverified') ? 'No primary documentation could be retrieved when this lesson was written: www.gnu.org could not be reached. Its claims were not checked against the sources. Read it with care and verify anything you will rely on.' : null,
    questions: reference.questions.filter((question) => question.kind === 'mcq').map((question, index) => ({
      id: index + 1, prompt: question.prompt, choices: question.choices!,
      section: question.section, learning_objective: question.learning_objective,
    })),
    exercise: shaped ? SHAPED_EXERCISE : reference.exercise,
    kind: 'lesson' as const,
    fresh_sample: true,
    agent_used: mockClassroomSettings[subjectId].agent,
    prompt_profile: `classroom.${subjectId}`,
    prompt_version: `classroom.${subjectId}.v1`,
    estimated_minutes: mockClassroomSettings[subjectId].sessionMinutes,
    plan: sessionPlan(mockClassroomSettings[subjectId].sessionMinutes),
    status: 'in_progress',
  };
}

/** The desk's split of a session: under half to read, then practise, then check. */
function sessionPlan(minutes: number): SessionPlan {
  const depth = Math.min(60, Math.max(30, minutes));
  const learn_minutes = Math.floor((depth * 47) / 100);
  const check_minutes = Math.floor((depth * 20) / 100);
  return { learn_minutes, practice_minutes: depth - learn_minutes - check_minutes, check_minutes };
}

function appState(): AppStateView {
  const inSetup = typeof location !== 'undefined' && location.search.includes('setup') && !setupCompleted;
  return {
    onboarded: !inSetup,
    selected_focus: mockSelectedFocus,
    // Recurring obligations now belong to classes; no separate daily trigger.
    schedule_hour: 19,
    schedule_minute: 0,
    debug_day: true,
    enforcement_disarmed: params.has('disarmed'),
    schedule_paused: mockPaused,
    kiosk_level: mockKioskLevel,
    model: mockModel,
    agent: mockAgent,
    custom_agent_bin: mockCustomBin,
    deepseek_key_configured: mockDeepseekKeyConfigured,
    alarm: null,
    blocks: [],
    classroom_programs: CLASSROOM_CATALOG.map((item) => mockClassroomProgram(item.id)),
    classroom_slots: mockClassroomSlots,
    classroom_due_count: mockClassroomSlots.filter((slot) => slot.owed).length,
    appointments: [],
    focus: null,
    active_classroom_sessions: [
      ...(mockActiveLanguage
        ? [
            {
              session_id: String(mockActiveLanguage.session_id),
              subject_id: mockActiveLanguage.language,
              kind: 'language' as const,
              label: mockActiveLanguage.label,
              title: mockActiveLanguage.title,
              runtime: 'legacy' as const,
              lifecycle: 'in_progress',
            },
          ]
        : []),
      ...(mockActiveEngineering
        ? [
            {
              session_id: mockActiveEngineering.session_id,
              subject_id: mockActiveEngineering.subject_id,
              kind: 'engineering' as const,
              label: mockActiveEngineering.label,
              title: mockActiveEngineering.title,
              runtime: 'legacy' as const,
              lifecycle: 'in_progress',
            },
          ]
        : []),
    ],
  };
}

let previewFreeOnly = true;

function mockPrograms(): ClassroomProgramView[] {
  return CLASSROOM_CATALOG.map((item) => mockClassroomProgram(item.id as ClassroomSubjectId));
}
// A class published in the preview joins the catalogue and gets a program.
onPreviewTell(() => mockEmit('classroom:state', { refresh: true }));
onPreviewPublish((course) => {
  if (CLASSROOM_CATALOG.some((item) => item.id === course.id)) return;
  CLASSROOM_CATALOG.push({ id: course.id, kind: course.kind, label: course.label, native: course.native_label, short: course.short_code });
  mockClassroomSettings[course.id] = { enabled: false, agent: mockAgent, model: mockModel, customBin: '', sessionMinutes: 30, learningGoal: '', targetWeeklyMinutes: 0, focusPolicy: 'advisory' as FocusPolicy };
});
function rejectMockConflicts(candidates: ScheduleCandidate[], scope: ConflictScope = {}) {
  const conflicts = scheduleConflicts(candidates, mockClassroomSlots, mockPrograms(), scope);
  if (conflicts.length) throw new Error(conflictMessage(conflicts));
}
/** The class's session length follows its study times: the most common day length, the shorter on a tie. */
function followMockScheduleLength(subjectId: ClassroomSubjectId) {
  const tally = new Map<number, number>();
  for (const slot of mockClassroomSlots.filter((slot) => slot.subject_id === subjectId && slot.enabled)) {
    for (const day of slot.weekdays) {
      const minutes = slot.durations[String(day)] ?? mockClassroomSettings[subjectId].sessionMinutes;
      tally.set(minutes, (tally.get(minutes) ?? 0) + 1);
    }
  }
  const usual = [...tally.entries()].sort((a, b) => b[1] - a[1] || a[0] - b[0])[0];
  if (usual) mockClassroomSettings[subjectId].sessionMinutes = usual[0];
}
function pauseMockClassWithoutSchedule(subjectId: ClassroomSubjectId) {
  if (mockClassroomSlots.some(slot => slot.subject_id === subjectId && slot.enabled)) return;
  mockClassroomSettings[subjectId].enabled = false;
  if (subjectId === 'german' || subjectId === 'italian') mockLanguageSettings[subjectId].enabled = false;
}

/** Practice questions in the preview, per class; the checks' bank is described, never shown. */
const mockPractice = new Map<string, BankQuestion[]>();
function previewPractice(courseId: ClassroomSubjectId): PracticeBankView {
  const questions = mockPractice.get(courseId) ?? [];
  const stages = courseDefinition(courseId)?.entry_points ?? [];
  const custom = courseId.startsWith('custom-');
  const checks = custom ? previewCustom.bankOf(courseId).filter((q) => !q.voided).length : 14;
  return { course_id: courseId, questions: questions.map((q) => ({ ...q })), per_stage: stages.map((s) => ({ stage: s.id, label: s.label, count: questions.filter((q) => !q.voided && q.entry_point === s.id).length })), usable: questions.filter((q) => !q.voided).length, written_at: questions.length ? new Date().toISOString() : null, checks_bank: { source: custom ? (checks ? 'written' : 'none') : 'bundled', questions: checks }, working: false };
}

/** The Logs page in the preview: a finished lesson and a curriculum being drafted right now. */
const PREVIEW_RUN_STARTED = Date.now() - 154_000;
function previewRuns(): ExecutionRun[] {
  const started = new Date(PREVIEW_RUN_STARTED).toISOString();
  return [
    { run_id: 'draft:custom-religious-latin', course_id: 'custom-religious-latin', label: 'Religious Latin', activity: 'draft', started_at: started, last_at: new Date(PREVIEW_RUN_STARTED + 90_000).toISOString(), lines: 4, outcome: 'running', error: null },
    { run_id: 'study-typescript-1', course_id: 'typescript', label: 'TypeScript', activity: 'lesson', started_at: new Date(PREVIEW_RUN_STARTED - 3_600_000).toISOString(), last_at: new Date(PREVIEW_RUN_STARTED - 3_318_000).toISOString(), lines: 3, outcome: 'ready', error: null },
  ];
}
function previewLogLines(): ExecutionLogLine[] {
  const at = (offset: number) => new Date(PREVIEW_RUN_STARTED + offset).toISOString();
  return [
    { id: 1, at: at(-3_600_000), run_id: 'study-typescript-1', course_id: 'typescript', line: 'Claude Code · opus · lesson' },
    { id: 2, at: at(-3_540_000), run_id: 'study-typescript-1', course_id: 'typescript', line: 'research: retrieved 4 primary source(s): www.typescriptlang.org' },
    { id: 3, at: at(-3_318_000), run_id: 'study-typescript-1', course_id: 'typescript', line: 'Claude Code · finished in 282.0s' },
    { id: 4, at: at(0), run_id: 'draft:custom-religious-latin', course_id: 'custom-religious-latin', line: 'class builder: asking claude (opus) to draft "Religious Latin"' },
    { id: 5, at: at(200), run_id: 'draft:custom-religious-latin', course_id: 'custom-religious-latin', line: 'Claude Code · opus · course-draft' },
    { id: 6, at: at(45_000), run_id: 'draft:custom-religious-latin', course_id: 'custom-religious-latin', line: 'class builder: still drafting (45s); a whole course takes a few minutes' },
    { id: 7, at: at(90_000), run_id: 'draft:custom-religious-latin', course_id: 'custom-religious-latin', line: 'class builder: still drafting (90s); a whole course takes a few minutes' },
  ];
}

export const mockApi = {
  getEnrollmentOptions: async (courseId: ClassroomSubjectId) => {
    const options = previewEnrollmentOptions(courseId);
    options.default_configuration.tutor = { provider: mockAgent, model: mockModel, custom_agent_bin: mockCustomBin || null };
    const path = previewClassPath(courseId);
    if (path) options.default_configuration = structuredClone(path.configuration);
    return options;
  },
  getClassPath: async (courseId: ClassroomSubjectId) => previewClassPath(courseId),
  getUnitChallenge: getPreviewChallenge,
  startUnitChallenge: startPreviewChallenge,
  saveUnitChallengeResponse: savePreviewChallengeResponse,
  submitUnitChallengeRound: submitPreviewChallenge,
  applyUnitChallenge: applyPreviewChallenge,
  reviseClassPath: async (input: RevisePath) => revisePreviewClassPath(input, (slug) => seedConcepts.find((concept) => concept.slug === slug && concept.focus === input.course_id)?.title),
  acceptClassPath: async (input: AcceptPath) => {
    const path = await acceptPreviewClassPath(input);
    applyPreviewPath(previewClassPath(path.recommendation.course.course_id));
    mockEmit('classroom:state', previewPathSummary(path));
    return path;
  },
  getPlacementCheck: getPreviewPlacement,
  startPlacementCheck: startPreviewPlacement,
  savePlacementResponse: savePreviewPlacement,
  submitPlacementRound: submitPreviewPlacement,
  continuePlacementCheck: continuePreviewPlacement,
  finishPlacementCheck: finishPreviewPlacement,
  getPathRecommendation: recommendPreviewPath,
  getEnrollmentDraft: async (courseId: ClassroomSubjectId) => previewEnrollmentDraft(courseId),
  saveEnrollmentDraft: async (input: SaveEnrollmentDraft) => savePreviewEnrollmentDraft(input),
  getCatalog: async () => [...COURSES, ...previewCustom.published()],
  listCustomCourses: previewCustom.list,
  getCustomCourse: previewCustom.get,
  createCustomCourse: previewCustom.create,
  saveCustomCourseBrief: previewCustom.saveBrief,
  saveCustomCourseDraft: previewCustom.saveDraft,
  draftCustomCourse: previewCustom.draft,
  reviewCustomCourse: previewCustom.review,
  verifyCustomCourseSources: previewCustom.verify,
  writeCustomCourseBank: previewCustom.writeBank,
  voidCustomQuestion: previewCustom.voidQuestion,
  getPracticeBank: async (courseId: ClassroomSubjectId) => previewPractice(courseId),
  writePracticeQuestions: async (courseId: ClassroomSubjectId, stage: string) => {
    await new Promise((r) => setTimeout(r, 1500));
    const held = mockPractice.get(courseId) ?? [];
    const batch = Date.now();
    const fresh: BankQuestion[] = Array.from({ length: 8 }, (_, i) => ({ id: `${courseId}-practice-${batch}-${i + 1}`, criterion: `${courseId}-practice-${batch}-${i + 1}`, competency: '', entry_point: stage, label: `Practice ${held.length + i + 1}`, prompt: `Practice question ${held.length + i + 1} for the ${stage} stage: which statement holds?`, choices: [{ id: '1', text: 'A plausible mistake' }, { id: '2', text: 'The mechanism as the source describes it' }, { id: '3', text: 'A common misconception' }, { id: '4', text: 'An unrelated fact' }], answer: '2', explanation: 'The source names the mechanism; the first and third are the misconceptions the lesson warns about, the fourth is unrelated.', followup_for: null, source: 'https://docs.example.org/' }));
    mockPractice.set(courseId, [...held, ...fresh]);
    return previewPractice(courseId);
  },
  voidPracticeQuestion: async (courseId: ClassroomSubjectId, questionId: string, reason: string) => { const q = (mockPractice.get(courseId) ?? []).find((q) => q.id === questionId); if (q) { q.voided = true; q.void_reason = reason; } return previewPractice(courseId); },
  clearPracticeBank: async (courseId: ClassroomSubjectId) => { mockPractice.delete(courseId); return previewPractice(courseId); },
  fixCustomCourseFinding: previewCustom.fixFinding,
  fixAllCustomCourseFindings: previewCustom.fixAll,
  resolveCustomCourseFinding: previewCustom.resolveFinding,
  acceptCustomCourseSource: previewCustom.acceptSource,
  replaceCustomCourseSource: previewCustom.replaceSource,
  replaceAllCustomCourseSources: previewCustom.replaceAllSources,
  markCustomCourseRead: previewCustom.markRead,
  publishCustomCourse: previewCustom.publish,
  deleteCustomCourseDraft: previewCustom.remove,
  exportCustomCourse: previewCustom.export,
  importCustomCourse: previewCustom.import,
  getAppState: async () => appState(),
  checkAgent: async () => false,
  getRunnerConfiguration: async (runner: string) => previewConfiguration(runner),
  saveRunnerConfiguration: async (configuration: import('./contracts/agents').RunnerConfiguration) => savePreviewConfiguration(configuration),
  listAgentRunners: async () => previewRunners(),
  getRunnerModels: async (runner: string, _refresh = false) => previewModels(runner),
  setRunnerKey: desktopRequired,
  getStudyPulse: async (): Promise<StudyPulse> => mockPulse(),
  recoveryCommand: async (line: string): Promise<RecoveryReply> => {
    const verb = line.trim().toLowerCase().split(/\s+/)[0];
    if (verb === 'help') return { lines: ['Principia recovery console. Four commands, in order, end an enforced session:', '  1. unlock', '     Explains what will happen and issues a six-character challenge code.', '  2. confirm <code>', '     Type the code back. It proves a person is at the keyboard.', '  3. phrase <your escape phrase>', '     The break-glass phrase you set during setup.', '  4. release', '     Pauses the session with its work intact, breaks the streak, and frees the machine.', '', 'Also: status · cancel · close · help'], released: false, close: false };
    if (verb === 'status') return { lines: ['desk: not locked', 'focused session: none', 'sequence: not started (type: unlock)'], released: false, close: false };
    if (verb === 'unlock') return { lines: ['desk: not locked', '', 'Releasing pauses the session with its work intact and breaks your streak.', 'Challenge code: K7PM2X', 'To continue, type:  confirm K7PM2X'], released: false, close: false };
    if (verb === 'confirm') return { lines: ['Confirmed.', 'Now type:  phrase <your escape phrase>'], released: false, close: false };
    if (verb === 'phrase') return { lines: ['Phrase accepted.', 'Type:  release'], released: false, close: false };
    if (verb === 'release') return { lines: ['Released. The lock is down.', 'This console closes in a moment.'], released: true, close: false };
    if (verb === 'close' || verb === 'exit' || verb === 'quit') return { lines: [], released: false, close: true };
    return { lines: [`Unknown command: ${verb}. Type help.`], released: false, close: false };
  },
  recoveryStatus: async (): Promise<RecoveryStatus> => guessStatus(true),
  openRecoveryConsole: async () => { if (typeof window !== 'undefined') window.open('/recovery', 'principia-recovery', 'width=720,height=460'); },
  closeRecoveryConsole: async () => { if (typeof window !== 'undefined' && window.name === 'principia-recovery') window.close(); },
  getSearchSettings: async (): Promise<SearchSettingsView> => mockSearch(),
  setSearchSettings: async (provider: SearchProvider, searxngUrl: string): Promise<SearchSettingsView> => { mockSearchProvider = provider; mockSearxngUrl = searxngUrl || 'http://127.0.0.1:8899'; return mockSearch(); },
  setSearchKey: async (provider: SearchProvider, value: string): Promise<SearchSettingsView> => { if (provider === 'brave') mockBraveKey = !!value.trim(); if (provider === 'tavily') mockTavilyKey = !!value.trim(); return mockSearch(); },
  testSearch: async (query: string): Promise<SearchResult[]> => [
    { title: 'Redirections (Bash Reference Manual)', url: 'https://www.gnu.org/software/bash/manual/html_node/Redirections.html', snippet: `Preview result for “${query}”: before a command is executed, its input and output may be redirected using a special notation interpreted by the shell.`, engine: 'preview' },
    { title: 'bash(1) — Linux manual page', url: 'https://man7.org/linux/man-pages/man1/bash.1.html', snippet: 'REDIRECTION: Before a command is executed, its input and output may be redirected.', engine: 'preview' },
  ],
  getLocalModels: async () => previewLocal(),
  getLocalPulls: async () => [],
  installLocalRunner: desktopRequired,
  startLocalRunner: desktopRequired,
  pullLocalModel: desktopRequired,
  removeLocalModel: desktopRequired,
  selectRunner: async (agent: string, model: string, customBin: string) => {
    rememberPreviewModel(mockAgent, mockModel);
    mockAgent = agent as AppStateView['agent']; mockModel = model; mockCustomBin = customBin;
    rememberPreviewModel(agent, model);
  },
  getOpenrouterFreeOnly: async () => previewFreeOnly,
  setOpenrouterFreeOnly: async (value: boolean) => { previewFreeOnly = value; },
  testAgentConnection: async (agent = 'claude', _customBin = '', model = 'default') => ({ runner: previewRunners().find(r => r.provider === agent || r.id === agent)?.id ?? 'custom-cli' as RunnerId, model, ok: false, detail: 'Connection tests run in the desktop app. This browser preview does not contact providers.', duration_ms: 0 }),
  getAgentActivity: async () => [],
  getAgentPolicy: async (): Promise<AgentPolicy> => JSON.parse(localStorage.getItem('principia.preview.agent-policy') ?? '{"fallback_agent":null,"fallback_model":"default","monthly_budget_usd":0}'),
  setAgentPolicy: async (policy: AgentPolicy) => { localStorage.setItem('principia.preview.agent-policy', JSON.stringify(policy)); },
  completeSetup: async () => {
    setupCompleted = true;
    return appState();
  },
  getCurriculumMap: async (focus: FocusArea): Promise<CurriculumMapView> => {
    const path = previewClassPath(focus as ClassroomSubjectId);
    const plan = path?.recommendation;
    const listed = (list: { id: string }[] | undefined, slug: string) => !!list?.some((topic) => topic.id === slug);
    const statusFor = (slug: string): CurriculumConceptView['path_status'] =>
      !plan ? 'upcoming'
      : listed(plan.bridges, slug) ? 'bridge'
      : listed(plan.checked, slug) ? 'prior_knowledge_checked'
      : listed(plan.bypassed, slug) ? 'bypassed_by_choice'
      : listed(plan.refreshers, slug) ? 'needs_refresher'
      : listed(plan.earlier_topics, slug) ? (plan.earlier_topics.find((t) => t.id === slug)?.reason.startsWith('Declared familiar') ? 'bypassed_by_choice' : 'not_assessed')
      : 'upcoming';
    const concepts = seedConcepts.filter((concept) => concept.focus === focus).map((concept): CurriculumConceptView => ({
      id: seedConcepts.indexOf(concept) + 1,
      slug: concept.slug,
      title: concept.title,
      category: concept.category,
      tier: concept.tier,
      phase: concept.curriculum.phase as CurriculumMapView['current_phase'],
      core: concept.curriculum.core,
      prerequisites: concept.prereqs,
      mastery_state: 'unseen',
      times_picked: 0,
      last_picked_date: null,
      learner_outcome: concept.curriculum.learner_outcome,
      artifact: concept.curriculum.artifact,
      related_concepts: concept.curriculum.related_concepts,
      path_status: statusFor(concept.slug),
      required: concept.curriculum.core && ['upcoming', 'in_progress', 'bridge'].includes(statusFor(concept.slug)),
    }));
    const core = concepts.filter((concept) => concept.core);
    return {
      focus,
      label: CLASSROOM_CATALOG.find((item) => item.id === focus)?.label ?? focus,
      month_outcome: courseDefinition(focus)!.outcome,
      completed_sessions: 0,
      current_phase: 'foundations',
      concepts,
      path: path && plan ? {
        revision: path.revision,
        entry_label: plan.entry_label,
        required_total: core.filter((concept) => concept.required).length,
        required_done: 0,
        coverage_total: core.length,
        coverage_done: 0,
        bypassed: concepts.filter((concept) => concept.path_status === 'bypassed_by_choice').length,
        checked: concepts.filter((concept) => concept.path_status === 'prior_knowledge_checked').length,
        refreshers: concepts.filter((concept) => concept.path_status === 'needs_refresher').length,
        bridges: concepts.filter((concept) => concept.path_status === 'bridge').length,
      } : null,
      bridge_proposals: [],
      challenges_available: !String(focus).startsWith('custom-'),
    };
  },
  configureClassroomProgram: async (input: {
    subject_id: ClassroomSubjectId;
    enabled: boolean;
    agent: AppStateView['agent'];
    model: AppStateView['model'];
    custom_agent_bin: string;
    session_minutes: number;
    start_level?: CefrLevel | null;
    target_level?: CefrLevel | null;
    weekly_minutes?: number | null;
  }) => {
    const settings = mockClassroomSettings[input.subject_id];
    if (input.enabled && !mockClassroomSlots.some(slot => slot.subject_id === input.subject_id && slot.enabled)) throw new Error("Add a study time in this class's Schedule tab before activating it.");
    if (input.enabled && (!settings.enabled || input.session_minutes > settings.sessionMinutes)) {
      const own = mockClassroomSlots.filter((slot) => slot.subject_id === input.subject_id && slot.enabled);
      rejectMockConflicts(own.map((slot) => ({ subject_id: input.subject_id, hour: slot.hour, minute: slot.minute, weekdays: slot.weekdays, session_minutes: input.session_minutes })), { excludedSlotIds: own.map((slot) => slot.id) });
    }
    settings.enabled = input.enabled;
    settings.agent = input.agent;
    settings.model = input.model;
    settings.customBin = input.custom_agent_bin;
    settings.sessionMinutes = input.session_minutes;
    if (input.subject_id === 'german' || input.subject_id === 'italian') {
      const language = mockLanguageSettings[input.subject_id];
      language.enabled = input.enabled;
      language.sessionMinutes = input.session_minutes;
      if (input.start_level) language.startLevel = input.start_level;
      if (input.target_level) language.targetLevel = input.target_level;
      if (input.weekly_minutes) language.weeklyMinutes = input.weekly_minutes;
    }
    return mockClassroomProgram(input.subject_id);
  },
  upsertClassroomSlot: async (input: {
    id?: number | null;
    subject_id: ClassroomSubjectId;
    hour: number;
    minute: number;
    weekdays: number[];
    enabled: boolean;
    durations?: Record<string, number>;
    starts?: Record<string, string>;
  }) => {
    const id = input.id ?? Math.max(800, ...mockClassroomSlots.map((slot) => slot.id)) + 1;
    const catalog = CLASSROOM_CATALOG.find((item) => item.id === input.subject_id)!;
    const base = `${String(input.hour).padStart(2, '0')}:${String(input.minute).padStart(2, '0')}`;
    const settings = mockClassroomSettings[input.subject_id];
    // Every day the rule fires on gets its minutes written down, as the desk does.
    const durations = Object.fromEntries(input.weekdays.map((day) => [String(day), input.durations?.[String(day)] ?? settings.sessionMinutes]));
    const slot: ClassroomSlotView = {
      id,
      subject_id: input.subject_id,
      label: catalog.label,
      short_code: catalog.short,
      kind: catalog.kind,
      durations,
      starts: Object.fromEntries(Object.entries(input.starts ?? {}).filter(([day, time]) => input.weekdays.includes(Number(day)) && time !== base)),
      hour: input.hour,
      minute: input.minute,
      weekdays: input.weekdays,
      enabled: input.enabled,
      owed: false,
      in_progress: false,
      next_fire_at: new Date(Date.now() + 8 * 60 * 60 * 1000).toISOString().slice(0, 19),
      source: 'manual',
      occurrence_id: null,
      disposition: null,
    };
    const existing = mockClassroomSlots.findIndex((candidate) => candidate.id === id);
    if (input.id != null && (existing < 0 || mockClassroomSlots[existing].subject_id !== input.subject_id)) throw new Error('classroom slot was not found');
    if (input.enabled) rejectMockConflicts([{ subject_id: input.subject_id, hour: input.hour, minute: input.minute, weekdays: input.weekdays, session_minutes: settings.sessionMinutes, durations, starts: slot.starts }], { excludedSlotIds: input.id != null ? [input.id] : [] });
    if (existing >= 0) mockClassroomSlots[existing] = slot;
    else mockClassroomSlots = [...mockClassroomSlots, slot];
    pauseMockClassWithoutSchedule(input.subject_id);
    followMockScheduleLength(input.subject_id);
    return mockClassroomSlots;
  },
  planClassroomSchedule: async (input: {
    subject_id: ClassroomSubjectId;
    learning_goal: string;
    target_weekly_minutes: number;
    windows: {
      weekdays: number[];
      start_hour: number;
      start_minute: number;
      end_hour: number;
      end_minute: number;
    }[];
    commit: boolean;
  }): Promise<ClassroomPlanView> => {
    if (input.windows.length === 0) {
      throw new Error('add at least one availability window');
    }
    const catalog = CLASSROOM_CATALOG.find((item) => item.id === input.subject_id)!;
    const settings = mockClassroomSettings[input.subject_id];
    const slots: PlannedSlot[] = [];
    for (const window of input.windows) {
      const start = window.start_hour * 60 + window.start_minute;
      const end = window.end_hour * 60 + window.end_minute;
      if (start >= end) {
        throw new Error('availability window end time must be after its start time');
      }
      const weekdays = Array.from(new Set(window.weekdays)).sort((a, b) => a - b);
      if (weekdays.length === 0) {
        throw new Error('every availability window needs at least one valid weekday');
      }
      const existing = slots.find(
        (slot) => slot.hour === window.start_hour && slot.minute === window.start_minute,
      );
      if (existing) {
        existing.weekdays = Array.from(new Set([...existing.weekdays, ...weekdays])).sort(
          (a, b) => a - b,
        );
      } else {
        slots.push({ hour: window.start_hour, minute: window.start_minute, weekdays });
      }
    }
    slots.sort((a, b) => a.hour - b.hour || a.minute - b.minute);
    const totalWeeklyMinutes = slots.reduce(
      (sum, slot) => sum + slot.weekdays.length * settings.sessionMinutes,
      0,
    );
    const meetsTarget = input.target_weekly_minutes === 0 || totalWeeklyMinutes >= input.target_weekly_minutes;
    const conflicts = scheduleConflicts(slots.map((slot) => ({ subject_id: input.subject_id, hour: slot.hour, minute: slot.minute, weekdays: slot.weekdays, session_minutes: settings.sessionMinutes })), mockClassroomSlots, mockPrograms(), { excludePlannedFor: input.subject_id });
    if (input.commit && conflicts.length) throw new Error(conflictMessage(conflicts));
    if (!input.commit) {
      return {
        slots,
        total_weekly_minutes: totalWeeklyMinutes,
        target_weekly_minutes: input.target_weekly_minutes,
        meets_target: meetsTarget,
        conflicts,
        program: null,
        schedule: null,
      };
    }
    settings.learningGoal = input.learning_goal.trim();
    settings.targetWeeklyMinutes = input.target_weekly_minutes;
    if (input.subject_id === 'german' || input.subject_id === 'italian') {
      mockLanguageSettings[input.subject_id].weeklyMinutes = input.target_weekly_minutes;
    }
    mockClassroomSlots = [
      ...mockClassroomSlots.filter(
        (slot) => !(slot.subject_id === input.subject_id && slot.source === 'planned'),
      ),
      ...slots.map((slot, index) => ({
        id: 900 + index + Math.floor(Math.random() * 100),
        subject_id: input.subject_id,
        label: catalog.label,
        short_code: catalog.short,
        kind: catalog.kind,
        durations: {},
        starts: {},
        hour: slot.hour,
        minute: slot.minute,
        weekdays: slot.weekdays,
        enabled: true,
        owed: false,
        in_progress: false,
        next_fire_at: new Date(Date.now() + 8 * 60 * 60 * 1000).toISOString().slice(0, 19),
        source: 'planned' as const,
        occurrence_id: null,
        disposition: null,
      })),
    ];
    return {
      slots,
      total_weekly_minutes: totalWeeklyMinutes,
      target_weekly_minutes: input.target_weekly_minutes,
      meets_target: meetsTarget,
      conflicts,
      program: mockClassroomProgram(input.subject_id),
      schedule: mockClassroomSlots.filter((slot) => slot.subject_id === input.subject_id),
    };
  },
  deleteClassroomSlot: async (id: number) => {
    const removed = mockClassroomSlots.find(slot => slot.id === id);
    mockClassroomSlots = mockClassroomSlots.filter((slot) => slot.id !== id);
    if (removed) { pauseMockClassWithoutSchedule(removed.subject_id); followMockScheduleLength(removed.subject_id); }
    return mockClassroomSlots;
  },
  startClassroomSession: async (
    subjectId: ClassroomSubjectId,
  ): Promise<ClassroomSessionStart> => {
    if (!mockClassroomProgram(subjectId).enabled) throw new Error("Add a study time and activate this class before starting it.");
    if (subjectId === 'german' || subjectId === 'italian') {
      mockLanguageSessionId += 1;
      mockActiveLanguage = mockLanguageLesson(subjectId);
      mockActiveLanguage.session_id = String(mockLanguageSessionId);
      return { kind: 'language', lesson: mockActiveLanguage };
    }
    mockEngineeringSessionId += 1;
    mockActiveEngineering = mockEngineeringLesson(subjectId as FocusArea);
    mockActiveEngineering.session_id = String(mockEngineeringSessionId);
    return { kind: 'engineering', lesson: mockActiveEngineering };
  },
  startClassReview: async () => { throw new Error('Reviews need the desktop app.'); },
  endBlock: async () => { throw new Error('Blocks need the desktop app.'); },
  listExecutionRuns: async () => previewRuns(),
  reportFrontendError: async (message: string) => { console.error(message); },
  getExecutionLog: async (runId: string) => previewLogLines().filter((line) => line.run_id === runId),
  getRecentExecutionLog: async () => previewLogLines(),
  resumeClassroomSession: async (
    subjectId: ClassroomSubjectId,
  ): Promise<ClassroomSessionStart | null> => {
    if (
      (subjectId === 'german' || subjectId === 'italian') &&
      mockActiveLanguage?.language === subjectId
    ) {
      return { kind: 'language', lesson: mockActiveLanguage };
    }
    if (mockActiveEngineering?.subject_id === subjectId) {
      return { kind: 'engineering', lesson: mockActiveEngineering };
    }
    return null;
  },
  submitClassroomEngineeringSession: async (input: {
    session_id: number;
    answers: number[];
    reflection: string;
  }): Promise<EngineeringSessionResult> => {
    if (!mockActiveEngineering || mockActiveEngineering.session_id !== String(input.session_id)) {
      throw new Error('engineering classroom session not found');
    }
    const lesson = mockActiveEngineering;
    const reference = referenceFor(lesson.subject_id).questions.filter((question) => question.kind === 'mcq');
    if (input.answers.length !== lesson.questions.length || input.answers.some((answer, index) => !Number.isInteger(answer) || answer < 0 || answer >= lesson.questions[index].choices.length)) {
      throw new Error('Answer every displayed question before submitting.');
    }
    const correct = input.answers.map((answer, index) => lesson.questions[index].choices[answer] === reference[index].correct_answer);
    const score = correct.filter(Boolean).length / lesson.questions.length;
    mockActiveEngineering = null;
    return {
      session_id: String(input.session_id),
      subject_id: lesson.subject_id,
      passed: score >= 0.8,
      score,
      corrections: lesson.questions.map((question, index) => ({
        question_id: question.id,
        prompt: question.prompt,
        selected_answer: question.choices[input.answers[index]] ?? '',
        correct_answer: reference[index].correct_answer,
        correct: correct[index],
        explanation: reference[index].explanation,
      })),
      kind: 'lesson' as const,
      fresh_sample: true,
    };
  },
  // Shared-runtime lessons are prepared natively; the browser preview keeps its bundled lessons.
  saveClassLessonWork: async () => { throw new Error('Saved lesson work needs the desktop app.'); },
  saveClassCheckAnswer: async () => { throw new Error('Saved lesson work needs the desktop app.'); },
  submitClassCheck: async () => { throw new Error('Shared-runtime knowledge checks need the desktop app.'); },
  submitClassLanguageCheck: async () => { throw new Error('Shared-runtime knowledge checks need the desktop app.'); },
  skipAppointment: async () => { throw new Error('Appointments need the desktop app.'); },
  rescheduleAppointment: async () => { throw new Error('Appointments need the desktop app.'); },
  setClassFocusPolicy: async (subjectId: ClassroomSubjectId, policy: FocusPolicy) => {
    mockClassroomSettings[subjectId].focusPolicy = policy;
    return mockPrograms().find((program) => program.subject_id === subjectId)!;
  },
  getClassAppointments: async () => [],
  pauseClassLesson: async () => {},
  skipClassLesson: async (sessionId: string) => { if (mockActiveEngineering?.session_id === sessionId) mockActiveEngineering = null; },
  getExercise: async (owner: {
    course_id?: number | null;
    classroom_session_id?: number | null;
  }): Promise<ExerciseView | null> => {
    if (owner.classroom_session_id != null) {
      const lesson =
        mockActiveEngineering?.session_id === String(owner.classroom_session_id)
          ? mockActiveEngineering
          : null;
      if (!lesson?.exercise) return null;
      const completion = mockClassroomExerciseCompletions.get(owner.classroom_session_id);
      return {
        course_id: null,
        classroom_session_id: owner.classroom_session_id,
        study_session_id: null,
        ...lesson.exercise,
        draft: mockClassroomExerciseDrafts.get(owner.classroom_session_id) ?? null,
        completed: completion?.completed ?? false,
        reflection: completion?.reflection ?? '',
      };
    }
    return {
      ...MOCK_EXERCISE,
      course_id: owner.course_id ?? null,
      classroom_session_id: null,
      draft: mockExerciseDraft,
      completed: mockExerciseCompleted,
      reflection: mockExerciseReflection,
    };
  },
  saveExerciseDraft: async (
    owner: { course_id?: number | null; classroom_session_id?: number | null },
    draft: string,
  ) => {
    if (owner.classroom_session_id != null) {
      mockClassroomExerciseDrafts.set(owner.classroom_session_id, draft);
    } else if (owner.course_id === MOCK_COURSE_ID) {
      mockExerciseDraft = draft;
    }
  },
  saveExerciseCompletion: async (
    owner: { course_id?: number | null; classroom_session_id?: number | null },
    completed: boolean,
    reflection: string,
  ) => {
    if (owner.classroom_session_id != null) {
      mockClassroomExerciseCompletions.set(owner.classroom_session_id, {
        completed,
        reflection,
      });
    } else if (owner.course_id === MOCK_COURSE_ID) {
      mockExerciseCompleted = completed;
      mockExerciseReflection = reflection;
    }
  },
  getChat: async (owner: {
    course_id?: number | null;
    classroom_session_id?: number | null;
  }): Promise<ChatMessage[]> => {
    if (owner.classroom_session_id != null) {
      return mockClassroomChatThreads.get(owner.classroom_session_id) ?? [];
    }
    return mockChatThreads.get(owner.course_id ?? -1) ?? [];
  },
  sendChatMessage: async (
    owner: { course_id?: number | null; classroom_session_id?: number | null },
    message: string,
  ): Promise<ChatMessage[]> => {
    const trimmed = message.trim();
    if (!trimmed) throw new Error('message cannot be empty');
    if (trimmed.length > 2_000) throw new Error('message is too long');
    await new Promise((resolve) => setTimeout(resolve, 500));
    if (owner.classroom_session_id != null) {
      const thread = mockClassroomChatThreads.get(owner.classroom_session_id) ?? [];
      thread.push({ role: 'user', content: trimmed, section: null, follow_ups: [] });
      thread.push({
        role: 'assistant',
        content:
          'Start from the smallest mechanism in this lesson, then trace how it composes into the architecture decision. The opening analogy gives you the shape; its “where the analogy breaks” paragraph tells you which runtime constraint must replace the metaphor.',
        section: 'The precise model',
        follow_ups: [
          'Can you trace the mechanism step by step?',
          'Which observation would disprove your current model?',
          'How will you expose this in the exercise?',
        ],
      });
      mockClassroomChatThreads.set(owner.classroom_session_id, thread);
      return thread;
    }
    const thread = mockChatThreads.get(owner.course_id ?? -1) ?? [];
    thread.push({ role: 'user', content: trimmed, section: null, follow_ups: [] });
    thread.push({
      role: 'assistant',
      content:
        "The course's mental model: the loop drains every pending microtask completely before it ever looks at the next macrotask. Think of it like a chef who finishes every add-on ticket for the current dish before glancing at the next order, that ordering is what the diagram in \"The simple version\" is showing.",
      section: 'The precise model',
      follow_ups: [
        'Can you trace one complete event-loop turn?',
        'Which observation distinguishes microtasks from tasks?',
        'How does the exercise reveal checkpoint ordering?',
      ],
    });
    mockChatThreads.set(owner.course_id ?? -1, thread);
    return thread;
  },
  submitLanguageSession: async (input: {
    session_id: number;
    answers: number[];
    writing_response: string;
    speaking_completed: boolean;
    listened: boolean;
    confidence: number;
  }): Promise<LanguageSessionResult> => {
    if (!mockActiveLanguage || String(input.session_id) !== mockActiveLanguage.session_id) {
      throw new Error('language session not found');
    }
    const correctIndexes = [0, 0, 0, 0, 0];
    const correct = input.answers.filter((answer, index) => answer === correctIndexes[index]).length;
    const score = correct / correctIndexes.length;
    const result: LanguageSessionResult = {
      session_id: String(input.session_id),
      passed: score >= 0.6,
      score,
      corrections: mockActiveLanguage.questions.map((question, index) => ({
        question_id: question.id,
        prompt: question.prompt,
        selected_answer: question.choices[input.answers[index]] ?? '',
        correct_answer: question.choices[correctIndexes[index]],
        correct: input.answers[index] === correctIndexes[index],
        explanation:
          input.answers[index] === correctIndexes[index]
            ? 'Correct: this expression fits the scenario and register.'
            : 'Review the model dialogue and retrieve the complete phrase as one chunk.',
      })),
      requirement: null,
      level_advanced_to: null,
      current_level: mockActiveLanguage.level,
      progress: mockLanguageProgram(mockActiveLanguage.language),
    };
    mockActiveLanguage = null;
    return result;
  },
  setKioskLevel: async (level: string) => {
    mockKioskLevel = level as AppStateView['kiosk_level'];
  },
  setModel: async (model: string) => {
    mockModel = model as AppStateView['model'];
  },
  setAgent: async (agent: string, customBin?: string) => {
    mockAgent = agent as AppStateView['agent'];
    mockCustomBin = customBin ?? '';
  },
  setDeepseekApiKey: async (key: string) => {
    mockDeepseekKeyConfigured = key.trim().length > 0;
  },
  snoozeAlarm: async (_occurrenceId: string, minutes: number) => new Date(Date.now() + minutes * 60_000).toISOString(),
  showDesk: async () => {},
  startFromTray: async () => {},
  quitDesk: async () => { throw new Error('The browser preview cannot quit the desk.'); },
  hideTrayPanel: async () => {},
  sizeTrayPanel: async () => {},
  pauseSchedule: async () => {
    mockPaused = true;
  },
  resumeSchedule: async () => {
    mockPaused = false;
  },
  escapeSession: async () => {
    clearMockChatThreads();
    return true;
  },
  getEscapePhrase: async () =>
    'I am choosing to skip my training today and I accept the broken streak',
  getDashboard: async (query: ProgressQuery = {}): Promise<DashboardView> => {
    const programs = CLASSROOM_CATALOG.map(item => mockClassroomProgram(item.id));
    const dateAt = (ago: number) => { const d = new Date(); d.setDate(d.getDate()-ago); return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`; };
    const history: ProgressEntry[] = Array.from({length: 48}, (_, i) => {
      const program = programs[i % programs.length];
      return { source: program.kind === 'language' ? 'language' : 'classroom', owner_id: String(i+1), date: dateAt(Math.floor(i/2)), subject_id: program.subject_id, title: `${program.label}: ${['Foundations and first principles','Practice and retrieval','Applying the next concept'][i%3]}`, status: i===0 ? 'in_progress' : i%11===0 ? 'skipped' : 'completed', score: i===0 || i%11===0 ? null : 0.7+(i%4)/10, can_read: true };
    });
    const scoped = history.filter(h => !query.subject_id || h.subject_id===query.subject_id);
    const completed = scoped.filter(h => h.status==='completed');
    const matching = scoped.filter(h => (!query.status || h.status===query.status) && `${h.title} ${h.subject_id}`.toLowerCase().includes((query.search??'').trim().toLowerCase()));
    const page = Math.min(Math.max(0,query.page??0),Math.floor(Math.max(0,matching.length-1)/8));
    const dates = new Set(completed.map(h=>h.date));
    let streak = 0; let ago = dates.has(dateAt(0)) ? 0 : 1; while(dates.has(dateAt(ago++))) streak++;
    return {today:dateAt(0), classes:programs.map(p=>({...p,completed_sessions:history.filter(h=>h.subject_id===p.subject_id && h.status==='completed').length})), completed_sessions:completed.length, study_days:dates.size, streak, activity:Array.from({length:28},(_,i)=>({date:dateAt(27-i),completed:completed.filter(h=>h.date===dateAt(27-i)).length})), history:matching.slice(page*8,page*8+8),history_total:matching.length,page,page_size:8};
  },
  getProgressLesson: async (_source: ProgressEntry['source'], _ownerId: string): Promise<ProgressLesson | null> => ({ title: 'Preview lesson', date: new Date().toISOString().slice(0,10), markdown: COURSE_MD, course_id: null, classroom_session_id: null, study_session_id: null }),
  getLessonDocument: async (_source: ProgressEntry['source'], _ownerId: string): Promise<LessonDocument> => {
    const lesson = mockEngineeringLesson(mockSelectedFocus);
    return {
      file_stem: `preview-${lesson.concept_slug}`,
      class_label: lesson.label,
      title: lesson.title,
      topic: lesson.concept_title,
      category: lesson.category,
      date: new Date().toISOString().slice(0, 10),
      status: 'in progress',
      score: null,
      tutor: lesson.agent_used,
      answer_key: false,
      research_note: lesson.research_note,
      review_notes: lesson.review_notes,
      level: lesson.level,
      markdown: lesson.markdown,
      resources: lesson.resources,
      exercise: lesson.exercise ? { title: lesson.exercise.title, instructions: lesson.exercise.instructions, deliverable: lesson.exercise.deliverable ?? null, starter_code: lesson.exercise.starter_code ?? null, hints: lesson.exercise.hints, draft: null, reflection: null, completed: false } : null,
      questions: lesson.questions.map((question, index) => ({ position: index + 1, prompt: question.prompt, section: question.section, objective: question.learning_objective, choices: question.choices, correct_answer: null, explanation: null, your_answer: index === 0 ? question.choices[1] : null, result: null })),
      language: null,
    };
  },
  saveLessonPdf: async (_source: ProgressEntry['source'], ownerId: string, bytes: Uint8Array): Promise<LessonFileResult> => {
    const lesson = mockEngineeringLesson(mockSelectedFocus);
    const fileName = `preview-${ownerId}.pdf`;
    if (typeof document !== 'undefined') {
      const url = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }));
      const anchor = document.createElement('a');
      anchor.href = url; anchor.download = fileName; anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
    return { path: `~/Documents/Principia Desk/lessons/${lesson.label}/${fileName}`, file_name: fileName, title: lesson.title, questions: lesson.questions.length, answer_key: false };
  },
  exportLessonCsv: async (_source: ProgressEntry['source'], ownerId: string): Promise<LessonFileResult> => {
    // The desktop writes the file natively; the preview hands the browser a
    // small stand-in so the control can be exercised end to end.
    const lesson = mockEngineeringLesson(mockSelectedFocus);
    const quote = (value: string) => `"${value.replace(/"/g, '""')}"`;
    const lines = [
      'kind,position,section,text,detail,choice_a,choice_b,choice_c,choice_d,correct_answer,explanation,your_answer,result,url',
      `meta,,title,${quote(lesson.title)},,,,,,,,,,`,
      ...lesson.questions.map((question, index) => `question,${index + 1},${quote(question.section)},${quote(question.prompt)},${quote(question.learning_objective)},${question.choices.map(quote).join(',')},,,,,`),
    ];
    const fileName = `preview-${ownerId}.csv`;
    if (typeof document !== 'undefined') {
      const url = URL.createObjectURL(new Blob([`\ufeff${lines.join('\r\n')}\r\n`], { type: 'text/csv' }));
      const anchor = document.createElement('a');
      anchor.href = url; anchor.download = fileName; anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
    return { path: `~/Documents/Principia Desk/lessons/${fileName}`, file_name: fileName, title: lesson.title, questions: lesson.questions.length, answer_key: false };
  },
  revealExport: async (_path: string) => {},
  markFrontendReady: async () => {},
};
