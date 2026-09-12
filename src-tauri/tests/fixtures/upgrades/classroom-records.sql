INSERT INTO classroom_programs (subject_id, kind, label, short_code, enabled, agent, model, prompt_profile, prompt_version, session_minutes, updated_at)
VALUES ('typescript', 'engineering', 'TypeScript', 'TS', 1, 'claude', 'sonnet', 'classroom.typescript', 'v1', 45, '2026-09-08T08:00:00Z');
INSERT INTO classroom_schedule_slots (id, subject_id, hour, minute, weekdays_json, enabled, source, created_at)
VALUES (51, 'typescript', 10, 30, '[1,3,5]', 1, 'manual', '2026-09-08T08:00:00Z');
INSERT INTO classroom_sessions (id, subject_id, slot_id, session_date, status, title, payload_json, score, response_json, exercise_draft, exercise_completed, exercise_reflection, agent_used, prompt_version, started_at, completed_at)
VALUES (501, 'typescript', 51, '2026-09-08', 'completed', 'Original class document', '{"archived":"original payload"}', 0.8, '{"answers":[1,2,0]}', 'class draft with owner 501', 1, 'class reflection', 'claude', 'classroom.typescript.v1', '2026-09-08T10:30:00Z', '2026-09-08T11:15:00Z');
INSERT INTO classroom_exit_attempts (session_id, concept_id, question_id, section, learning_objective, misconception, correct, attempted_at)
VALUES (501, 50, 1, 'Retained section', 'Retained objective', 'Retained misconception', 0, '2026-09-08T11:10:00Z');
INSERT INTO language_programs (language, enabled, start_level, current_level, target_level, start_date, weekly_minutes, session_minutes, preferred, updated_at)
VALUES ('german', 1, 'A2', 'B1', 'B2', '2026-07-01', 150, 30, 1, '2026-09-08T08:00:00Z'),
       ('italian', 0, 'A1', 'A1', 'A2', '2026-08-01', 90, 30, 0, '2026-09-08T08:00:00Z');
INSERT INTO language_schedule_slots (id, language, hour, minute, weekdays_json, enabled, created_at)
VALUES (51, 'german', 8, 0, '[2,4,6]', 1, '2026-09-08T08:00:00Z');
INSERT INTO language_sessions (id, slot_id, language, session_date, level, unit_slug, phase, status, lesson_json, score, response_json, started_at, completed_at)
VALUES (501, 51, 'german', '2026-09-08', 'B1', 'retained-unit', 2, 'completed', '{"dialogue":"Unverändert"}', 0.75, '{"writing":"Meine Antwort"}', '2026-09-08T08:00:00Z', '2026-09-08T08:30:00Z');
INSERT INTO language_unit_progress (language, unit_slug, phase_completed, score_ema, encounters, last_seen_date, next_review_date)
VALUES ('german', 'retained-unit', 2, 0.75, 4, '2026-09-08', '2026-09-15');
INSERT INTO language_skill_scores (language, strand, score_ema, encounters, last_seen_date)
VALUES ('german', 'reading', 0.8, 6, '2026-09-08'), ('german', 'spoken_interaction', 0.25, 2, '2026-09-07');
INSERT INTO exercise_drafts (course_id, draft, completed, reflection, updated_at)
VALUES (501, 'course draft with owner 501', 1, 'course reflection', '2026-09-08T19:00:00Z');
