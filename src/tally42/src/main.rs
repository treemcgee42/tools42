mod model;

use model::Statement;

const SAMPLE_TOML: &str = r#"
account = "amex-gold"
statement-file = "2026-01-19.pdf"
closing-date = 2026-01-16

[[transaction]]
description = "So Gong Dong"
date = "2025-12-19"
amount = 41.64
category = "eating-out"
"#;

fn main() {
    let statement: Statement = match toml::from_str(SAMPLE_TOML) {
        Ok(stmt) => stmt,
        Err(err) => {
            eprintln!("error: failed to parse sample statement: {err}");
            std::process::exit(1);
        }
    };
    println!("parsed statement for account: {}", statement.account);
}
