use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub id: Uuid,
    pub statement_id: Option<Uuid>,
    pub description: Option<String>,
    pub posted_at: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Posting {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub account_id: Uuid,
    pub amount: i64,
    pub currency: String,
    pub direction: PostingDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostingDirection {
    Debit,
    Credit,
}

impl PostingDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }

    pub fn from_db_str(value: &str) -> Result<Self, PostingListError> {
        match value {
            "debit" => Ok(Self::Debit),
            "credit" => Ok(Self::Credit),
            _ => Err(PostingListError::InvalidDirection {
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPostingInput {
    pub id: Uuid,
    pub account_id: Uuid,
    pub amount: i64,
    pub currency: String,
    pub direction: PostingDirection,
}

#[derive(Debug)]
pub enum TransactionListError {
    Sql(rusqlite::Error),
    InvalidId { value: String, source: uuid::Error },
    InvalidStatementId { value: String, source: uuid::Error },
}

impl Display for TransactionListError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while listing transactions: {err}"),
            Self::InvalidId { value, source } => {
                write!(f, "invalid transaction id UUID '{value}': {source}")
            }
            Self::InvalidStatementId { value, source } => {
                write!(f, "invalid transaction statement_id UUID '{value}': {source}")
            }
        }
    }
}

impl std::error::Error for TransactionListError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::InvalidId { source, .. } => Some(source),
            Self::InvalidStatementId { source, .. } => Some(source),
        }
    }
}

impl From<rusqlite::Error> for TransactionListError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Debug)]
pub enum TransactionWriteError {
    Sql(rusqlite::Error),
    ReadBack(TransactionListError),
    NotFound(Uuid),
}

impl Display for TransactionWriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while writing transaction: {err}"),
            Self::ReadBack(err) => write!(f, "failed to read back transaction after write: {err}"),
            Self::NotFound(id) => write!(f, "transaction not found: {id}"),
        }
    }
}

impl std::error::Error for TransactionWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::ReadBack(err) => Some(err),
            Self::NotFound(_) => None,
        }
    }
}

impl From<rusqlite::Error> for TransactionWriteError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Debug)]
pub enum PostingListError {
    Sql(rusqlite::Error),
    InvalidId { value: String, source: uuid::Error },
    InvalidTransactionId { value: String, source: uuid::Error },
    InvalidAccountId { value: String, source: uuid::Error },
    InvalidDirection { value: String },
}

impl Display for PostingListError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while listing postings: {err}"),
            Self::InvalidId { value, source } => {
                write!(f, "invalid posting id UUID '{value}': {source}")
            }
            Self::InvalidTransactionId { value, source } => {
                write!(f, "invalid posting transaction_id UUID '{value}': {source}")
            }
            Self::InvalidAccountId { value, source } => {
                write!(f, "invalid posting account_id UUID '{value}': {source}")
            }
            Self::InvalidDirection { value } => {
                write!(f, "invalid posting direction '{value}'")
            }
        }
    }
}

impl std::error::Error for PostingListError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::InvalidId { source, .. } => Some(source),
            Self::InvalidTransactionId { source, .. } => Some(source),
            Self::InvalidAccountId { source, .. } => Some(source),
            Self::InvalidDirection { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for PostingListError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Debug)]
pub enum PostingWriteError {
    Sql(rusqlite::Error),
    ReadBack(PostingListError),
    NotFound(Uuid),
}

impl Display for PostingWriteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(f, "sqlite error while writing posting: {err}"),
            Self::ReadBack(err) => write!(f, "failed to read back posting after write: {err}"),
            Self::NotFound(id) => write!(f, "posting not found: {id}"),
        }
    }
}

impl std::error::Error for PostingWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::ReadBack(err) => Some(err),
            Self::NotFound(_) => None,
        }
    }
}

impl From<rusqlite::Error> for PostingWriteError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

#[derive(Debug)]
pub enum CreateTransactionWithPostingsError {
    Sql(rusqlite::Error),
    ReadBackTransaction(TransactionListError),
    ReadBackPosting(PostingListError),
    TransactionNotFound(Uuid),
    PostingNotFound(Uuid),
}

impl Display for CreateTransactionWithPostingsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(err) => write!(
                f,
                "sqlite error while creating transaction with postings: {err}"
            ),
            Self::ReadBackTransaction(err) => {
                write!(f, "failed to read back transaction after atomic write: {err}")
            }
            Self::ReadBackPosting(err) => {
                write!(f, "failed to read back posting after atomic write: {err}")
            }
            Self::TransactionNotFound(id) => {
                write!(f, "transaction not found after atomic write: {id}")
            }
            Self::PostingNotFound(id) => write!(f, "posting not found after atomic write: {id}"),
        }
    }
}

impl std::error::Error for CreateTransactionWithPostingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sql(err) => Some(err),
            Self::ReadBackTransaction(err) => Some(err),
            Self::ReadBackPosting(err) => Some(err),
            Self::TransactionNotFound(_) => None,
            Self::PostingNotFound(_) => None,
        }
    }
}

impl From<rusqlite::Error> for CreateTransactionWithPostingsError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl CreateTransactionWithPostingsError {
    pub(crate) fn from_transaction_write(value: TransactionWriteError) -> Self {
        match value {
            TransactionWriteError::Sql(err) => Self::Sql(err),
            TransactionWriteError::ReadBack(err) => Self::ReadBackTransaction(err),
            TransactionWriteError::NotFound(id) => Self::TransactionNotFound(id),
        }
    }

    pub(crate) fn from_posting_write(value: PostingWriteError) -> Self {
        match value {
            PostingWriteError::Sql(err) => Self::Sql(err),
            PostingWriteError::ReadBack(err) => Self::ReadBackPosting(err),
            PostingWriteError::NotFound(id) => Self::PostingNotFound(id),
        }
    }
}
