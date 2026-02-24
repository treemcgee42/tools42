use super::db::Db;
use super::user_data::{UserDataError, UserDataManager};
use std::fmt::{Display, Formatter};
use std::path::Path;

pub struct Core {
    _user_data: UserDataManager,
    _db: Db,
}

#[derive(Debug)]
pub enum CoreError {
    UserData(UserDataError),
}

impl Display for CoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserData(err) => write!(f, "failed to initialize core: {err}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UserData(err) => Some(err),
        }
    }
}

impl From<UserDataError> for CoreError {
    fn from(value: UserDataError) -> Self {
        Self::UserData(value)
    }
}

impl Core {
    pub fn from_environment() -> Result<Self, CoreError> {
        let user_data = UserDataManager::from_environment()?;
        Self::from_user_data(user_data)
    }

    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Result<Self, CoreError> {
        let user_data = UserDataManager::from_data_dir(data_dir);
        Self::from_user_data(user_data)
    }

    pub fn init(&self) -> Result<(), CoreError> {
        Ok(())
    }

    pub(super) fn db_mut(&mut self) -> &mut Db {
        &mut self._db
    }

    #[cfg(test)]
    pub(super) fn open_for_tests() -> Result<Self, CoreError> {
        let user_data = UserDataManager::from_data_dir(std::env::temp_dir().join("tally42-tests"));
        let db = Db::open_for_tests().map_err(UserDataError::OpenDb)?;
        Ok(Self {
            _user_data: user_data,
            _db: db,
        })
    }

    fn from_user_data(user_data: UserDataManager) -> Result<Self, CoreError> {
        let db = user_data.open_db()?;
        Ok(Self {
            _user_data: user_data,
            _db: db,
        })
    }
}
