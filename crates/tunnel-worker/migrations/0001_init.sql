CREATE TABLE clients (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  token_hash   TEXT NOT NULL UNIQUE,
  token_prefix TEXT NOT NULL,
  created_at   INTEGER NOT NULL,
  disabled     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE routes (
  id           TEXT PRIMARY KEY,
  client_id    TEXT NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
  kind         TEXT NOT NULL,
  matcher      TEXT NOT NULL,
  target       TEXT NOT NULL,
  strip_prefix INTEGER NOT NULL DEFAULT 1,
  created_at   INTEGER NOT NULL,
  UNIQUE(kind, matcher)
);

CREATE INDEX idx_routes_client ON routes(client_id);
