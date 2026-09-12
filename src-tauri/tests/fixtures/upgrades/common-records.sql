INSERT INTO config VALUES ('kiosk_level', 'firm'), ('selected_focus', 'system-design'), ('quiz_round:2026-09-08', '{"question_ids":[601],"answer":"preserved"}');
INSERT INTO concepts (id, slug, title, category, weight, times_picked, last_picked_date, active, tier, prereqs_json)
VALUES (50, 'cap-theorem', 'CAP theorem', 'fundamentals', 1.5, 4, '2026-09-08', 1, 0, '[]');
INSERT INTO sessions (date, concept_id, status, current_step, started_at, reading_seconds, session_type, plan_reason)
VALUES ('2026-09-08', 50, 'in_progress', 'course', '2026-09-08T18:00:00Z', 95, 'lesson', 'Retained learner choice');
INSERT INTO courses (id, session_date, concept_id, markdown, resources_json, source, generated_at)
VALUES (501, '2026-09-08', 50, '# First archived lesson — Grüße', '[{"url":"https://www.postgresql.org/docs/current/"}]', 'fallback', '2026-09-08T18:01:00Z'),
       (502, '2026-09-08', 50, '# Second document on the same day', '[]', 'claude', '2026-09-08T18:30:00Z');
INSERT INTO questions (id, course_id, prompt, kind, choices_json, correct_answer, explanation)
VALUES (601, 501, 'Retained question?', 'mcq', '["A","B","C","D"]', 'B', 'Original feedback');
INSERT INTO attempts (id, question_id, session_date, user_answer, correct, grader_feedback, graded_by)
VALUES (701, 601, '2026-09-08', 'A', 0, 'Misconception retained', 'local');
INSERT INTO carryover (question_id, failed_on, times_failed, scheduled_for) VALUES (601, '2026-09-08', 2, '2026-09-09');
INSERT INTO audio_scripts (course_id, lines_json, engine, audio_dir, created_at)
VALUES (501, '[{"speaker":"teacher","text":"Retain the media path."}]', 'vibevoice', '/retained/audio/501', '2026-09-08T18:10:00Z');
INSERT INTO exit_questions (id, course_id, prompt, choices_json, correct_answer, explanation)
VALUES (801, 501, 'Exit question?', '["A","B","C","D"]', 'A', 'Retained exit explanation');
INSERT INTO mastery (concept_id, state, score_ema, encounters, last_seen_date, next_review_date, review_interval_days, teacher_notes)
VALUES (50, 'practicing', 0.725, 4, '2026-09-08', '2026-09-15', 7, 'Imported history must remain distinct from new assessment evidence.');
INSERT INTO profile VALUES ('learning_goal', 'Understand services and failure');
INSERT INTO generation_jobs (id, kind, target_date, status, attempts, error, created_at)
VALUES (901, 'quiz', '2026-09-09', 'failed', 1, 'Original provider unavailable', '2026-09-08T22:00:00Z');
