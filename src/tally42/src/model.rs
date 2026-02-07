use rust_decimal::Decimal;
use serde::Deserialize;
use time::{Date, Month};

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Statement {
    pub account: String,
    pub statement_file: String,
    #[serde(deserialize_with = "deserialize_date")]
    pub closing_date: Date,
    pub transaction: Vec<Transaction>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct Transaction {
    pub description: String,
    #[serde(deserialize_with = "deserialize_date")]
    pub date: Date,
    pub amount: Decimal,
    pub category: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DateRepr {
    Str(String),
    Datetime(toml::value::Datetime),
}

fn deserialize_date<'de, D>(deserializer: D) -> Result<Date, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let repr = DateRepr::deserialize(deserializer)?;
    match repr {
        DateRepr::Str(s) => parse_date_str(&s).map_err(serde::de::Error::custom),
        DateRepr::Datetime(dt) => parse_toml_datetime(dt).map_err(serde::de::Error::custom),
    }
}

fn parse_date_str(s: &str) -> Result<Date, String> {
    let fmt = time::format_description::parse("[year]-[month]-[day]")
        .map_err(|e| format!("invalid date format: {e}"))?;
    Date::parse(s, &fmt).map_err(|e| format!("invalid date string '{s}': {e}"))
}

fn parse_toml_datetime(dt: toml::value::Datetime) -> Result<Date, String> {
    let date = dt
        .date
        .ok_or_else(|| "expected a date value".to_string())?;
    let month = Month::try_from(date.month)
        .map_err(|_| format!("invalid month: {}", date.month))?;
    Date::from_calendar_date(date.year.into(), month, date.day)
        .map_err(|e| format!("invalid date: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
account = "amex-gold"
statement-file = "2026-01-19.pdf"
closing-date = 2026-01-16

[[transaction]]
description = "So Gong Dong"
date = "2025-12-19"
amount = 41.64
category = "eating-out"
"#;

    #[test]
    fn parses_sample_statement() {
        let statement: Statement = toml::from_str(SAMPLE).expect("parse statement");
        assert_eq!(statement.account, "amex-gold");
        assert_eq!(statement.statement_file, "2026-01-19.pdf");
        assert_eq!(statement.transaction.len(), 1);
        assert_eq!(statement.transaction[0].description, "So Gong Dong");
        assert_eq!(
            statement.transaction[0].date,
            Date::from_calendar_date(2025, Month::December, 19).unwrap()
        );
    }
}
