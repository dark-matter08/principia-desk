-- A study time may start at a different time on each weekday it fires on.
-- The map is keyed by ISO weekday (1 = Monday) and holds "HH:MM"; a missing
-- day, or a missing map, means the rule's own hour and minute.
ALTER TABLE classroom_schedule_slots ADD COLUMN starts_json TEXT;
