-- A study time may last a different number of minutes on each weekday it
-- fires on. The map is keyed by ISO weekday (1 = Monday) and holds minutes;
-- a missing day, or a missing map, means the class's default session length.
ALTER TABLE classroom_schedule_slots ADD COLUMN durations_json TEXT;
