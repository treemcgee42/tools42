use crate::db::{Db, DbError};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

const APP_DIR_NAME: &str = "tally42";
const DB_FILE_NAME: &str = "tally42.db";

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
    OpenDatabase(rusqlite::Error),
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
            Self::OpenDatabase(err) => write!(f, "failed to open sqlite database: {err}"),
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
        std::fs::create_dir_all(&self.data_dir).map_err(UserDataError::CreateDataDir)?;
        // `Connection::open` creates the sqlite file if it does not exist yet.
        // The temporary connection is closed automatically when it is dropped
        // at the end of this function scope.
        rusqlite::Connection::open(&self.db_path).map_err(UserDataError::OpenDatabase)?;
        Ok(())
    }

    pub fn open_db(&self) -> Result<Db, UserDataError> {
        std::fs::create_dir_all(&self.data_dir).map_err(UserDataError::CreateDataDir)?;
        Db::open(&self.db_path).map_err(UserDataError::OpenDb)
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
    use tempfile::tempdir;

    #[test]
    fn init_creates_data_dir_and_db_file() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("nested").join("state");
        let manager = UserDataManager::from_data_dir(&data_dir);

        manager.init().expect("initialize user data");

        assert!(manager.data_dir().is_dir());
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
        assert_eq!(applied_count, 2);
        assert!(manager.db_path().is_file());
    }
}
