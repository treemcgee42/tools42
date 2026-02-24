CREATE TABLE transactions (
  id TEXT PRIMARY KEY,
  statement_id TEXT,
  description TEXT,
  posted_at TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),

  FOREIGN KEY(statement_id) REFERENCES statements(id)
);

CREATE TABLE postings (
  id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  amount INTEGER NOT NULL,
  currency TEXT NOT NULL,
  direction TEXT NOT NULL CHECK (direction IN ('debit', 'credit')),

  FOREIGN KEY(transaction_id) REFERENCES transactions(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id) REFERENCES accounts(id)
);
