use super::db::{Db, DbError};
use super::user_data::UserDataError;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    pub id: Uuid,
    pub institution: String,
    pub account_id: Uuid,
    pub period_start: String,
    pub period_end: String,
    pub currency: String,
    pub file_hash: String,
    pub file_size: i64,
    pub imported_at: String,
    pub replaced_by: Option<Uuid>,
}

impl Statement {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> Result<Self, StatementListError> {
        let id_str: String = row.get("id")?;
        let account_id_str: String = row.get("account_id")?;
        let replaced_by_str: Option<String> = row.get("replaced_by")?;

        let id = Uuid::parse_str(&id_str).map_err(|source| StatementListError::InvalidId {
            value: id_str.clone(),
            source,
        })?;
        let account_id = Uuid::parse_str(&account_id_str).map_err(|source| {
            StatementListError::InvalidAccountId {
                value: account_id_str.clone(),
                source,
            }
        })?;
        let replaced_by = replaced_by_str
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|source| StatementListError::InvalidReplacedById {
                value: replaced_by_str.clone().unwrap_or_default(),
                source,
            })?;

        Ok(Self {
            id,
            institution: row.get("institution")?,
            account_id,
            period_start: row.get("period_start")?,
            period_end: row.get("period_end")?,
            currency: row.get("currency")?,
            file_hash: row.get("file_hash")?,
            file_size: row.get("file_size")?,
            imported_at: row.get("imported_at")?,
            replaced_by,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddStatementInput {
    pub institution: String,
    pub account_id: Uuid,
    pub period_start: String,
    pub period_end: String,
    pub currency: String,
    pub replaced_by: Option<Uuid>,
}

#[derive(Debug)]
pub enum StatementListError {
    Sql(rusqlite::Error),
    InvalidId { value: String, source: uuid::Error },
    InvalidAccountId { value: String, source: uuid::Error },
    InvalidReplacedById { value: String, source: uuid::Error },
}

impl Display for StatementListError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while listing statements: {err}"),
            Self::InvalidId { value, source } => {
                write!(f, "invalid statement id UUID '{value}': {source}")
            }
            Self::InvalidAccountId { value, source } => {
                write!(f, "invalid statement account_id UUID '{value}': {source}")
            }
            Self::InvalidReplacedById { value, source } => {
                write!(f, "invalid statement replaced_by UUID '{value}': {source}")
            }
        }
    }
}

impl std::error::Error for StatementListError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::InvalidId { source, .. } => Some(source),
            Self::InvalidAccountId { source, .. } => Some(source),
            Self::InvalidReplacedById { source, .. } => Some(source),
        }
    }
}

impl From<rusqlite::Error> for StatementListError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Debug)]
pub enum StatementWriteError {
    Sql(rusqlite::Error),
    ReadBack(StatementListError),
    NotFound(Uuid),
}

impl Display for StatementWriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while writing statement: {err}"),
            Self::ReadBack(err) => write!(f, "failed to read back statement after write: {err}"),
            Self::NotFound(id) => write!(f, "statement not found: {id}"),
        }
    }
}

impl std::error::Error for StatementWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::ReadBack(err) => Some(err),
            Self::NotFound(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StatementWriteError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl Db {
    pub fn list_statements(&self) -> Result<Vec<Statement>, StatementListError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              institution,
              account_id,
              period_start,
              period_end,
              currency,
              file_hash,
              file_size,
              imported_at,
              replaced_by
            FROM statements
            ORDER BY imported_at, id
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut statements = Vec::new();

        while let Some(row) = rows.next()? {
            statements.push(Statement::from_row(row)?);
        }

        Ok(statements)
    }

    pub fn create_statement(
        &self,
        id: Uuid,
        institution: &str,
        account_id: Uuid,
        period_start: &str,
        period_end: &str,
        currency: &str,
        file_hash: &str,
        file_size: i64,
        replaced_by: Option<Uuid>,
    ) -> Result<Statement, StatementWriteError> {
        let id_str = id.to_string();
        let account_id_str = account_id.to_string();
        let replaced_by_str = replaced_by.map(|v| v.to_string());
        self.conn().execute(
            "
            INSERT INTO statements (
              id,
              institution,
              account_id,
              period_start,
              period_end,
              currency,
              file_hash,
              file_size,
              replaced_by
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            rusqlite::params![
                id_str,
                institution,
                account_id_str,
                period_start,
                period_end,
                currency,
                file_hash,
                file_size,
                replaced_by_str
            ],
        )?;
        self.get_statement_by_id(id)?
            .ok_or(StatementWriteError::NotFound(id))
    }

    fn get_statement_by_id(&self, id: Uuid) -> Result<Option<Statement>, StatementWriteError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              institution,
              account_id,
              period_start,
              period_end,
              currency,
              file_hash,
              file_size,
              imported_at,
              replaced_by
            FROM statements
            WHERE id = ?1
            ",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => Statement::from_row(row)
                .map(Some)
                .map_err(StatementWriteError::ReadBack),
            None => Ok(None),
        }
    }
}

#[derive(Debug)]
pub enum AddStatementError {
    OpenSource(std::io::Error),
    CreateTempFile(std::io::Error),
    ReadSource(std::io::Error),
    WriteTempFile(std::io::Error),
    TempFileMetadata(std::io::Error),
    FileTooLarge(u64),
    DuplicateFileHash { hash: String, path: PathBuf },
    RenameToFinal(std::io::Error),
    OpenDb(DbError),
    PrepareUserData(UserDataError),
    InsertStatement(StatementWriteError),
    InsertStatementCleanupFailed {
        insert_error: StatementWriteError,
        cleanup_error: std::io::Error,
        path: PathBuf,
    },
}

impl Display for AddStatementError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenSource(err) => write!(f, "failed to open source statement file: {err}"),
            Self::CreateTempFile(err) => write!(f, "failed to create temp statement file: {err}"),
            Self::ReadSource(err) => write!(f, "failed while reading source statement file: {err}"),
            Self::WriteTempFile(err) => {
                write!(f, "failed while writing managed statement file: {err}")
            }
            Self::TempFileMetadata(err) => {
                write!(f, "failed to read temp statement file metadata: {err}")
            }
            Self::FileTooLarge(size) => write!(f, "statement file too large for i64 size: {size}"),
            Self::DuplicateFileHash { hash, path } => write!(
                f,
                "statement file with hash '{hash}' already exists at {}",
                path.display()
            ),
            Self::RenameToFinal(err) => write!(f, "failed to finalize managed statement file: {err}"),
            Self::OpenDb(err) => write!(f, "failed to open database for statement ingest: {err}"),
            Self::PrepareUserData(err) => {
                write!(f, "failed to prepare user data for statement ingest: {err}")
            }
            Self::InsertStatement(err) => write!(f, "failed to insert statement row: {err}"),
            Self::InsertStatementCleanupFailed {
                insert_error,
                cleanup_error,
                path,
            } => write!(
                f,
                "failed to insert statement row ({insert_error}) and failed to remove copied file {}: {cleanup_error}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for AddStatementError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OpenSource(err) => Some(err),
            Self::CreateTempFile(err) => Some(err),
            Self::ReadSource(err) => Some(err),
            Self::WriteTempFile(err) => Some(err),
            Self::TempFileMetadata(err) => Some(err),
            Self::FileTooLarge(_) => None,
            Self::DuplicateFileHash { .. } => None,
            Self::RenameToFinal(err) => Some(err),
            Self::OpenDb(err) => Some(err),
            Self::PrepareUserData(err) => Some(err),
            Self::InsertStatement(err) => Some(err),
            Self::InsertStatementCleanupFailed {
                insert_error,
                cleanup_error,
                ..
            } => {
                let _ = cleanup_error;
                Some(insert_error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Db;

    #[test]
    fn create_statement_inserts_and_returns_statement() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("12121212-1212-1212-1212-121212121212").unwrap();
        db.create_account(account_id, None, "checking", "USD", None)
            .expect("create account");

        let statement_id = Uuid::parse_str("13131313-1313-1313-1313-131313131313").unwrap();
        let statement = db
            .create_statement(
                statement_id,
                "Chase",
                account_id,
                "2026-01-01",
                "2026-01-31",
                "USD",
                "sha256:abc123",
                4096,
                None,
            )
            .expect("create statement");

        assert_eq!(statement.id, statement_id);
        assert_eq!(statement.account_id, account_id);
        assert_eq!(statement.institution, "Chase");
        assert_eq!(statement.period_start, "2026-01-01");
        assert_eq!(statement.period_end, "2026-01-31");
        assert_eq!(statement.currency, "USD");
        assert_eq!(statement.file_hash, "sha256:abc123");
        assert_eq!(statement.file_size, 4096);
        assert_eq!(statement.replaced_by, None);
        assert!(!statement.imported_at.is_empty());
    }

    #[test]
    fn list_statements_returns_rows_and_maps_replaced_by() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("14141414-1414-1414-1414-141414141414").unwrap();
        db.create_account(account_id, None, "savings", "USD", None)
            .expect("create account");

        let first_id = Uuid::parse_str("15151515-1515-1515-1515-151515151515").unwrap();
        let second_id = Uuid::parse_str("16161616-1616-1616-1616-161616161616").unwrap();

        db.create_statement(
            first_id,
            "Bank",
            account_id,
            "2026-02-01",
            "2026-02-28",
            "USD",
            "sha256:first",
            100,
            None,
        )
        .expect("create first statement");
        db.create_statement(
            second_id,
            "Bank",
            account_id,
            "2026-03-01",
            "2026-03-31",
            "USD",
            "sha256:second",
            200,
            Some(first_id),
        )
        .expect("create second statement");

        let statements = db.list_statements().expect("list statements");
        assert_eq!(statements.len(), 2);
        assert!(statements.iter().any(|s| s.id == first_id && s.replaced_by.is_none()));
        assert!(statements
            .iter()
            .any(|s| s.id == second_id && s.replaced_by == Some(first_id)));
    }
}
