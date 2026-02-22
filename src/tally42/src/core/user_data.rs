use super::db::{Db, DbError};
use super::statement::{AddStatementError, AddStatementInput, Statement};
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const APP_DIR_NAME: &str = "tally42";
const DB_FILE_NAME: &str = "tally42.db";
const STATEMENTS_DIR_NAME: &str = "statements";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDataManager {
    data_dir: PathBuf,
    db_path: PathBuf,
}

#[derive(Debug)]
pub enum UserDataError {
    MissingHomeDir,
    CreateDataDir(std::io::Error),
    DeleteDatabase(std::io::Error),
    OpenDb(DbError),
}

impl Display for UserDataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHomeDir => write!(
                f,
                "could not resolve user data directory: HOME is not set and XDG_DATA_HOME is absent"
            ),
            Self::CreateDataDir(err) => write!(f, "failed to create data directory: {err}"),
            Self::DeleteDatabase(err) => write!(f, "failed to delete sqlite database: {err}"),
            Self::OpenDb(err) => write!(f, "failed to initialize sqlite database: {err}"),
        }
    }
}

impl std::error::Error for UserDataError {}

impl UserDataManager {
    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        let db_path = data_dir.join(DB_FILE_NAME);
        Self { data_dir, db_path }
    }

    pub fn from_environment() -> Result<Self, UserDataError> {
        let data_dir = resolve_default_data_dir()?;
        Ok(Self::from_data_dir(data_dir))
    }

    pub fn init(&self) -> Result<(), UserDataError> {
        let _db = self.open_db()?;
        Ok(())
    }

    pub fn open_db(&self) -> Result<Db, UserDataError> {
        std::fs::create_dir_all(&self.data_dir).map_err(UserDataError::CreateDataDir)?;
        std::fs::create_dir_all(self.statements_dir()).map_err(UserDataError::CreateDataDir)?;
        Db::open(&self.db_path).map_err(UserDataError::OpenDb)
    }

    pub fn add_statement(
        &self,
        source_path: impl AsRef<Path>,
        input: AddStatementInput,
    ) -> Result<Statement, AddStatementError> {
        let source_path = source_path.as_ref();
        let db = self.open_db().map_err(AddStatementError::PrepareUserData)?;
        let statements_dir = self.statements_dir();

        let mut source = std::fs::File::open(source_path).map_err(AddStatementError::OpenSource)?;
        let temp_path = statements_dir.join(format!(".tmp-statement-{}", Uuid::new_v4()));
        let mut temp_file =
            std::fs::File::create(&temp_path).map_err(AddStatementError::CreateTempFile)?;

        let mut hasher = Sha256::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = source.read(&mut buf).map_err(AddStatementError::ReadSource)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            temp_file
                .write_all(&buf[..n])
                .map_err(AddStatementError::WriteTempFile)?;
        }
        temp_file.flush().map_err(AddStatementError::WriteTempFile)?;

        let file_size_u64 = temp_file
            .metadata()
            .map_err(AddStatementError::TempFileMetadata)?
            .len();
        let file_size = i64::try_from(file_size_u64)
            .map_err(|_| AddStatementError::FileTooLarge(file_size_u64))?;
        let file_hash = format!("{:x}", hasher.finalize());
        let final_path = self.statement_file_path_for_source(&file_hash, source_path);
        drop(temp_file);

        let duplicate_path = self.find_statement_file_path(&file_hash);
        if let Some(existing_path) = duplicate_path {
            let _ = std::fs::remove_file(&temp_path);
            return Err(AddStatementError::DuplicateFileHash {
                hash: file_hash,
                path: existing_path,
            });
        }

        std::fs::rename(&temp_path, &final_path).map_err(AddStatementError::RenameToFinal)?;

        let statement_id = Uuid::new_v4();
        let insert_result = db.create_statement(
            statement_id,
            &input.institution,
            input.account_id,
            &input.period_start,
            &input.period_end,
            &input.currency,
            &file_hash,
            file_size,
            input.replaced_by,
        );

        match insert_result {
            Ok(statement) => Ok(statement),
            Err(insert_error) => match std::fs::remove_file(&final_path) {
                Ok(()) => Err(AddStatementError::InsertStatement(insert_error)),
                Err(cleanup_error) => Err(AddStatementError::InsertStatementCleanupFailed {
                    insert_error,
                    cleanup_error,
                    path: final_path,
                }),
            },
        }
    }

    pub fn delete_db(&self) -> Result<bool, UserDataError> {
        match std::fs::remove_file(&self.db_path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(UserDataError::DeleteDatabase(err)),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn statements_dir(&self) -> PathBuf {
        self.data_dir.join(STATEMENTS_DIR_NAME)
    }

    pub fn statement_file_path(&self, file_hash: &str) -> PathBuf {
        self.find_statement_file_path(file_hash)
            .unwrap_or_else(|| self.statements_dir().join(file_hash))
    }

    fn statement_file_path_for_source(&self, file_hash: &str, source_path: &Path) -> PathBuf {
        match source_path.extension() {
            Some(ext) if !ext.is_empty() => self
                .statements_dir()
                .join(format!("{file_hash}.{}", ext.to_string_lossy())),
            _ => self.statements_dir().join(file_hash),
        }
    }

    fn find_statement_file_path(&self, file_hash: &str) -> Option<PathBuf> {
        let exact = self.statements_dir().join(file_hash);
        if exact.exists() {
            return Some(exact);
        }

        let entries = std::fs::read_dir(self.statements_dir()).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let stem_matches = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s == file_hash)
                .unwrap_or(false);
            if stem_matches {
                return Some(path);
            }
        }
        None
    }
}

fn resolve_default_data_dir() -> Result<PathBuf, UserDataError> {
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        return Ok(PathBuf::from(xdg_data_home).join(APP_DIR_NAME));
    }

    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".local").join("share").join(APP_DIR_NAME));
    }

    Err(UserDataError::MissingHomeDir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::statement::StatementWriteError;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    #[test]
    fn init_creates_data_dir_and_db_file() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("nested").join("state");
        let manager = UserDataManager::from_data_dir(&data_dir);

        manager.init().expect("initialize user data");

        assert!(manager.data_dir().is_dir());
        assert!(manager.statements_dir().is_dir());
        assert!(manager.db_path().is_file());
        assert_eq!(manager.db_path(), data_dir.join(DB_FILE_NAME));
    }

    #[test]
    fn init_is_idempotent() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("state");
        let manager = UserDataManager::from_data_dir(&data_dir);

        manager.init().expect("first init");
        manager.init().expect("second init");

        assert!(manager.data_dir().is_dir());
        assert!(manager.statements_dir().is_dir());
        assert!(manager.db_path().is_file());
    }

    #[test]
    fn delete_db_removes_existing_file() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("state");
        let manager = UserDataManager::from_data_dir(&data_dir);
        manager.init().expect("init db");
        assert!(manager.db_path().is_file());

        let deleted = manager.delete_db().expect("delete db");

        assert!(deleted);
        assert!(!manager.db_path().exists());
    }

    #[test]
    fn delete_db_is_idempotent_when_missing() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("state");
        let manager = UserDataManager::from_data_dir(&data_dir);

        let deleted = manager.delete_db().expect("delete missing db");

        assert!(!deleted);
    }

    #[test]
    fn open_db_returns_migrated_database() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("state");
        let manager = UserDataManager::from_data_dir(&data_dir);

        let db = manager.open_db().expect("open db");

        let applied_count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .expect("count applied migrations");
        assert_eq!(applied_count, 3);
        assert!(manager.db_path().is_file());
        assert!(manager.statements_dir().is_dir());
    }

    fn write_test_file(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).expect("write test statement file");
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn sample_add_input(account_id: Uuid) -> AddStatementInput {
        AddStatementInput {
            institution: "Chase".to_string(),
            account_id,
            period_start: "2026-01-01".to_string(),
            period_end: "2026-01-31".to_string(),
            currency: "USD".to_string(),
            replaced_by: None,
        }
    }

    #[test]
    fn add_statement_copies_file_and_inserts_db_row() {
        let temp_dir = tempdir().expect("create temp dir");
        let manager = UserDataManager::from_data_dir(temp_dir.path().join("state"));
        let source_path = temp_dir.path().join("statement.pdf");
        let bytes = b"%PDF-1.7 sample";
        write_test_file(&source_path, bytes);

        let account_id = Uuid::parse_str("21212121-2121-2121-2121-212121212121").unwrap();
        let db = manager.open_db().expect("open db");
        db.create_account(account_id, None, "checking", "USD", None)
            .expect("create account");
        drop(db);

        let created = manager
            .add_statement(&source_path, sample_add_input(account_id))
            .expect("add statement");

        let expected_hash = sha256_hex(bytes);
        let stored_path = manager.statement_file_path(&expected_hash);
        assert_eq!(created.file_hash, expected_hash);
        assert_eq!(created.file_size, bytes.len() as i64);
        assert!(stored_path.is_file());
        assert_eq!(
            stored_path.extension().and_then(|e| e.to_str()),
            Some("pdf")
        );
        assert_eq!(std::fs::read(&stored_path).expect("read stored file"), bytes);

        let db = manager.open_db().expect("reopen db");
        let statements = db.list_statements().expect("list statements");
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].id, created.id);
    }

    #[test]
    fn add_statement_fails_on_duplicate_hash_without_overwriting() {
        let temp_dir = tempdir().expect("create temp dir");
        let manager = UserDataManager::from_data_dir(temp_dir.path().join("state"));
        let source_path = temp_dir.path().join("statement.pdf");
        let bytes = b"duplicate bytes";
        write_test_file(&source_path, bytes);

        let account_id = Uuid::parse_str("22222222-3333-4444-5555-666666666666").unwrap();
        let db = manager.open_db().expect("open db");
        db.create_account(account_id, None, "checking", "USD", None)
            .expect("create account");
        drop(db);

        let first = manager
            .add_statement(&source_path, sample_add_input(account_id))
            .expect("first add");
        let err = manager
            .add_statement(&source_path, sample_add_input(account_id))
            .expect_err("second add should fail");

        let expected_hash = sha256_hex(bytes);
        let stored_path = manager.statement_file_path(&expected_hash);
        assert!(matches!(
            err,
            AddStatementError::DuplicateFileHash { ref hash, .. } if hash == &expected_hash
        ));
        assert!(stored_path.is_file());
        assert_eq!(std::fs::read(&stored_path).expect("read stored file"), bytes);

        let db = manager.open_db().expect("reopen db");
        let statements = db.list_statements().expect("list statements");
        assert_eq!(statements.len(), 1);
        assert_eq!(statements[0].id, first.id);
    }

    #[test]
    fn add_statement_rolls_back_copied_file_if_db_insert_fails() {
        let temp_dir = tempdir().expect("create temp dir");
        let manager = UserDataManager::from_data_dir(temp_dir.path().join("state"));
        let source_path = temp_dir.path().join("statement.pdf");
        let bytes = b"fk failure rollback";
        write_test_file(&source_path, bytes);
        let expected_hash = sha256_hex(bytes);

        let missing_account_id = Uuid::parse_str("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap();
        let err = manager
            .add_statement(&source_path, sample_add_input(missing_account_id))
            .expect_err("add should fail on missing account FK");

        assert!(matches!(
            err,
            AddStatementError::InsertStatement(StatementWriteError::Sql(_))
                | AddStatementError::InsertStatementCleanupFailed { .. }
        ));
        assert!(!manager.statement_file_path(&expected_hash).exists());

        let db = manager.open_db().expect("open db");
        let statements = db.list_statements().expect("list statements");
        assert!(statements.is_empty());
    }
}
