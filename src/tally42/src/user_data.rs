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
    OpenDatabase(rusqlite::Error),
}

impl Display for UserDataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHomeDir => write!(
                f,
                "could not resolve user data directory: HOME is not set and XDG_DATA_HOME is absent"
            ),
            Self::CreateDataDir(err) => write!(f, "failed to create data directory: {err}"),
            Self::OpenDatabase(err) => write!(f, "failed to open sqlite database: {err}"),
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
}
