use super::db::DbError;
use super::user_data::UserDataError;
use std::path::PathBuf;
use std::fmt::{Display, Formatter};
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
