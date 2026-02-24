use super::migration::{
    Migration, MigrationDiscoveryError, MigrationRunner, MigrationRunnerError, MigrationsDir,
};
use super::transaction::{
    CreateTransactionWithPostingsError, NewPostingInput, Posting, PostingDirection, PostingListError,
    PostingWriteError, Transaction, TransactionListError, TransactionWriteError,
};
use std::fmt::{Display, Formatter};
use std::path::Path;
use uuid::Uuid;

const LIST_TRANSACTIONS_SQL: &str = "
SELECT
  id,
  statement_id,
  description,
  posted_at,
  created_at
FROM transactions
ORDER BY posted_at, created_at, id
";

const GET_TRANSACTION_BY_ID_SQL: &str = "
SELECT
  id,
  statement_id,
  description,
  posted_at,
  created_at
FROM transactions
WHERE id = ?1
";

const LIST_POSTINGS_SQL: &str = "
SELECT
  id,
  transaction_id,
  account_id,
  amount,
  currency,
  direction
FROM postings
ORDER BY transaction_id, id
";

const LIST_POSTINGS_FOR_TRANSACTION_SQL: &str = "
SELECT
  id,
  transaction_id,
  account_id,
  amount,
  currency,
  direction
FROM postings
WHERE transaction_id = ?1
ORDER BY id
";

const GET_POSTING_BY_ID_SQL: &str = "
SELECT
  id,
  transaction_id,
  account_id,
  amount,
  currency,
  direction
FROM postings
WHERE id = ?1
";

pub struct Db {
    conn: rusqlite::Connection,
}

#[derive(Debug)]
pub enum DbError {
    Open(rusqlite::Error),
    DiscoverMigrations(MigrationDiscoveryError),
    RunMigrations(MigrationRunnerError),
}

impl Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(err) => write!(f, "failed to open sqlite database: {err}"),
            Self::DiscoverMigrations(err) => {
                write!(f, "failed to discover embedded migrations: {err}")
            }
            Self::RunMigrations(err) => write!(f, "failed to run embedded migrations: {err}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open(err) => Some(err),
            Self::DiscoverMigrations(err) => Some(err),
            Self::RunMigrations(err) => Some(err),
        }
    }
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open(path).map_err(DbError::Open)?;
        Self::from_connection(conn)
    }

    pub fn open_for_tests() -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open_in_memory().map_err(DbError::Open)?;
        Self::from_connection(conn)
    }

    pub fn list_transactions(&self) -> Result<Vec<Transaction>, TransactionListError> {
        let mut stmt = self.conn.prepare(LIST_TRANSACTIONS_SQL)?;
        let mut rows = stmt.query([])?;
        let mut transactions = Vec::new();

        while let Some(row) = rows.next()? {
            transactions.push(transaction_from_row(row)?);
        }

        Ok(transactions)
    }

    pub fn create_transaction(
        &self,
        id: Uuid,
        statement_id: Option<Uuid>,
        description: Option<&str>,
        posted_at: &str,
    ) -> Result<Transaction, TransactionWriteError> {
        let id_str = id.to_string();
        let statement_id_str = statement_id.map(|v| v.to_string());
        self.conn.execute(
            "
            INSERT INTO transactions (id, statement_id, description, posted_at)
            VALUES (?1, ?2, ?3, ?4)
            ",
            rusqlite::params![id_str, statement_id_str, description, posted_at],
        )?;
        self.get_transaction_by_id(id)?
            .ok_or(TransactionWriteError::NotFound(id))
    }

    pub fn list_postings(&self) -> Result<Vec<Posting>, PostingListError> {
        let mut stmt = self.conn.prepare(LIST_POSTINGS_SQL)?;
        let mut rows = stmt.query([])?;
        let mut postings = Vec::new();

        while let Some(row) = rows.next()? {
            postings.push(posting_from_row(row)?);
        }

        Ok(postings)
    }

    pub fn list_postings_for_transaction(
        &self,
        transaction_id: Uuid,
    ) -> Result<Vec<Posting>, PostingListError> {
        let mut stmt = self.conn.prepare(LIST_POSTINGS_FOR_TRANSACTION_SQL)?;
        let mut rows = stmt.query([transaction_id.to_string()])?;
        let mut postings = Vec::new();

        while let Some(row) = rows.next()? {
            postings.push(posting_from_row(row)?);
        }

        Ok(postings)
    }

    pub fn create_posting(
        &self,
        id: Uuid,
        transaction_id: Uuid,
        account_id: Uuid,
        amount: i64,
        currency: &str,
        direction: PostingDirection,
    ) -> Result<Posting, PostingWriteError> {
        let id_str = id.to_string();
        let transaction_id_str = transaction_id.to_string();
        let account_id_str = account_id.to_string();
        self.conn.execute(
            "
            INSERT INTO postings (id, transaction_id, account_id, amount, currency, direction)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            rusqlite::params![
                id_str,
                transaction_id_str,
                account_id_str,
                amount,
                currency,
                direction.as_str()
            ],
        )?;
        self.get_posting_by_id(id)?.ok_or(PostingWriteError::NotFound(id))
    }

    pub fn create_transaction_with_postings(
        &mut self,
        id: Uuid,
        statement_id: Option<Uuid>,
        description: Option<&str>,
        posted_at: &str,
        postings: &[NewPostingInput],
    ) -> Result<(Transaction, Vec<Posting>), CreateTransactionWithPostingsError> {
        let tx = self.conn.transaction()?;
        let id_str = id.to_string();
        let statement_id_str = statement_id.map(|v| v.to_string());

        tx.execute(
            "
            INSERT INTO transactions (id, statement_id, description, posted_at)
            VALUES (?1, ?2, ?3, ?4)
            ",
            rusqlite::params![id_str, statement_id_str, description, posted_at],
        )?;

        for posting in postings {
            tx.execute(
                "
                INSERT INTO postings (id, transaction_id, account_id, amount, currency, direction)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                rusqlite::params![
                    posting.id.to_string(),
                    id.to_string(),
                    posting.account_id.to_string(),
                    posting.amount,
                    posting.currency.as_str(),
                    posting.direction.as_str(),
                ],
            )?;
        }

        tx.commit()?;

        let transaction = self
            .get_transaction_by_id(id)
            .map_err(CreateTransactionWithPostingsError::from_transaction_write)?
            .ok_or(CreateTransactionWithPostingsError::TransactionNotFound(id))?;

        let mut inserted_postings = Vec::with_capacity(postings.len());
        for posting in postings {
            let inserted = self
                .get_posting_by_id(posting.id)
                .map_err(CreateTransactionWithPostingsError::from_posting_write)?
                .ok_or(CreateTransactionWithPostingsError::PostingNotFound(posting.id))?;
            inserted_postings.push(inserted);
        }

        Ok((transaction, inserted_postings))
    }

    fn from_connection(conn: rusqlite::Connection) -> Result<Self, DbError> {
        let source = MigrationsDir::embedded();
        let migrations = Migration::from_source(&source).map_err(DbError::DiscoverMigrations)?;
        let runner = MigrationRunner::new(&conn);
        runner
            .run(&source, &migrations)
            .map_err(DbError::RunMigrations)?;
        Ok(Self { conn })
    }

    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    fn get_transaction_by_id(&self, id: Uuid) -> Result<Option<Transaction>, TransactionWriteError> {
        let mut stmt = self.conn.prepare(GET_TRANSACTION_BY_ID_SQL)?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => transaction_from_row(row)
                .map(Some)
                .map_err(TransactionWriteError::ReadBack),
            None => Ok(None),
        }
    }

    fn get_posting_by_id(&self, id: Uuid) -> Result<Option<Posting>, PostingWriteError> {
        let mut stmt = self.conn.prepare(GET_POSTING_BY_ID_SQL)?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => posting_from_row(row).map(Some).map_err(PostingWriteError::ReadBack),
            None => Ok(None),
        }
    }
}

fn transaction_from_row(row: &rusqlite::Row<'_>) -> Result<Transaction, TransactionListError> {
    let id_str: String = row.get("id")?;
    let statement_id_str: Option<String> = row.get("statement_id")?;

    let id = Uuid::parse_str(&id_str).map_err(|source| TransactionListError::InvalidId {
        value: id_str.clone(),
        source,
    })?;
    let statement_id = statement_id_str
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|source| TransactionListError::InvalidStatementId {
            value: statement_id_str.clone().unwrap_or_default(),
            source,
        })?;

    Ok(Transaction {
        id,
        statement_id,
        description: row.get("description")?,
        posted_at: row.get("posted_at")?,
        created_at: row.get("created_at")?,
    })
}

fn posting_from_row(row: &rusqlite::Row<'_>) -> Result<Posting, PostingListError> {
    let id_str: String = row.get("id")?;
    let transaction_id_str: String = row.get("transaction_id")?;
    let account_id_str: String = row.get("account_id")?;
    let direction_str: String = row.get("direction")?;

    let id = Uuid::parse_str(&id_str).map_err(|source| PostingListError::InvalidId {
        value: id_str.clone(),
        source,
    })?;
    let transaction_id = Uuid::parse_str(&transaction_id_str).map_err(|source| {
        PostingListError::InvalidTransactionId {
            value: transaction_id_str.clone(),
            source,
        }
    })?;
    let account_id = Uuid::parse_str(&account_id_str).map_err(|source| {
        PostingListError::InvalidAccountId {
            value: account_id_str.clone(),
            source,
        }
    })?;

    Ok(Posting {
        id,
        transaction_id,
        account_id,
        amount: row.get("amount")?,
        currency: row.get("currency")?,
        direction: PostingDirection::from_db_str(&direction_str)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_for_tests_applies_embedded_migrations() {
        let db = Db::open_for_tests().expect("open in-memory db");

        let applied_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 4);

        let note_column_exists: i64 = db
            .conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM pragma_table_info('accounts')
                    WHERE name = 'note'
                )",
                [],
                |row| row.get(0),
            )
            .expect("check note column");
        assert_eq!(note_column_exists, 1);
    }

    #[test]
    fn open_creates_db_and_applies_migrations() {
        let temp_dir = tempdir().expect("create temp dir");
        let db_path = temp_dir.path().join("tally42.db");

        let db = Db::open(&db_path).expect("open file db");
        assert!(db_path.is_file());

        let accounts_exists: i64 = db
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='accounts')",
                [],
                |row| row.get(0),
            )
            .expect("check accounts table");
        assert_eq!(accounts_exists, 1);
    }

    #[test]
    fn repeated_open_is_idempotent() {
        let temp_dir = tempdir().expect("create temp dir");
        let db_path = temp_dir.path().join("tally42.db");

        let _first = Db::open(&db_path).expect("first open");
        let second = Db::open(&db_path).expect("second open");

        let applied_count: i64 = second
            .conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 4);
    }

    #[test]
    fn create_transaction_inserts_and_returns_transaction() {
        let db = Db::open_for_tests().expect("open in-memory db");

        let tx_id = Uuid::parse_str("17171717-1717-1717-1717-171717171717").unwrap();
        let transaction = db
            .create_transaction(tx_id, None, Some("Coffee"), "2026-02-20")
            .expect("create transaction");

        assert_eq!(transaction.id, tx_id);
        assert_eq!(transaction.statement_id, None);
        assert_eq!(transaction.description.as_deref(), Some("Coffee"));
        assert_eq!(transaction.posted_at, "2026-02-20");
        assert!(!transaction.created_at.is_empty());
    }

    #[test]
    fn create_transaction_with_statement_id_round_trips() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("18181818-1818-1818-1818-181818181818").unwrap();
        db.create_account(account_id, None, "checking", "USD", None)
            .expect("create account");
        let statement_id = Uuid::parse_str("19191919-1919-1919-1919-191919191919").unwrap();
        db.create_statement(
            statement_id,
            "Bank",
            account_id,
            "2026-02-01",
            "2026-02-28",
            "USD",
            "sha256:tx-stmt",
            123,
            None,
        )
        .expect("create statement");

        let tx_id = Uuid::parse_str("20202020-2020-2020-2020-202020202020").unwrap();
        let transaction = db
            .create_transaction(tx_id, Some(statement_id), None, "2026-02-21")
            .expect("create transaction");

        assert_eq!(transaction.statement_id, Some(statement_id));
        assert_eq!(transaction.description, None);
    }

    #[test]
    fn list_transactions_returns_rows_and_maps_nullable_fields() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let first_id = Uuid::parse_str("21212121-2121-2121-2121-212121212121").unwrap();
        let second_id = Uuid::parse_str("22222222-aaaa-bbbb-cccc-222222222222").unwrap();

        db.create_transaction(first_id, None, None, "2026-02-10")
            .expect("create first transaction");
        db.create_transaction(second_id, None, Some("Rent"), "2026-02-11")
            .expect("create second transaction");

        let transactions = db.list_transactions().expect("list transactions");
        assert_eq!(transactions.len(), 2);
        assert!(transactions
            .iter()
            .any(|t| t.id == first_id && t.statement_id.is_none() && t.description.is_none()));
        assert!(transactions.iter().any(|t| t.id == second_id
            && t.description.as_deref() == Some("Rent")));
    }

    #[test]
    fn create_posting_inserts_and_returns_posting() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("23232323-2323-2323-2323-232323232323").unwrap();
        db.create_account(account_id, None, "expense:coffee", "USD", None)
            .expect("create account");
        let tx_id = Uuid::parse_str("24242424-2424-2424-2424-242424242424").unwrap();
        db.create_transaction(tx_id, None, Some("Coffee"), "2026-02-22")
            .expect("create transaction");

        let posting_id = Uuid::parse_str("25252525-2525-2525-2525-252525252525").unwrap();
        let posting = db
            .create_posting(
                posting_id,
                tx_id,
                account_id,
                450,
                "USD",
                PostingDirection::Debit,
            )
            .expect("create posting");

        assert_eq!(posting.id, posting_id);
        assert_eq!(posting.transaction_id, tx_id);
        assert_eq!(posting.account_id, account_id);
        assert_eq!(posting.amount, 450);
        assert_eq!(posting.currency, "USD");
        assert_eq!(posting.direction, PostingDirection::Debit);
    }

    #[test]
    fn list_postings_for_transaction_filters_and_orders() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("26262626-2626-2626-2626-262626262626").unwrap();
        db.create_account(account_id, None, "assets:cash", "USD", None)
            .expect("create account");

        let tx_a = Uuid::parse_str("27272727-2727-2727-2727-272727272727").unwrap();
        let tx_b = Uuid::parse_str("28282828-2828-2828-2828-282828282828").unwrap();
        db.create_transaction(tx_a, None, None, "2026-02-01")
            .expect("create tx a");
        db.create_transaction(tx_b, None, None, "2026-02-02")
            .expect("create tx b");

        let posting_a2 = Uuid::parse_str("29292929-2929-2929-2929-292929292929").unwrap();
        let posting_a1 = Uuid::parse_str("2a2a2a2a-2a2a-2a2a-2a2a-2a2a2a2a2a2a").unwrap();
        let posting_b1 = Uuid::parse_str("2b2b2b2b-2b2b-2b2b-2b2b-2b2b2b2b2b2b").unwrap();

        db.create_posting(
            posting_a2,
            tx_a,
            account_id,
            100,
            "USD",
            PostingDirection::Credit,
        )
        .expect("create posting a2");
        db.create_posting(
            posting_a1,
            tx_a,
            account_id,
            100,
            "USD",
            PostingDirection::Debit,
        )
        .expect("create posting a1");
        db.create_posting(posting_b1, tx_b, account_id, 50, "USD", PostingDirection::Debit)
            .expect("create posting b1");

        let postings = db
            .list_postings_for_transaction(tx_a)
            .expect("list postings for transaction");
        let ids: Vec<_> = postings.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![posting_a2, posting_a1]);
    }

    #[test]
    fn create_transaction_with_postings_is_atomic_on_posting_failure() {
        let mut db = Db::open_for_tests().expect("open in-memory db");
        let valid_account_id = Uuid::parse_str("2c2c2c2c-2c2c-2c2c-2c2c-2c2c2c2c2c2c").unwrap();
        db.create_account(valid_account_id, None, "assets:checking", "USD", None)
            .expect("create account");

        let tx_id = Uuid::parse_str("2d2d2d2d-2d2d-2d2d-2d2d-2d2d2d2d2d2d").unwrap();
        let good_posting_id = Uuid::parse_str("2e2e2e2e-2e2e-2e2e-2e2e-2e2e2e2e2e2e").unwrap();
        let bad_posting_id = Uuid::parse_str("2f2f2f2f-2f2f-2f2f-2f2f-2f2f2f2f2f2f").unwrap();
        let missing_account_id = Uuid::parse_str("30303030-3030-3030-3030-303030303030").unwrap();

        let err = db
            .create_transaction_with_postings(
                tx_id,
                None,
                Some("atomic"),
                "2026-02-23",
                &[
                    NewPostingInput {
                        id: good_posting_id,
                        account_id: valid_account_id,
                        amount: 100,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Debit,
                    },
                    NewPostingInput {
                        id: bad_posting_id,
                        account_id: missing_account_id,
                        amount: 100,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Credit,
                    },
                ],
            )
            .expect_err("atomic create should fail");

        assert!(matches!(err, CreateTransactionWithPostingsError::Sql(_)));
        assert!(db
            .list_transactions()
            .expect("list transactions")
            .iter()
            .all(|t| t.id != tx_id));
        assert!(db
            .list_postings()
            .expect("list postings")
            .iter()
            .all(|p| p.transaction_id != tx_id));
    }
}
