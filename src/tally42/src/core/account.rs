use super::db::Db;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: Uuid,               // UUID-based ID
    pub parent_id: Option<Uuid>, // for nesting/categories; None = root
    pub name: String,           // display name (not a full path)
    pub currency: String,       // e.g. "USD" (engine treats as opaque)
    pub is_closed: bool,        // cannot post when true
    pub created_at: String,     // sqlite datetime('now') text
    pub note: Option<String>,
}

impl Account {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> Result<Self, AccountListError> {
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

        Ok(Self {
            id,
            parent_id,
            name: row.get("name")?,
            currency: row.get("currency")?,
            is_closed: is_closed != 0,
            created_at: row.get("created_at")?,
            note: row.get("note")?,
        })
    }
}

#[derive(Debug)]
pub enum AccountListError {
    Sql(rusqlite::Error),
    InvalidId { value: String, source: uuid::Error },
    InvalidParentId { value: String, source: uuid::Error },
}

impl Display for AccountListError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while listing accounts: {err}"),
            Self::InvalidId { value, source } => {
                write!(f, "invalid account id UUID '{value}': {source}")
            }
            Self::InvalidParentId { value, source } => {
                write!(f, "invalid parent account id UUID '{value}': {source}")
            }
        }
    }
}

impl std::error::Error for AccountListError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::InvalidId { source, .. } => Some(source),
            Self::InvalidParentId { source, .. } => Some(source),
        }
    }
}

impl From<rusqlite::Error> for AccountListError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Debug)]
pub enum AccountWriteError {
    Sql(rusqlite::Error),
    ReadBack(AccountListError),
    NotFound(Uuid),
}

impl Display for AccountWriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while writing account: {err}"),
            Self::ReadBack(err) => write!(f, "failed to read back account after write: {err}"),
            Self::NotFound(id) => write!(f, "account not found: {id}"),
        }
    }
}

impl std::error::Error for AccountWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::ReadBack(err) => Some(err),
            Self::NotFound(_) => None,
        }
    }
}

impl From<rusqlite::Error> for AccountWriteError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl Db {
    pub fn list_accounts(&self) -> Result<Vec<Account>, AccountListError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT id, parent_id, name, currency, is_closed, created_at, note
            FROM accounts
            ORDER BY parent_id, name, id
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut accounts = Vec::new();

        while let Some(row) = rows.next()? {
            accounts.push(Account::from_row(row)?);
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
        self.conn().execute(
            "
            INSERT INTO accounts (id, parent_id, name, currency, is_closed, note)
            VALUES (?1, ?2, ?3, ?4, 0, ?5)
            ",
            rusqlite::params![id_str, parent_id_str, name, currency, note],
        )?;
        self.get_account_by_id(id)?.ok_or(AccountWriteError::NotFound(id))
    }

    pub fn rename_account(&self, id: Uuid, new_name: &str) -> Result<Account, AccountWriteError> {
        let updated = self.conn().execute(
            "UPDATE accounts SET name = ?2 WHERE id = ?1",
            rusqlite::params![id.to_string(), new_name],
        )?;
        if updated == 0 {
            return Err(AccountWriteError::NotFound(id));
        }
        self.get_account_by_id(id)?.ok_or(AccountWriteError::NotFound(id))
    }

    pub fn close_account(&self, id: Uuid) -> Result<Account, AccountWriteError> {
        let updated = self.conn().execute(
            "UPDATE accounts SET is_closed = 1 WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )?;
        if updated == 0 {
            return Err(AccountWriteError::NotFound(id));
        }
        self.get_account_by_id(id)?.ok_or(AccountWriteError::NotFound(id))
    }

    fn get_account_by_id(&self, id: Uuid) -> Result<Option<Account>, AccountWriteError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT id, parent_id, name, currency, is_closed, created_at, note
            FROM accounts
            WHERE id = ?1
            ",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => Account::from_row(row).map(Some).map_err(AccountWriteError::ReadBack),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Db;
    use rusqlite::params;

    fn insert_account(
        conn: &rusqlite::Connection,
        id: &str,
        parent_id: Option<&str>,
        name: &str,
        currency: &str,
        is_closed: i64,
        created_at: &str,
        note: Option<&str>,
    ) {
        conn.execute(
            "
            INSERT INTO accounts (id, parent_id, name, currency, is_closed, created_at, note)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![id, parent_id, name, currency, is_closed, created_at, note],
        )
        .expect("insert account");
    }

    #[test]
    fn list_accounts_returns_all_fields() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let conn = db.conn();

        let id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        insert_account(
            &conn,
            &id.to_string(),
            None,
            "checking",
            "USD",
            0,
            "2026-02-22 13:00:00",
            Some("household spending"),
        );

        let accounts = db.list_accounts().expect("list accounts");
        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts[0],
            Account {
                id,
                parent_id: None,
                name: "checking".to_string(),
                currency: "USD".to_string(),
                is_closed: false,
                created_at: "2026-02-22 13:00:00".to_string(),
                note: Some("household spending".to_string()),
            }
        );
    }

    #[test]
    fn list_accounts_maps_null_parent_and_note() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let conn = db.conn();

        let id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        insert_account(
            &conn,
            &id.to_string(),
            None,
            "root",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );

        let accounts = db.list_accounts().expect("list accounts");
        assert_eq!(accounts[0].parent_id, None);
        assert_eq!(accounts[0].note, None);
    }

    #[test]
    fn list_accounts_orders_by_parent_then_name() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let conn = db.conn();

        let root_a = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa1").unwrap();
        let root_b = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb1").unwrap();
        let child_a1 = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa2").unwrap();
        let child_a2 = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa3").unwrap();

        insert_account(
            &conn,
            &root_b.to_string(),
            None,
            "b-root",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );
        insert_account(
            &conn,
            &root_a.to_string(),
            None,
            "a-root",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );
        insert_account(
            &conn,
            &child_a1.to_string(),
            Some(&root_a.to_string()),
            "a-child",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );
        insert_account(
            &conn,
            &child_a2.to_string(),
            Some(&root_a.to_string()),
            "z-child",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );

        let accounts = db.list_accounts().expect("list accounts");
        let names: Vec<_> = accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["a-root", "b-root", "a-child", "z-child"]);
    }

    #[test]
    fn list_accounts_maps_is_closed_to_bool() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let conn = db.conn();

        let open_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let closed_id = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        insert_account(
            &conn,
            &open_id.to_string(),
            None,
            "open",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );
        insert_account(
            &conn,
            &closed_id.to_string(),
            None,
            "closed",
            "USD",
            1,
            "2026-02-22 13:00:00",
            None,
        );

        let accounts = db.list_accounts().expect("list accounts");
        let open = accounts.iter().find(|a| a.id == open_id).unwrap();
        let closed = accounts.iter().find(|a| a.id == closed_id).unwrap();
        assert!(!open.is_closed);
        assert!(closed.is_closed);
    }

    #[test]
    fn list_accounts_errors_on_invalid_id_uuid() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let conn = db.conn();

        insert_account(
            &conn,
            "not-a-uuid",
            None,
            "broken",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );

        let err = db.list_accounts().expect_err("expected invalid id error");
        assert!(matches!(err, AccountListError::InvalidId { .. }));
    }

    #[test]
    fn list_accounts_errors_on_invalid_parent_id_uuid() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let conn = db.conn();
        conn.execute_batch("PRAGMA foreign_keys=OFF;")
            .expect("disable foreign keys for malformed parent_id fixture");

        insert_account(
            &conn,
            "55555555-5555-5555-5555-555555555555",
            Some("not-a-uuid"),
            "broken-child",
            "USD",
            0,
            "2026-02-22 13:00:00",
            None,
        );

        let err = db.list_accounts().expect_err("expected invalid parent id error");
        assert!(matches!(err, AccountListError::InvalidParentId { .. }));
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
