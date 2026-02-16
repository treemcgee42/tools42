use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub file_path: PathBuf,
}

#[derive(Debug)]
pub enum MigrationParseError {
    InvalidExtension,
    InvalidFilename,
    InvalidVersion(std::num::ParseIntError),
}

impl Display for MigrationParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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
pub enum MigrationDiscoveryError {
    Io(std::io::Error),
    Parse(MigrationParseError),
    DuplicateVersion(u32),
}

impl Display for MigrationDiscoveryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to discover migrations from directory: {err}"),
            Self::Parse(err) => write!(f, "failed to parse migration file: {err}"),
            Self::DuplicateVersion(version) => {
                write!(f, "duplicate migration version found: {version}")
            }
        }
    }
}

impl std::error::Error for MigrationDiscoveryError {}

impl From<std::io::Error> for MigrationDiscoveryError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<MigrationParseError> for MigrationDiscoveryError {
    fn from(value: MigrationParseError) -> Self {
        Self::Parse(value)
    }
}

#[derive(Debug)]
pub enum MigrationRunnerError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
}

impl Display for MigrationRunnerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read migration sql: {err}"),
            Self::Sql(err) => write!(f, "sqlite error while running migrations: {err}"),
        }
    }
}

impl std::error::Error for MigrationRunnerError {}

impl From<std::io::Error> for MigrationRunnerError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for MigrationRunnerError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
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
        Ok(Self {
            version,
            name: name.to_string(),
            file_path: path.to_path_buf(),
        })
    }

    pub fn sql(&self) -> Result<String, std::io::Error> {
        std::fs::read_to_string(&self.file_path)
    }

    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Vec<Self>, MigrationDiscoveryError> {
        let mut migrations = Vec::new();

        for entry in std::fs::read_dir(dir).map_err(MigrationDiscoveryError::from)? {
            let entry = entry.map_err(MigrationDiscoveryError::from)?;
            let path = entry.path();
            let is_sql = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("sql"))
                .unwrap_or(false);
            if !is_sql {
                continue;
            }

            let migration = Self::from_file(&path).map_err(MigrationDiscoveryError::from)?;
            migrations.push(migration);
        }

        migrations.sort_by(|a, b| {
            a.version
                .cmp(&b.version)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.file_path.cmp(&b.file_path))
        });

        for pair in migrations.windows(2) {
            if pair[0].version == pair[1].version {
                return Err(MigrationDiscoveryError::DuplicateVersion(pair[0].version));
            }
        }

        Ok(migrations)
    }
}

impl<'conn> MigrationRunner<'conn> {
    pub fn new(conn: &'conn rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn run(&self, migrations: &[Migration]) -> Result<(), MigrationRunnerError> {
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
            let already_applied = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [migration.version],
                |row| row.get::<_, i64>(0),
            )? != 0;
            if already_applied {
                continue;
            }

            let sql = migration.sql().map_err(MigrationRunnerError::from)?;
            self.conn.execute_batch(&sql)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                rusqlite::params![migration.version, migration.name],
            )?;
        }

        Ok(())
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
        assert_eq!(migration.file_path, path);
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
    fn sql_reads_on_demand() {
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.sql");
        let sql = "CREATE TABLE accounts(id INTEGER PRIMARY KEY);";
        std::fs::write(&path, sql).expect("write migration");

        let migration = Migration::from_file(&path).expect("parse migration");
        let loaded_sql = migration.sql().expect("read migration sql");

        assert_eq!(loaded_sql, sql);
    }

    #[test]
    fn from_dir_returns_sorted_migrations() {
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();
        std::fs::write(dir.join("0010_ten.sql"), "SELECT 10;").expect("write migration");
        std::fs::write(dir.join("0002_two.sql"), "SELECT 2;").expect("write migration");
        std::fs::write(dir.join("0001_one.sql"), "SELECT 1;").expect("write migration");

        let migrations = Migration::from_dir(dir).expect("discover migrations");
        let versions: Vec<u32> = migrations.into_iter().map(|m| m.version).collect();

        assert_eq!(versions, vec![1, 2, 10]);
    }

    #[test]
    fn from_dir_fails_on_invalid_sql_filename() {
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();
        std::fs::write(dir.join("not-a-migration.sql"), "SELECT 1;").expect("write migration");

        let err = Migration::from_dir(dir).expect_err("invalid migration filename should fail");

        assert!(matches!(
            err,
            MigrationDiscoveryError::Parse(MigrationParseError::InvalidFilename)
        ));
    }

    #[test]
    fn from_dir_fails_on_duplicate_version() {
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();
        std::fs::write(dir.join("0001_first.sql"), "SELECT 1;").expect("write migration");
        std::fs::write(dir.join("1_second.sql"), "SELECT 2;").expect("write migration");

        let err = Migration::from_dir(dir).expect_err("duplicate versions should fail");

        assert!(matches!(err, MigrationDiscoveryError::DuplicateVersion(1)));
    }

    #[test]
    fn run_creates_schema_migrations_table_and_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);

        runner.run(&[]).expect("first run should succeed");
        runner.run(&[]).expect("second run should also succeed");

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
    fn run_applies_new_migrations_and_records_them() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();

        std::fs::write(
            dir.join("0001_create_accounts.sql"),
            "CREATE TABLE accounts(id INTEGER PRIMARY KEY);",
        )
        .expect("write migration");
        std::fs::write(
            dir.join("0002_create_transactions.sql"),
            "CREATE TABLE transactions(id INTEGER PRIMARY KEY, account_id INTEGER NOT NULL);",
        )
        .expect("write migration");

        let migrations = Migration::from_dir(dir).expect("discover migrations");
        runner.run(&migrations).expect("run migrations");

        let applied_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 2);

        let accounts_exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='accounts')",
                [],
                |row| row.get(0),
            )
            .expect("check accounts table");
        assert_eq!(accounts_exists, 1);

        let transactions_exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='transactions')",
                [],
                |row| row.get(0),
            )
            .expect("check transactions table");
        assert_eq!(transactions_exists, 1);
    }

    #[test]
    fn run_is_idempotent_for_applied_migrations() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();

        std::fs::write(
            dir.join("0001_create_accounts.sql"),
            "CREATE TABLE accounts(id INTEGER PRIMARY KEY);",
        )
        .expect("write migration");
        let migrations = Migration::from_dir(dir).expect("discover migrations");

        runner.run(&migrations).expect("first run");
        runner.run(&migrations).expect("second run");

        let applied_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 1);
    }
}
