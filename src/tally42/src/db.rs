use crate::account::{Account, AccountListError, AccountWriteError};
use crate::migration::{
    Migration, MigrationDiscoveryError, MigrationRunner, MigrationRunnerError, MigrationsDir,
};
use std::fmt::{Display, Formatter};
use std::path::Path;
use uuid::Uuid;

const LIST_ACCOUNTS_SQL: &str = "
SELECT id, parent_id, name, currency, is_closed, created_at, note
FROM accounts
ORDER BY parent_id, name, id
";

const GET_ACCOUNT_BY_ID_SQL: &str = "
SELECT id, parent_id, name, currency, is_closed, created_at, note
FROM accounts
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

    pub fn list_accounts(&self) -> Result<Vec<Account>, AccountListError> {
        let mut stmt = self.conn.prepare(LIST_ACCOUNTS_SQL)?;
        let mut rows = stmt.query([])?;
        let mut accounts = Vec::new();

        while let Some(row) = rows.next()? {
            accounts.push(account_from_row(row)?);
        }

        Ok(accounts)
    }

    pub fn create_account(
        &self,
        id: Uuid,
        parent_id: Option<Uuid>,
        name: &str,
        currency: &str,
        note: Option<&str>,
    ) -> Result<Account, AccountWriteError> {
        let id_str = id.to_string();
        let parent_id_str = parent_id.map(|p| p.to_string());
        self.conn.execute(
            "
            INSERT INTO accounts (id, parent_id, name, currency, is_closed, note)
            VALUES (?1, ?2, ?3, ?4, 0, ?5)
            ",
            rusqlite::params![id_str, parent_id_str, name, currency, note],
        )?;
        self.get_account_by_id(id)?.ok_or(AccountWriteError::NotFound(id))
    }

    pub fn rename_account(&self, id: Uuid, new_name: &str) -> Result<Account, AccountWriteError> {
        let updated = self.conn.execute(
            "UPDATE accounts SET name = ?2 WHERE id = ?1",
            rusqlite::params![id.to_string(), new_name],
        )?;
        if updated == 0 {
            return Err(AccountWriteError::NotFound(id));
        }
        self.get_account_by_id(id)?.ok_or(AccountWriteError::NotFound(id))
    }

    pub fn close_account(&self, id: Uuid) -> Result<Account, AccountWriteError> {
        let updated = self.conn.execute(
            "UPDATE accounts SET is_closed = 1 WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )?;
        if updated == 0 {
            return Err(AccountWriteError::NotFound(id));
        }
        self.get_account_by_id(id)?.ok_or(AccountWriteError::NotFound(id))
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

    #[cfg(test)]
    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    fn get_account_by_id(&self, id: Uuid) -> Result<Option<Account>, AccountWriteError> {
        let mut stmt = self.conn.prepare(GET_ACCOUNT_BY_ID_SQL)?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => account_from_row(row).map(Some).map_err(AccountWriteError::ReadBack),
            None => Ok(None),
        }
    }
}

fn account_from_row(row: &rusqlite::Row<'_>) -> Result<Account, AccountListError> {
    let id_str: String = row.get("id")?;
    let parent_id_str: Option<String> = row.get("parent_id")?;
    let is_closed: i64 = row.get("is_closed")?;

    let id = Uuid::parse_str(&id_str).map_err(|source| AccountListError::InvalidId {
        value: id_str.clone(),
        source,
    })?;
    let parent_id = parent_id_str
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|source| AccountListError::InvalidParentId {
            value: parent_id_str.clone().unwrap_or_default(),
            source,
        })?;

    Ok(Account {
        id,
        parent_id,
        name: row.get("name")?,
        currency: row.get("currency")?,
        is_closed: is_closed != 0,
        created_at: row.get("created_at")?,
        note: row.get("note")?,
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
        assert_eq!(applied_count, 2);

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
        assert_eq!(applied_count, 2);
    }

    #[test]
    fn create_account_inserts_and_returns_account() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let id = Uuid::parse_str("66666666-6666-6666-6666-666666666666").unwrap();

        let account = db
            .create_account(id, None, "cash", "USD", Some("wallet"))
            .expect("create account");

        assert_eq!(account.id, id);
        assert_eq!(account.parent_id, None);
        assert_eq!(account.name, "cash");
        assert_eq!(account.currency, "USD");
        assert!(!account.is_closed);
        assert_eq!(account.note.as_deref(), Some("wallet"));
        assert!(!account.created_at.is_empty());
    }

    #[test]
    fn rename_account_updates_name() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let id = Uuid::parse_str("77777777-7777-7777-7777-777777777777").unwrap();
        db.create_account(id, None, "old-name", "USD", None)
            .expect("create account");

        let renamed = db.rename_account(id, "new-name").expect("rename account");

        assert_eq!(renamed.name, "new-name");
        assert_eq!(renamed.id, id);
    }

    #[test]
    fn rename_account_returns_not_found_for_missing_id() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let missing = Uuid::parse_str("88888888-8888-8888-8888-888888888888").unwrap();

        let err = db
            .rename_account(missing, "new-name")
            .expect_err("rename should fail");

        assert!(matches!(err, AccountWriteError::NotFound(id) if id == missing));
    }

    #[test]
    fn close_account_sets_is_closed() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let id = Uuid::parse_str("99999999-9999-9999-9999-999999999999").unwrap();
        db.create_account(id, None, "card", "USD", None)
            .expect("create account");

        let closed = db.close_account(id).expect("close account");

        assert!(closed.is_closed);
        assert_eq!(closed.id, id);
    }

    #[test]
    fn close_account_returns_not_found_for_missing_id() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let missing = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap();

        let err = db.close_account(missing).expect_err("close should fail");

        assert!(matches!(err, AccountWriteError::NotFound(id) if id == missing));
    }
}
