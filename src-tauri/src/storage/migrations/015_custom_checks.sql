-- The marks a learner's own class collects on its way to being published:
-- the draft the tutor last read back, and the draft the learner confirmed
-- reading themselves. Publishing needs the review, its findings settled,
-- the sources fetched and accepted, and that confirmation on the draft as
-- it stands. Absent until the first check runs.
ALTER TABLE custom_courses ADD COLUMN checks_json TEXT;
