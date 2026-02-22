use include_dir::{include_dir, Dir};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

pub static EMBEDDED_MIGRATIONS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

pub enum MigrationsDir {
    Embedded(&'static Dir<'static>),
    Fs(PathBuf),
}

impl MigrationsDir {
    pub fn embedded() -> Self {
        Self::Embedded(&EMBEDDED_MIGRATIONS_DIR)
    }

    pub fn fs(path: impl AsRef<Path>) -> Self {
        Self::Fs(path.as_ref().to_path_buf())
    }

    pub fn migration_files(&self) -> Result<Vec<String>, MigrationDiscoveryError> {
        match self {
            Self::Embedded(dir) => {
                let mut files = Vec::new();
                for file in dir.files() {
                    let path_str = file
                        .path()
                        .to_str()
                        .ok_or(MigrationDiscoveryError::InvalidUtf8FileName)?;
                    if !path_str.ends_with(".sql") {
                        continue;
                    }
                    files.push(path_str.to_string());
                }
                Ok(files)
            }
            Self::Fs(base_dir) => {
                let mut files = Vec::new();
                for entry in std::fs::read_dir(base_dir).map_err(MigrationDiscoveryError::Io)? {
                    let entry = entry.map_err(MigrationDiscoveryError::Io)?;
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let is_sql = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("sql"))
                        .unwrap_or(false);
                    if !is_sql {
                        continue;
                    }
                    let file_name = entry
                        .file_name()
                        .into_string()
                        .map_err(|_| MigrationDiscoveryError::InvalidUtf8FileName)?;
                    files.push(file_name);
                }
                Ok(files)
            }
        }
    }

    pub fn read_file_utf8(&self, file_name: &str) -> Result<String, MigrationContentError> {
        match self {
            Self::Embedded(dir) => {
                let file = dir
                    .get_file(file_name)
                    .ok_or_else(|| MigrationContentError::MissingEmbeddedFile(file_name.into()))?;
                let content = file.contents_utf8().ok_or_else(|| {
                    MigrationContentError::NonUtf8EmbeddedFile(file_name.into())
                })?;
                Ok(content.to_string())
            }
            Self::Fs(base_dir) => {
                let path = base_dir.join(file_name);
                std::fs::read_to_string(path).map_err(MigrationContentError::Io)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub file_name: String,
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
    InvalidUtf8FileName,
}

impl Display for MigrationDiscoveryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to discover migrations from source: {err}"),
            Self::Parse(err) => write!(f, "failed to parse migration filename: {err}"),
            Self::DuplicateVersion(version) => {
                write!(f, "duplicate migration version found: {version}")
            }
            Self::InvalidUtf8FileName => {
                write!(f, "migration file name must be valid utf-8")
            }
        }
    }
}

impl std::error::Error for MigrationDiscoveryError {}

impl From<MigrationParseError> for MigrationDiscoveryError {
    fn from(value: MigrationParseError) -> Self {
        Self::Parse(value)
    }
}

#[derive(Debug)]
pub enum MigrationContentError {
    Io(std::io::Error),
    MissingEmbeddedFile(String),
    NonUtf8EmbeddedFile(String),
}

impl Display for MigrationContentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read migration sql content: {err}"),
            Self::MissingEmbeddedFile(file_name) => {
                write!(f, "embedded migration file not found: {file_name}")
            }
            Self::NonUtf8EmbeddedFile(file_name) => {
                write!(f, "embedded migration file is not valid utf-8: {file_name}")
            }
        }
    }
}

impl std::error::Error for MigrationContentError {}

#[derive(Debug)]
pub enum MigrationRunnerError {
    Content(MigrationContentError),
    Sql(rusqlite::Error),
}

impl Display for MigrationRunnerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content(err) => write!(f, "failed to load migration content: {err}"),
            Self::Sql(err) => write!(f, "sqlite error while running migrations: {err}"),
        }
    }
}

impl std::error::Error for MigrationRunnerError {}

impl From<MigrationContentError> for MigrationRunnerError {
    fn from(value: MigrationContentError) -> Self {
        Self::Content(value)
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
    pub fn from_file_name(file_name: &str) -> Result<Self, MigrationParseError> {
        let path = Path::new(file_name);
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

        let version = version_str
            .parse::<u32>()
            .map_err(MigrationParseError::InvalidVersion)?;
        Ok(Self {
            version,
            name: name.to_string(),
            file_name: file_name.to_string(),
        })
    }

    pub fn from_source(source: &MigrationsDir) -> Result<Vec<Self>, MigrationDiscoveryError> {
        let mut migrations = Vec::new();
        for file_name in source.migration_files()? {
            migrations.push(Self::from_file_name(&file_name)?);
        }

        migrations.sort_by(|a, b| {
            a.version
                .cmp(&b.version)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.file_name.cmp(&b.file_name))
        });

        for pair in migrations.windows(2) {
            if pair[0].version == pair[1].version {
                return Err(MigrationDiscoveryError::DuplicateVersion(pair[0].version));
            }
        }

        Ok(migrations)
    }

    pub fn sql(&self, source: &MigrationsDir) -> Result<String, MigrationContentError> {
        source.read_file_utf8(&self.file_name)
    }
}

impl<'conn> MigrationRunner<'conn> {
    pub fn new(conn: &'conn rusqlite::Connection) -> Self {
        Self { conn }
    }

    pub fn run(
        &self,
        source: &MigrationsDir,
        migrations: &[Migration],
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
            let already_applied = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [migration.version],
                |row| row.get::<_, i64>(0),
            )? != 0;
            if already_applied {
                continue;
            }

            let sql = migration.sql(source)?;
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
    fn embedded_source_lists_seed_file() {
        let source = MigrationsDir::embedded();
        let files = source.migration_files().expect("list embedded migration files");
        assert!(files.contains(&"0001_add_accounts_table.sql".to_string()));
    }

    #[test]
    fn from_file_name_parses_zero_padded_version() {
        let migration = Migration::from_file_name("0001_create_accounts.sql")
            .expect("parse migration file name");
        assert_eq!(migration.version, 1);
        assert_eq!(migration.name, "create_accounts");
        assert_eq!(migration.file_name, "0001_create_accounts.sql");
    }

    #[test]
    fn from_file_name_rejects_non_sql_extension() {
        let err = Migration::from_file_name("0001_create_accounts.txt")
            .expect_err("non-sql migration extension should fail");
        assert!(matches!(err, MigrationParseError::InvalidExtension));
    }

    #[test]
    fn sql_reads_on_demand_from_fs_source() {
        let temp_dir = tempdir().expect("create temp dir");
        let path = temp_dir.path().join("0001_create_accounts.sql");
        let sql = "CREATE TABLE accounts(id INTEGER PRIMARY KEY);";
        std::fs::write(&path, sql).expect("write migration");

        let source = MigrationsDir::fs(temp_dir.path());
        let migration = Migration::from_file_name("0001_create_accounts.sql")
            .expect("parse migration file name");
        let loaded_sql = migration.sql(&source).expect("read migration sql");

        assert_eq!(loaded_sql, sql);
    }

    #[test]
    fn sql_reads_from_embedded_source() {
        let source = MigrationsDir::embedded();
        let migration = Migration::from_file_name("0001_add_accounts_table.sql")
            .expect("parse migration file name");
        let loaded_sql = migration.sql(&source).expect("read embedded migration sql");

        assert!(loaded_sql.contains("CREATE TABLE accounts"));
    }

    #[test]
    fn from_source_returns_sorted_migrations() {
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();
        std::fs::write(dir.join("0010_ten.sql"), "SELECT 10;").expect("write migration");
        std::fs::write(dir.join("0002_two.sql"), "SELECT 2;").expect("write migration");
        std::fs::write(dir.join("0001_one.sql"), "SELECT 1;").expect("write migration");

        let source = MigrationsDir::fs(dir);
        let migrations = Migration::from_source(&source).expect("discover migrations");
        let versions: Vec<u32> = migrations.into_iter().map(|m| m.version).collect();

        assert_eq!(versions, vec![1, 2, 10]);
    }

    #[test]
    fn from_source_fails_on_invalid_sql_filename() {
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();
        std::fs::write(dir.join("not-a-migration.sql"), "SELECT 1;").expect("write migration");

        let source = MigrationsDir::fs(dir);
        let err = Migration::from_source(&source).expect_err("invalid migration filename");

        assert!(matches!(
            err,
            MigrationDiscoveryError::Parse(MigrationParseError::InvalidFilename)
        ));
    }

    #[test]
    fn from_source_fails_on_duplicate_version() {
        let temp_dir = tempdir().expect("create temp dir");
        let dir = temp_dir.path();
        std::fs::write(dir.join("0001_first.sql"), "SELECT 1;").expect("write migration");
        std::fs::write(dir.join("1_second.sql"), "SELECT 2;").expect("write migration");

        let source = MigrationsDir::fs(dir);
        let err = Migration::from_source(&source).expect_err("duplicate versions should fail");

        assert!(matches!(err, MigrationDiscoveryError::DuplicateVersion(1)));
    }

    #[test]
    fn run_creates_schema_migrations_table_and_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);
        let source = MigrationsDir::fs(tempdir().expect("create temp dir").path());

        runner.run(&source, &[]).expect("first run should succeed");
        runner.run(&source, &[]).expect("second run should succeed");

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

        let source = MigrationsDir::fs(dir);
        let migrations = Migration::from_source(&source).expect("discover migrations");
        runner.run(&source, &migrations).expect("run migrations");

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

        let source = MigrationsDir::fs(dir);
        let migrations = Migration::from_source(&source).expect("discover migrations");

        runner.run(&source, &migrations).expect("first run");
        runner.run(&source, &migrations).expect("second run");

        let applied_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 1);
    }

    #[test]
    fn run_applies_embedded_migrations() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite database");
        let runner = MigrationRunner::new(&conn);
        let source = MigrationsDir::embedded();
        let migrations = Migration::from_source(&source).expect("discover embedded migrations");

        runner.run(&source, &migrations).expect("run embedded migrations");

        let applied_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 3);

        let accounts_exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='accounts')",
                [],
                |row| row.get(0),
            )
            .expect("check accounts table");
        assert_eq!(accounts_exists, 1);
    }
}
