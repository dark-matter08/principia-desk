import { describe, expect, it } from 'vitest';
import { COURSES, ENGINEERING_COURSES } from './catalog';
import { mockApi } from './mock';
import concepts from '../../src-tauri/seed/concepts.json';

const references = import.meta.glob<{ slug: string; questions: { kind: string; choices: string[] | null; correct_answer: string }[] }>('../../src-tauri/seed/fallback_courses/*.json', { eager: true, import: 'default' });

describe('catalog contract in the browser preview', () => {
  it('exposes all nine native-shaped definitions and the shell prerequisite', async () => {
    expect(await mockApi.getCatalog()).toEqual(COURSES);
    expect(COURSES).toHaveLength(9);
    expect(ENGINEERING_COURSES).toHaveLength(7);
    expect(COURSES.find((course) => course.id === 'bash-scripting')?.prerequisite_courses).toEqual(['linux-bash']);
  });

  it.each(ENGINEERING_COURSES)('$id loads its own curriculum and grades its own reference questions', async (course) => {
    const map = await mockApi.getCurriculumMap(course.id);
    const authored = concepts.filter((concept) => concept.focus === course.id);
    expect(map.concepts.map((concept) => concept.slug)).toEqual(authored.map((concept) => concept.slug));
    expect(map.month_outcome).toBe(course.outcome);
    // Each course keeps its own hour: classes that stay active may not overlap.
    await mockApi.upsertClassroomSlot({ subject_id: course.id, hour: 9 + ENGINEERING_COURSES.indexOf(course), minute: 0, weekdays: [1, 3, 5], enabled: true });
    await mockApi.configureClassroomProgram({ subject_id: course.id, enabled: true, agent: 'claude', model: 'opus', custom_agent_bin: '', session_minutes: 30 });
    const started = await mockApi.startClassroomSession(course.id);
    expect(started.kind).toBe('engineering');
    if (started.kind !== 'engineering') throw new Error('wrong subject adapter');
    const lesson = started.lesson;
    const seed = authored.find((concept) => concept.slug === lesson.concept_slug)!;
    expect(seed).toBeDefined();
    expect(lesson.curriculum).toEqual(seed.curriculum);
    expect(course.reference_lessons).toContain(lesson.concept_slug);
    const reference = Object.values(references).find((reference) => reference.slug === lesson.concept_slug)!;
    const answers = reference.questions.filter((question) => question.kind === 'mcq').map((question) => question.choices!.indexOf(question.correct_answer));
    const result = await mockApi.submitClassroomEngineeringSession({ session_id: Number(lesson.session_id), answers, reflection: 'Evidence from the reference exercise.' });
    expect(result.score).toBe(1);
    expect(result.corrections.every((correction) => correction.correct)).toBe(true);
  });
});
