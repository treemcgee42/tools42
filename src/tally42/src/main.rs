mod manager;
mod migration;
mod model;
mod user_data;

use clap::{Parser, Subcommand};
use manager::StatementManager;
use migration::{Migration, MigrationRunner, MigrationsDir};
use model::Statement;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use time::Date;
use user_data::UserDataManager;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "tally42")]
#[command(about = "Introspect personal financial statements", long_about = None)]
struct Args {
    #[arg(short, long, help = "Print per-file load details")]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
    Summary {
        #[arg(long, value_name = "YYYY-MM-DD", help = "Start date (inclusive)")]
        from: Option<String>,
        #[arg(long, value_name = "YYYY-MM-DD", help = "End date (inclusive)")]
        to: Option<String>,
    },
    DeleteDb,
}

fn main() {
    let args = Args::parse();
    let user_data_manager = user_data_manager_or_exit();

    match args.command {
        Command::Init => init_command_or_exit(&user_data_manager),
        Command::Summary { from, to } => {
            init_user_data_or_exit(&user_data_manager);
            run_summary(args.verbose, from, to)
        }
        Command::DeleteDb => delete_db_or_exit(&user_data_manager),
    }
}

fn user_data_manager_or_exit() -> UserDataManager {
    UserDataManager::from_environment().unwrap_or_else(|err| {
        eprintln!("error: failed to resolve user data directory: {err}");
        std::process::exit(1);
    })
}

fn init_user_data_or_exit(manager: &UserDataManager) {
    manager.init().unwrap_or_else(|err| {
        eprintln!("error: failed to initialize user data: {err}");
        std::process::exit(1);
    });
}

fn init_command_or_exit(manager: &UserDataManager) {
    init_user_data_or_exit(manager);
    run_embedded_migrations_or_exit(manager);
    println!("initialized database at {}", manager.db_path().display());
}

fn run_embedded_migrations_or_exit(manager: &UserDataManager) {
    let conn = rusqlite::Connection::open(manager.db_path()).unwrap_or_else(|err| {
        eprintln!(
            "error: failed to open database for migrations ({}): {err}",
            manager.db_path().display()
        );
        std::process::exit(1);
    });
    let source = MigrationsDir::embedded();
    let migrations = Migration::from_source(&source).unwrap_or_else(|err| {
        eprintln!("error: failed to discover embedded migrations: {err}");
        std::process::exit(1);
    });
    let runner = MigrationRunner::new(&conn);
    runner.run(&source, &migrations).unwrap_or_else(|err| {
        eprintln!("error: failed to run embedded migrations: {err}");
        std::process::exit(1);
    });
}

fn delete_db_or_exit(manager: &UserDataManager) {
    match manager.delete_db() {
        Ok(true) => println!("deleted database at {}", manager.db_path().display()),
        Ok(false) => println!("database not found at {}", manager.db_path().display()),
        Err(err) => {
            eprintln!("error: failed to delete database: {err}");
            std::process::exit(1);
        }
    }
}

fn is_toml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        .unwrap_or(false)
}

fn should_skip_dir(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    matches!(entry.file_name().to_str(), Some(".git" | "bld" | "bin"))
}

fn run_summary(verbose: bool, from: Option<String>, to: Option<String>) {
    let workdir = resolve_workdir();
    let (statements, warnings) = load_statements(&workdir, verbose);
    let manager = StatementManager::new(statements);

    let from_date = from.as_deref().map(parse_date_arg);
    let to_date = to.as_deref().map(parse_date_arg);
    if let (Some(f), Some(t)) = (from_date, to_date) {
        if f > t {
            eprintln!("error: --from must be on or before --to");
            std::process::exit(1);
        }
    }

    let mut total = Decimal::ZERO;
    let mut by_category: HashMap<String, Decimal> = HashMap::new();
    let mut by_account: HashMap<String, Decimal> = HashMap::new();
    let mut top_items = Vec::new();

    for item in manager.transactions_in_range(from_date, to_date) {
        let tx = item.transaction;
        if tx.amount <= Decimal::ZERO {
            continue;
        }
        total += tx.amount;
        *by_category.entry(tx.category.clone()).or_insert(Decimal::ZERO) += tx.amount;
        *by_account
            .entry(item.account.to_string())
            .or_insert(Decimal::ZERO) += tx.amount;
        top_items.push(item);
    }

    let mut category_vec: Vec<(String, Decimal)> = by_category.into_iter().collect();
    category_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut account_vec: Vec<(String, Decimal)> = by_account.into_iter().collect();
    account_vec.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    top_items.sort_by(|a, b| {
        b.transaction
            .amount
            .partial_cmp(&a.transaction.amount)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!(
        "Summary (from={} to={})",
        from_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "ALL".to_string()),
        to_date
            .map(|d| d.to_string())
            .unwrap_or_else(|| "ALL".to_string())
    );
    println!("Total expenses: {}", total.round_dp(2));

    println!("By category:");
    for (cat, amt) in category_vec {
        println!("  {cat}: {}", amt.round_dp(2));
    }

    println!("By account:");
    for (acct, amt) in account_vec {
        let pct = if total == Decimal::ZERO {
            Decimal::ZERO
        } else {
            (amt / total * Decimal::new(100, 0)).round_dp(2)
        };
        println!("  {acct}: {} ({}%)", amt.round_dp(2), pct);
    }

    println!("Top items:");
    for item in top_items.into_iter().take(10) {
        let tx = item.transaction;
        println!(
            "  {}  {}  {}  ({})",
            tx.date,
            tx.amount.round_dp(2),
            tx.description,
            item.account
        );
    }

    if verbose {
        eprintln!(
            "loaded {} statements with {} warnings",
            manager.statements().len(),
            warnings
        );
    }
}

fn resolve_workdir() -> PathBuf {
    let workdir = match env::var("TALLY42_WORKDIR") {
        Ok(val) => PathBuf::from(val),
        Err(_) => env::current_dir().unwrap_or_else(|err| {
            eprintln!("error: failed to get current dir: {err}");
            std::process::exit(1);
        }),
    };

    if !workdir.is_dir() {
        eprintln!("error: workdir is not a directory: {}", workdir.display());
        std::process::exit(1);
    }
    workdir
}

fn load_statements(workdir: &Path, verbose: bool) -> (Vec<Statement>, usize) {
    let mut statements = Vec::new();
    let mut errors = 0usize;

    for entry in WalkDir::new(workdir)
        .into_iter()
        .filter_entry(|e| !should_skip_dir(e))
        .filter_map(|e| e.ok())
    {
        if !is_toml_file(entry.path()) {
            continue;
        }

        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("warning: failed to read {}: {err}", path.display());
                errors += 1;
                continue;
            }
        };

        match toml::from_str::<Statement>(&content) {
            Ok(stmt) => {
                if verbose {
                    eprintln!(
                        "loaded: {} (account={}, closing-date={})",
                        path.display(),
                        stmt.account,
                        stmt.closing_date
                    );
                }
                statements.push(stmt);
            }
            Err(err) => {
                eprintln!("warning: failed to parse {}: {err}", path.display());
                errors += 1;
            }
        }
    }

    (statements, errors)
}

fn parse_date_arg(s: &str) -> Date {
    let fmt = time::format_description::parse("[year]-[month]-[day]").unwrap_or_else(|err| {
        eprintln!("error: failed to build date format: {err}");
        std::process::exit(1);
    });
    Date::parse(s, &fmt).unwrap_or_else(|err| {
        eprintln!("error: invalid date '{s}': {err}");
        std::process::exit(1);
    })
}
