use super::account::AccountWriteError;
use super::db::Db;
use super::{Account, AccountListError};
use super::user_data::{UserDataError, UserDataManager};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct Core {
    _user_data: UserDataManager,
    _db: Db,
}

#[derive(Debug)]
pub enum CoreError {
    UserData(UserDataError),
    AccountList(AccountListError),
    AccountWrite(AccountWriteError),
}

impl Display for CoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserData(err) => write!(f, "failed to initialize core: {err}"),
            Self::AccountList(err) => write!(f, "failed to list accounts: {err}"),
            Self::AccountWrite(err) => write!(f, "failed to create account: {err}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UserData(err) => Some(err),
            Self::AccountList(err) => Some(err),
            Self::AccountWrite(err) => Some(err),
        }
    }
}

impl From<UserDataError> for CoreError {
    fn from(value: UserDataError) -> Self {
        Self::UserData(value)
    }
}

impl From<AccountListError> for CoreError {
    fn from(value: AccountListError) -> Self {
        Self::AccountList(value)
    }
}

impl From<AccountWriteError> for CoreError {
    fn from(value: AccountWriteError) -> Self {
        Self::AccountWrite(value)
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

    pub fn db_path(&self) -> &Path {
        self._user_data.db_path()
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>, CoreError> {
        self._db.list_accounts().map_err(CoreError::from)
    }

    pub fn create_account(
        &self,
        name: &str,
        currency: &str,
        note: &str,
    ) -> Result<Account, CoreError> {
        self._db
            .create_account(Uuid::new_v4(), None, name, currency, Some(note))
            .map_err(CoreError::from)
    }

    pub fn delete_db_from_environment() -> Result<(PathBuf, bool), CoreError> {
        let user_data = UserDataManager::from_environment()?;
        let db_path = user_data.db_path().to_path_buf();
        let deleted = user_data.delete_db()?;
        Ok((db_path, deleted))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn list_accounts_delegates_to_db() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("state");
        let core = Core::from_data_dir(&data_dir).expect("open core");

        let account_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        core._db
            .create_account(account_id, None, "checking", "USD", None)
            .expect("create account");

        let accounts = core.list_accounts().expect("list accounts");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, account_id);
        assert_eq!(accounts[0].name, "checking");
        assert_eq!(accounts[0].currency, "USD");
        assert!(!accounts[0].is_closed);
    }

    #[test]
    fn create_account_delegates_to_db() {
        let temp_dir = tempdir().expect("create temp dir");
        let data_dir = temp_dir.path().join("state");
        let core = Core::from_data_dir(&data_dir).expect("open core");

        let created = core
            .create_account("cash", "USD", "wallet")
            .expect("create account");

        assert_eq!(created.parent_id, None);
        assert_eq!(created.name, "cash");
        assert_eq!(created.currency, "USD");
        assert_eq!(created.note.as_deref(), Some("wallet"));
        assert!(!created.is_closed);

        let accounts = core.list_accounts().expect("list accounts");
        assert_eq!(accounts, vec![created]);
    }
}
