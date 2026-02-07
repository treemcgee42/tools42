use crate::model::{Statement, Transaction};
use time::Date;

#[derive(Debug)]
pub struct StatementManager {
    statements: Vec<Statement>,
}

#[derive(Debug, Clone, Copy)]
pub struct TransactionView<'a> {
    pub account: &'a str,
    pub transaction: &'a Transaction,
}

impl StatementManager {
    pub fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }

    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    pub fn transactions_in_range(
        &self,
        from: Option<Date>,
        to: Option<Date>,
    ) -> impl Iterator<Item = TransactionView<'_>> {
        self.statements.iter().flat_map(move |stmt| {
            let account = stmt.account.as_str();
            stmt.transaction
                .iter()
                .filter(move |tx| in_range(tx.date, from, to))
                .map(move |tx| TransactionView { account, transaction: tx })
        })
    }
}

fn in_range(date: Date, from: Option<Date>, to: Option<Date>) -> bool {
    if let Some(f) = from {
        if date < f {
            return false;
        }
    }
    if let Some(t) = to {
        if date > t {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use time::Month;

    fn sample_statements() -> Vec<Statement> {
        vec![Statement {
            account: "checking".to_string(),
            statement_file: "2026-01-31.pdf".to_string(),
            closing_date: Date::from_calendar_date(2026, Month::January, 31).unwrap(),
            transaction: vec![
                Transaction {
                    description: "Coffee".to_string(),
                    date: Date::from_calendar_date(2026, Month::January, 5).unwrap(),
                    amount: Decimal::new(450, 2),
                    category: "eating-out".to_string(),
                },
                Transaction {
                    description: "Book".to_string(),
                    date: Date::from_calendar_date(2026, Month::February, 2).unwrap(),
                    amount: Decimal::new(1299, 2),
                    category: "shopping".to_string(),
                },
            ],
        }]
    }

    #[test]
    fn transactions_in_range_is_inclusive() {
        let mgr = StatementManager::new(sample_statements());
        let from = Date::from_calendar_date(2026, Month::January, 5).unwrap();
        let to = Date::from_calendar_date(2026, Month::February, 2).unwrap();

        let items: Vec<_> = mgr.transactions_in_range(Some(from), Some(to)).collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].account, "checking");
        assert_eq!(items[1].account, "checking");
    }
}
