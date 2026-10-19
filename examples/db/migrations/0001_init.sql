-- Reviewed initial schema for the reference app (Phase 7 example).
-- Non-destructive: creates tables and a view only.
CREATE TABLE teams (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);

CREATE TABLE tasks (
  id INTEGER PRIMARY KEY,
  team_id INTEGER NOT NULL REFERENCES teams (id),
  title TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0
);

CREATE VIEW v_team_performance AS
  SELECT t.id AS team_id, t.name AS team_name, COUNT(k.id) AS open_tasks
  FROM teams AS t
  LEFT JOIN tasks AS k ON k.team_id = t.id AND k.done = 0
  GROUP BY t.id, t.name;
