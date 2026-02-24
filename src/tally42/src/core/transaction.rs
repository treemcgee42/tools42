use super::db::Db;
use std::collections::BTreeMap;
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

impl Transaction {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> Result<Self, TransactionListError> {
        let id_str: String = row.get("id")?;
        let statement_id_str: Option<String> = row.get("statement_id")?;

        let id = Uuid::parse_str(&id_str).map_err(|source| TransactionListError::InvalidId {
            value: id_str.clone(),
            source,
        })?;
        let statement_id = statement_id_str
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|source| TransactionListError::InvalidStatementId {
                value: statement_id_str.clone().unwrap_or_default(),
                source,
            })?;

        Ok(Self {
            id,
            statement_id,
            description: row.get("description")?,
            posted_at: row.get("posted_at")?,
            created_at: row.get("created_at")?,
        })
    }
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

impl Posting {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> Result<Self, PostingListError> {
        let id_str: String = row.get("id")?;
        let transaction_id_str: String = row.get("transaction_id")?;
        let account_id_str: String = row.get("account_id")?;
        let direction_str: String = row.get("direction")?;

        let id = Uuid::parse_str(&id_str).map_err(|source| PostingListError::InvalidId {
            value: id_str.clone(),
            source,
        })?;
        let transaction_id = Uuid::parse_str(&transaction_id_str).map_err(|source| {
            PostingListError::InvalidTransactionId {
                value: transaction_id_str.clone(),
                source,
            }
        })?;
        let account_id = Uuid::parse_str(&account_id_str).map_err(|source| {
            PostingListError::InvalidAccountId {
                value: account_id_str.clone(),
                source,
            }
        })?;

        Ok(Self {
            id,
            transaction_id,
            account_id,
            amount: row.get("amount")?,
            currency: row.get("currency")?,
            direction: PostingDirection::from_db_str(&direction_str)?,
        })
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddPostingInput {
    pub account_id: Uuid,
    pub amount: i64,
    pub currency: String,
    pub direction: PostingDirection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddTransactionInput {
    pub statement_id: Option<Uuid>,
    pub description: Option<String>,
    pub posted_at: String,
    pub postings: Vec<AddPostingInput>,
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

#[derive(Debug)]
pub enum AddTransactionError {
    NoPostings,
    Unbalanced {
        currency: String,
        debit_total: i64,
        credit_total: i64,
    },
    AmountOverflow { currency: String },
    Write(CreateTransactionWithPostingsError),
}

impl Display for AddTransactionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPostings => write!(f, "transaction must have at least one posting"),
            Self::Unbalanced {
                currency,
                debit_total,
                credit_total,
            } => write!(
                f,
                "transaction is unbalanced for currency {currency}: debits={debit_total}, credits={credit_total}"
            ),
            Self::AmountOverflow { currency } => {
                write!(f, "posting totals overflowed while validating currency {currency}")
            }
            Self::Write(err) => write!(f, "failed to create transaction: {err}"),
        }
    }
}

impl std::error::Error for AddTransactionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoPostings => None,
            Self::Unbalanced { .. } => None,
            Self::AmountOverflow { .. } => None,
            Self::Write(err) => Some(err),
        }
    }
}

impl From<CreateTransactionWithPostingsError> for AddTransactionError {
    fn from(value: CreateTransactionWithPostingsError) -> Self {
        Self::Write(value)
    }
}

impl Db {
    pub fn add_transaction(
        &mut self,
        input: AddTransactionInput,
    ) -> Result<(Transaction, Vec<Posting>), AddTransactionError> {
        if input.postings.is_empty() {
            return Err(AddTransactionError::NoPostings);
        }

        let mut totals: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
        for posting in &input.postings {
            let entry = totals.entry(posting.currency.as_str()).or_insert((0, 0));
            match posting.direction {
                PostingDirection::Debit => {
                    entry.0 = entry
                        .0
                        .checked_add(posting.amount)
                        .ok_or_else(|| AddTransactionError::AmountOverflow {
                            currency: posting.currency.clone(),
                        })?;
                }
                PostingDirection::Credit => {
                    entry.1 = entry
                        .1
                        .checked_add(posting.amount)
                        .ok_or_else(|| AddTransactionError::AmountOverflow {
                            currency: posting.currency.clone(),
                        })?;
                }
            }
        }

        for (currency, (debit_total, credit_total)) in totals {
            if debit_total != credit_total {
                return Err(AddTransactionError::Unbalanced {
                    currency: currency.to_string(),
                    debit_total,
                    credit_total,
                });
            }
        }

        let tx_id = Uuid::new_v4();
        let postings: Vec<NewPostingInput> = input
            .postings
            .into_iter()
            .map(|posting| NewPostingInput {
                id: Uuid::new_v4(),
                account_id: posting.account_id,
                amount: posting.amount,
                currency: posting.currency,
                direction: posting.direction,
            })
            .collect();

        self.create_transaction_with_postings(
            tx_id,
            input.statement_id,
            input.description.as_deref(),
            &input.posted_at,
            &postings,
        )
        .map_err(AddTransactionError::Write)
    }

    pub fn list_transactions(&self) -> Result<Vec<Transaction>, TransactionListError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              statement_id,
              description,
              posted_at,
              created_at
            FROM transactions
            ORDER BY posted_at, created_at, id
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut transactions = Vec::new();

        while let Some(row) = rows.next()? {
            transactions.push(Transaction::from_row(row)?);
        }

        Ok(transactions)
    }

    pub fn create_transaction(
        &self,
        id: Uuid,
        statement_id: Option<Uuid>,
        description: Option<&str>,
        posted_at: &str,
    ) -> Result<Transaction, TransactionWriteError> {
        let id_str = id.to_string();
        let statement_id_str = statement_id.map(|v| v.to_string());
        self.conn().execute(
            "
            INSERT INTO transactions (id, statement_id, description, posted_at)
            VALUES (?1, ?2, ?3, ?4)
            ",
            rusqlite::params![id_str, statement_id_str, description, posted_at],
        )?;
        self.get_transaction_by_id(id)?
            .ok_or(TransactionWriteError::NotFound(id))
    }

    pub fn list_postings(&self) -> Result<Vec<Posting>, PostingListError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              transaction_id,
              account_id,
              amount,
              currency,
              direction
            FROM postings
            ORDER BY transaction_id, id
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut postings = Vec::new();

        while let Some(row) = rows.next()? {
            postings.push(Posting::from_row(row)?);
        }

        Ok(postings)
    }

    pub fn list_postings_for_transaction(
        &self,
        transaction_id: Uuid,
    ) -> Result<Vec<Posting>, PostingListError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              transaction_id,
              account_id,
              amount,
              currency,
              direction
            FROM postings
            WHERE transaction_id = ?1
            ORDER BY id
            ",
        )?;
        let mut rows = stmt.query([transaction_id.to_string()])?;
        let mut postings = Vec::new();

        while let Some(row) = rows.next()? {
            postings.push(Posting::from_row(row)?);
        }

        Ok(postings)
    }

    pub fn create_posting(
        &self,
        id: Uuid,
        transaction_id: Uuid,
        account_id: Uuid,
        amount: i64,
        currency: &str,
        direction: PostingDirection,
    ) -> Result<Posting, PostingWriteError> {
        let id_str = id.to_string();
        let transaction_id_str = transaction_id.to_string();
        let account_id_str = account_id.to_string();
        self.conn().execute(
            "
            INSERT INTO postings (id, transaction_id, account_id, amount, currency, direction)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            rusqlite::params![
                id_str,
                transaction_id_str,
                account_id_str,
                amount,
                currency,
                direction.as_str()
            ],
        )?;
        self.get_posting_by_id(id)?.ok_or(PostingWriteError::NotFound(id))
    }

    pub fn create_transaction_with_postings(
        &mut self,
        id: Uuid,
        statement_id: Option<Uuid>,
        description: Option<&str>,
        posted_at: &str,
        postings: &[NewPostingInput],
    ) -> Result<(Transaction, Vec<Posting>), CreateTransactionWithPostingsError> {
        let tx = self.conn_mut().transaction()?;
        let id_str = id.to_string();
        let statement_id_str = statement_id.map(|v| v.to_string());

        tx.execute(
            "
            INSERT INTO transactions (id, statement_id, description, posted_at)
            VALUES (?1, ?2, ?3, ?4)
            ",
            rusqlite::params![id_str, statement_id_str, description, posted_at],
        )?;

        for posting in postings {
            tx.execute(
                "
                INSERT INTO postings (id, transaction_id, account_id, amount, currency, direction)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                rusqlite::params![
                    posting.id.to_string(),
                    id.to_string(),
                    posting.account_id.to_string(),
                    posting.amount,
                    posting.currency.as_str(),
                    posting.direction.as_str(),
                ],
            )?;
        }

        tx.commit()?;

        let transaction = self
            .get_transaction_by_id(id)
            .map_err(CreateTransactionWithPostingsError::from_transaction_write)?
            .ok_or(CreateTransactionWithPostingsError::TransactionNotFound(id))?;

        let mut inserted_postings = Vec::with_capacity(postings.len());
        for posting in postings {
            let inserted = self
                .get_posting_by_id(posting.id)
                .map_err(CreateTransactionWithPostingsError::from_posting_write)?
                .ok_or(CreateTransactionWithPostingsError::PostingNotFound(posting.id))?;
            inserted_postings.push(inserted);
        }

        Ok((transaction, inserted_postings))
    }

    fn get_transaction_by_id(&self, id: Uuid) -> Result<Option<Transaction>, TransactionWriteError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              statement_id,
              description,
              posted_at,
              created_at
            FROM transactions
            WHERE id = ?1
            ",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => Transaction::from_row(row)
                .map(Some)
                .map_err(TransactionWriteError::ReadBack),
            None => Ok(None),
        }
    }

    fn get_posting_by_id(&self, id: Uuid) -> Result<Option<Posting>, PostingWriteError> {
        let mut stmt = self.conn().prepare(
            "
            SELECT
              id,
              transaction_id,
              account_id,
              amount,
              currency,
              direction
            FROM postings
            WHERE id = ?1
            ",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => Posting::from_row(row).map(Some).map_err(PostingWriteError::ReadBack),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Db;

    #[test]
    fn create_transaction_inserts_and_returns_transaction() {
        let db = Db::open_for_tests().expect("open in-memory db");

        let tx_id = Uuid::parse_str("17171717-1717-1717-1717-171717171717").unwrap();
        let transaction = db
            .create_transaction(tx_id, None, Some("Coffee"), "2026-02-20")
            .expect("create transaction");

        assert_eq!(transaction.id, tx_id);
        assert_eq!(transaction.statement_id, None);
        assert_eq!(transaction.description.as_deref(), Some("Coffee"));
        assert_eq!(transaction.posted_at, "2026-02-20");
        assert!(!transaction.created_at.is_empty());
    }

    #[test]
    fn create_transaction_with_statement_id_round_trips() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("18181818-1818-1818-1818-181818181818").unwrap();
        db.create_account(account_id, None, "checking", "USD", None)
            .expect("create account");
        let statement_id = Uuid::parse_str("19191919-1919-1919-1919-191919191919").unwrap();
        db.create_statement(
            statement_id,
            "Bank",
            account_id,
            "2026-02-01",
            "2026-02-28",
            "USD",
            "sha256:tx-stmt",
            123,
            None,
        )
        .expect("create statement");

        let tx_id = Uuid::parse_str("20202020-2020-2020-2020-202020202020").unwrap();
        let transaction = db
            .create_transaction(tx_id, Some(statement_id), None, "2026-02-21")
            .expect("create transaction");

        assert_eq!(transaction.statement_id, Some(statement_id));
        assert_eq!(transaction.description, None);
    }

    #[test]
    fn list_transactions_returns_rows_and_maps_nullable_fields() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let first_id = Uuid::parse_str("21212121-2121-2121-2121-212121212121").unwrap();
        let second_id = Uuid::parse_str("22222222-aaaa-bbbb-cccc-222222222222").unwrap();

        db.create_transaction(first_id, None, None, "2026-02-10")
            .expect("create first transaction");
        db.create_transaction(second_id, None, Some("Rent"), "2026-02-11")
            .expect("create second transaction");

        let transactions = db.list_transactions().expect("list transactions");
        assert_eq!(transactions.len(), 2);
        assert!(transactions
            .iter()
            .any(|t| t.id == first_id && t.statement_id.is_none() && t.description.is_none()));
        assert!(transactions
            .iter()
            .any(|t| t.id == second_id && t.description.as_deref() == Some("Rent")));
    }

    #[test]
    fn create_posting_inserts_and_returns_posting() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("23232323-2323-2323-2323-232323232323").unwrap();
        db.create_account(account_id, None, "expense:coffee", "USD", None)
            .expect("create account");
        let tx_id = Uuid::parse_str("24242424-2424-2424-2424-242424242424").unwrap();
        db.create_transaction(tx_id, None, Some("Coffee"), "2026-02-22")
            .expect("create transaction");

        let posting_id = Uuid::parse_str("25252525-2525-2525-2525-252525252525").unwrap();
        let posting = db
            .create_posting(
                posting_id,
                tx_id,
                account_id,
                450,
                "USD",
                PostingDirection::Debit,
            )
            .expect("create posting");

        assert_eq!(posting.id, posting_id);
        assert_eq!(posting.transaction_id, tx_id);
        assert_eq!(posting.account_id, account_id);
        assert_eq!(posting.amount, 450);
        assert_eq!(posting.currency, "USD");
        assert_eq!(posting.direction, PostingDirection::Debit);
    }

    #[test]
    fn list_postings_for_transaction_filters_and_orders() {
        let db = Db::open_for_tests().expect("open in-memory db");
        let account_id = Uuid::parse_str("26262626-2626-2626-2626-262626262626").unwrap();
        db.create_account(account_id, None, "assets:cash", "USD", None)
            .expect("create account");

        let tx_a = Uuid::parse_str("27272727-2727-2727-2727-272727272727").unwrap();
        let tx_b = Uuid::parse_str("28282828-2828-2828-2828-282828282828").unwrap();
        db.create_transaction(tx_a, None, None, "2026-02-01")
            .expect("create tx a");
        db.create_transaction(tx_b, None, None, "2026-02-02")
            .expect("create tx b");

        let posting_a2 = Uuid::parse_str("29292929-2929-2929-2929-292929292929").unwrap();
        let posting_a1 = Uuid::parse_str("2a2a2a2a-2a2a-2a2a-2a2a-2a2a2a2a2a2a").unwrap();
        let posting_b1 = Uuid::parse_str("2b2b2b2b-2b2b-2b2b-2b2b-2b2b2b2b2b2b").unwrap();

        db.create_posting(
            posting_a2,
            tx_a,
            account_id,
            100,
            "USD",
            PostingDirection::Credit,
        )
        .expect("create posting a2");
        db.create_posting(
            posting_a1,
            tx_a,
            account_id,
            100,
            "USD",
            PostingDirection::Debit,
        )
        .expect("create posting a1");
        db.create_posting(posting_b1, tx_b, account_id, 50, "USD", PostingDirection::Debit)
            .expect("create posting b1");

        let postings = db
            .list_postings_for_transaction(tx_a)
            .expect("list postings for transaction");
        let ids: Vec<_> = postings.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![posting_a2, posting_a1]);
    }

    #[test]
    fn create_transaction_with_postings_is_atomic_on_posting_failure() {
        let mut db = Db::open_for_tests().expect("open in-memory db");
        let valid_account_id = Uuid::parse_str("2c2c2c2c-2c2c-2c2c-2c2c-2c2c2c2c2c2c").unwrap();
        db.create_account(valid_account_id, None, "assets:checking", "USD", None)
            .expect("create account");

        let tx_id = Uuid::parse_str("2d2d2d2d-2d2d-2d2d-2d2d-2d2d2d2d2d2d").unwrap();
        let good_posting_id = Uuid::parse_str("2e2e2e2e-2e2e-2e2e-2e2e-2e2e2e2e2e2e").unwrap();
        let bad_posting_id = Uuid::parse_str("2f2f2f2f-2f2f-2f2f-2f2f-2f2f2f2f2f2f").unwrap();
        let missing_account_id = Uuid::parse_str("30303030-3030-3030-3030-303030303030").unwrap();

        let err = db
            .create_transaction_with_postings(
                tx_id,
                None,
                Some("atomic"),
                "2026-02-23",
                &[
                    NewPostingInput {
                        id: good_posting_id,
                        account_id: valid_account_id,
                        amount: 100,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Debit,
                    },
                    NewPostingInput {
                        id: bad_posting_id,
                        account_id: missing_account_id,
                        amount: 100,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Credit,
                    },
                ],
            )
            .expect_err("atomic create should fail");

        assert!(matches!(err, CreateTransactionWithPostingsError::Sql(_)));
        assert!(db
            .list_transactions()
            .expect("list transactions")
            .iter()
            .all(|t| t.id != tx_id));
        assert!(db
            .list_postings()
            .expect("list postings")
            .iter()
            .all(|p| p.transaction_id != tx_id));
    }

    #[test]
    fn add_transaction_creates_balanced_transaction_and_postings() {
        let mut db = Db::open_for_tests().expect("open in-memory db");
        let cash_id = Uuid::parse_str("31313131-3131-3131-3131-313131313131").unwrap();
        let expense_id = Uuid::parse_str("32323232-3232-3232-3232-323232323232").unwrap();
        db.create_account(cash_id, None, "assets:cash", "USD", None)
            .expect("create cash account");
        db.create_account(expense_id, None, "expenses:food", "USD", None)
            .expect("create expense account");

        let (transaction, postings) = db
            .add_transaction(AddTransactionInput {
                statement_id: None,
                description: Some("Lunch".to_string()),
                posted_at: "2026-02-24".to_string(),
                postings: vec![
                    AddPostingInput {
                        account_id: expense_id,
                        amount: 1500,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Debit,
                    },
                    AddPostingInput {
                        account_id: cash_id,
                        amount: 1500,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Credit,
                    },
                ],
            })
            .expect("add transaction");

        assert_eq!(transaction.description.as_deref(), Some("Lunch"));
        assert_eq!(transaction.posted_at, "2026-02-24");
        assert_eq!(postings.len(), 2);
        assert!(postings.iter().all(|p| p.transaction_id == transaction.id));
    }

    #[test]
    fn add_transaction_rejects_unbalanced_per_currency() {
        let mut db = Db::open_for_tests().expect("open in-memory db");
        let a_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let b_id = Uuid::parse_str("34343434-3434-3434-3434-343434343434").unwrap();
        let c_id = Uuid::parse_str("35353535-3535-3535-3535-353535353535").unwrap();
        let d_id = Uuid::parse_str("36363636-3636-3636-3636-363636363636").unwrap();
        for (id, name, cur) in [
            (a_id, "a", "USD"),
            (b_id, "b", "USD"),
            (c_id, "c", "EUR"),
            (d_id, "d", "EUR"),
        ] {
            db.create_account(id, None, name, cur, None)
                .expect("create account");
        }

        let err = db
            .add_transaction(AddTransactionInput {
                statement_id: None,
                description: None,
                posted_at: "2026-02-24".to_string(),
                postings: vec![
                    AddPostingInput {
                        account_id: a_id,
                        amount: 100,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Debit,
                    },
                    AddPostingInput {
                        account_id: b_id,
                        amount: 100,
                        currency: "USD".to_string(),
                        direction: PostingDirection::Credit,
                    },
                    AddPostingInput {
                        account_id: c_id,
                        amount: 200,
                        currency: "EUR".to_string(),
                        direction: PostingDirection::Debit,
                    },
                    AddPostingInput {
                        account_id: d_id,
                        amount: 150,
                        currency: "EUR".to_string(),
                        direction: PostingDirection::Credit,
                    },
                ],
            })
            .expect_err("should reject unbalanced transaction");

        assert!(matches!(
            err,
            AddTransactionError::Unbalanced {
                currency,
                debit_total: 200,
                credit_total: 150
            } if currency == "EUR"
        ));
        assert!(db.list_transactions().expect("list tx").is_empty());
        assert!(db.list_postings().expect("list postings").is_empty());
    }
}
