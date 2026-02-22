PRAGMA foreign_keys=OFF;
BEGIN;

CREATE TABLE accounts_new (
  id           TEXT PRIMARY KEY,
  parent_id    TEXT REFERENCES accounts_new(id),
  name         TEXT NOT NULL,
  currency     TEXT NOT NULL,
  is_closed    INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  note         TEXT,

  UNIQUE(parent_id, name)
);

INSERT INTO accounts_new (id, parent_id, name, currency, is_closed, created_at)
SELECT id, NULL, name, currency, is_closed, created_at -- drop "kind"
FROM accounts;

DROP TABLE accounts;
ALTER TABLE accounts_new RENAME TO accounts;

CREATE INDEX IF NOT EXISTS idx_accounts_parent_id ON accounts(parent_id);

COMMIT;
PRAGMA foreign_keys=ON;
