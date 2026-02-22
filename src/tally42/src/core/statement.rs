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
