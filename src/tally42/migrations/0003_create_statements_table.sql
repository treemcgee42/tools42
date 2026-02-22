CREATE TABLE statements (
  id TEXT PRIMARY KEY,

  institution TEXT NOT NULL,
  account_id TEXT NOT NULL,

  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,

  currency TEXT NOT NULL,

  file_hash TEXT NOT NULL UNIQUE,
  file_size INTEGER NOT NULL,

  imported_at TEXT NOT NULL DEFAULT (datetime('now')),
  replaced_by TEXT,

  FOREIGN KEY(account_id) REFERENCES accounts(id),
  FOREIGN KEY(replaced_by) REFERENCES statements(id)
);
