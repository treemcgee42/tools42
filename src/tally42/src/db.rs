use crate::account::{Account, AccountListError};
use crate::migration::{
    Migration, MigrationDiscoveryError, MigrationRunner, MigrationRunnerError, MigrationsDir,
};
use std::fmt::{Display, Formatter};
use std::path::Path;

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
        Account::list_accounts(&self.conn)
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
}
