use std::fmt::{Display, Formatter};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub sql: String,
}

#[derive(Debug)]
pub enum MigrationParseError {
    Read(std::io::Error),
    InvalidExtension,
    InvalidFilename,
    InvalidVersion(std::num::ParseIntError),
}

impl Display for MigrationParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(err) => write!(f, "failed to read migration file: {err}"),
            Self::InvalidExtension => write!(f, "migration file extension must be .sql"),
            Self::InvalidFilename => {
                write!(f, "migration filename must be <VERSION>_<NAME>.sql")
            }
            Self::InvalidVersion(err) => write!(f, "invalid migration version: {err}"),
        }
    }
}

impl std::error::Error for MigrationParseError {}

#[derive(Debug)]
pub enum MigrationRunnerError {
    Parse(MigrationParseError),
    Sql(rusqlite::Error),
}

impl Display for MigrationRunnerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "failed to parse migration file: {err}"),
            Self::Sql(err) => write!(f, "sqlite error while running migrations: {err}"),
        }
    }
}

impl std::error::Error for MigrationRunnerError {}

impl From<rusqlite::Error> for MigrationRunnerError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl From<MigrationParseError> for MigrationRunnerError {
    fn from(value: MigrationParseError) -> Self {
        Self::Parse(value)
    }
}

pub struct MigrationRunner<'conn> {
    conn: &'conn rusqlite::Connection,
}

impl Migration {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, MigrationParseError> {
        let path = path.as_ref();
        let is_sql = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("sql"))
            .unwrap_or(false);
        if !is_sql {
            return Err(MigrationParseError::InvalidExtension);
        }

        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or(MigrationParseError::InvalidFilename)?;
        let (version_str, name) = stem
            .split_once('_')
            .ok_or(MigrationParseError::InvalidFilename)?;
        if version_str.is_empty() || name.is_empty() {
            return Err(MigrationParseError::InvalidFilename);
        }

        // Parse u32 directly so zero-padded versions are naturally accepted.
        let version = version_str
            .parse::<u32>()
            .map_err(MigrationParseError::InvalidVersion)?;
        let sql = std::fs::read_to_string(path).map_err(MigrationParseError::Read)?;

        Ok(Self {
            version,
            name: name.to_string(),
            sql,
        })
    }

    pub fn from_files<P>(
        paths: impl IntoIterator<Item = P>,
    ) -> impl Iterator<Item = Result<Self, MigrationParseError>>
    where
        P: AsRef<Path>,
    {
        paths.into_iter().map(Self::from_file)
    }
}

impl<'conn> MigrationRunner<'conn> {
    pub fn new(conn: &'conn rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn run(
        &self,
        migrations: impl IntoIterator<Item = Result<Migration, MigrationParseError>>,
    ) -> Result<(), MigrationRunnerError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            ",
        )?;

        for migration in migrations {
            let _migration = migration.map_err(MigrationRunnerError::from)?;
            // Migration application is not implemented yet.
        }

        Ok(())
    }

    pub fn run_from_files<P>(
        &self,
        paths: impl IntoIterator<Item = P>,
    ) -> Result<(), MigrationRunnerError>
    where
        P: AsRef<Path>,
    {
        self.run(Migration::from_files(paths))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn from_file_parses_zero_padded_version() {
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.sql");
        let sql = "CREATE TABLE accounts(id INTEGER PRIMARY KEY);";
        std::fs::write(&path, sql).expect("write migration");

        let migration = Migration::from_file(&path).expect("parse migration");

        assert_eq!(migration.version, 1);
        assert_eq!(migration.name, "create_accounts");
        assert_eq!(migration.sql, sql);
    }

    #[test]
    fn from_file_rejects_non_sql_extension() {
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.txt");
        std::fs::write(&path, "SELECT 1;").expect("write migration");

        let err =
            Migration::from_file(&path).expect_err("non-sql migration extension should fail");

        assert!(matches!(err, MigrationParseError::InvalidExtension));
    }

    #[test]
    fn run_creates_schema_migrations_table_and_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);

        runner
            .run(std::iter::empty::<Result<Migration, MigrationParseError>>())
            .expect("first run should succeed");
        runner
            .run(std::iter::empty::<Result<Migration, MigrationParseError>>())
            .expect("second run should also succeed");

        let table_name: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
                [],
                |row| row.get(0),
            )
            .expect("schema_migrations table should exist");

        assert_eq!(table_name, "schema_migrations");
    }

    #[test]
    fn run_from_files_parses_and_runs() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.sql");
        std::fs::write(&path, "CREATE TABLE accounts(id INTEGER PRIMARY KEY);")
            .expect("write migration");

        runner
            .run_from_files([path.as_path()])
            .expect("run_from_files should succeed");

        let table_name: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
                [],
                |row| row.get(0),
            )
            .expect("schema_migrations table should exist");

        assert_eq!(table_name, "schema_migrations");
    }

    #[test]
    fn run_from_files_returns_parse_error() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.txt");
        std::fs::write(&path, "SELECT 1;").expect("write migration");

        let err = runner
            .run_from_files([path.as_path()])
            .expect_err("non-sql file should fail parsing");

        assert!(matches!(
            err,
            MigrationRunnerError::Parse(MigrationParseError::InvalidExtension)
        ));
    }
}
