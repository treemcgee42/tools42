mod core;

use core::{Account, Core};
use tli42::cmd::CmdBuilder;
use tli42::repl::{Action, CommandInputs, CompletionItem, HandlerError, Repl, ReplError};

fn main() {
    let mut repl = build_repl_or_exit();
    repl.run().unwrap_or_else(|err| {
        eprintln!("error: repl runtime failed: {err}");
        std::process::exit(1);
    });
}

fn build_repl_or_exit() -> Repl {
    build_repl().unwrap_or_else(|err| {
        eprintln!("error: failed to build repl: {err:?}");
        std::process::exit(1);
    })
}

fn build_repl() -> Result<Repl, ReplError> {
    let mut repl = Repl::new();
    let write_mode_id = register_write_mode(&mut repl)?;
    register_root_commands(&mut repl, write_mode_id)?;
    register_write_mode_commands(&mut repl, write_mode_id)?;
    Ok(repl)
}

fn register_write_mode(repl: &mut Repl) -> Result<u32, ReplError> {
    let write_mode_id = repl.add_mode("write");
    Ok(write_mode_id)
}

fn register_root_commands(repl: &mut Repl, write_mode_id: u32) -> Result<(), ReplError> {
    let mut write = CmdBuilder::new();
    write
        .literal_with_doc("write", "enter write mode")
        .command_doc("enter write mode commands");
    let write_cmd = write.build();

    repl.register_mode_command(
        0,
        &write_cmd,
        Box::new(move |_, _| Ok(Action::PushMode(write_mode_id))),
    )?;

    let mut show_accounts = CmdBuilder::new();
    show_accounts
        .literal_with_doc("show", "display read-only information")
        .literal_with_doc("accounts", "list accounts")
        .command_doc("list all accounts in the database");
    let show_accounts_cmd = show_accounts.build();
    repl.register_mode_command(
        0,
        &show_accounts_cmd,
        Box::new(|_, _| {
            show_accounts_command()?;
            Ok(Action::None)
        }),
    )?;

    Ok(())
}

fn register_write_mode_commands(repl: &mut Repl, write_mode_id: u32) -> Result<(), ReplError> {
    let mut create_account = CmdBuilder::new();
    create_account
        .literal_with_doc("create", "create data in the tally database")
        .literal_with_doc("account", "create an account")
        .labeled_arg_with_doc("name", "set the account name")
        .labeled_arg_with_doc("currency", "set the account currency")
        .labeled_arg_with_doc("note", "set the account note");
    let create_account_cmd = create_account.build();
    repl.register_mode_command(
        write_mode_id,
        &create_account_cmd,
        Box::new(|_, inputs| {
            create_account_command(inputs)?;
            Ok(Action::None)
        }),
    )?;

    let mut init = CmdBuilder::new();
    init.literal_with_doc("init", "initialize the tally database")
        .command_doc("create the tally database and schema");
    let init_cmd = init.build();
    repl.register_mode_command(
        write_mode_id,
        &init_cmd,
        Box::new(|_, _| {
            init_command()?;
            Ok(Action::None)
        }),
    )?;

    let mut delete_db = CmdBuilder::new();
    delete_db
        .literal_with_doc("delete-db", "delete the tally database file")
        .command_doc("remove the tally database from disk");
    let delete_db_cmd = delete_db.build();
    repl.register_mode_command(
        write_mode_id,
        &delete_db_cmd,
        Box::new(|_, _| {
            delete_db_command()?;
            Ok(Action::None)
        }),
    )?;

    Ok(())
}

fn init_command() -> Result<(), HandlerError> {
    let core = Core::from_environment().map_err(|err| HandlerError(err.to_string()))?;
    core.init()
        .map_err(|err| HandlerError(err.to_string()))?;
    println!("initialized database at {}", core.db_path().display());
    Ok(())
}

fn delete_db_command() -> Result<(), HandlerError> {
    match Core::delete_db_from_environment().map_err(|err| HandlerError(err.to_string()))? {
        (path, true) => println!("deleted database at {}", path.display()),
        (path, false) => println!("database not found at {}", path.display()),
    };
    Ok(())
}

fn show_accounts_command() -> Result<(), HandlerError> {
    let core = Core::from_environment().map_err(|err| HandlerError(err.to_string()))?;
    let accounts = core.list_accounts().map_err(|err| HandlerError(err.to_string()))?;
    print!("{}", format_accounts(&accounts));
    Ok(())
}

fn create_account_command(inputs: &CommandInputs) -> Result<(), HandlerError> {
    let name = inputs
        .labeled
        .get("name")
        .ok_or_else(|| HandlerError("missing required labeled input: name".to_string()))?;
    let currency = inputs
        .labeled
        .get("currency")
        .ok_or_else(|| HandlerError("missing required labeled input: currency".to_string()))?;
    let note = inputs
        .labeled
        .get("note")
        .ok_or_else(|| HandlerError("missing required labeled input: note".to_string()))?;

    let core = Core::from_environment().map_err(|err| HandlerError(err.to_string()))?;
    let account = core
        .create_account(name, currency, note)
        .map_err(|err| HandlerError(err.to_string()))?;
    print!("{}", format_created_account(&account));
    Ok(())
}

fn format_accounts(accounts: &[Account]) -> String {
    if accounts.is_empty() {
        return "accounts: (none)\n".to_string();
    }

    let width = accounts.iter().map(|account| account.name.len()).max().unwrap_or(0);
    let mut out = String::from("accounts:\n");
    for account in accounts {
        let status = if account.is_closed { "closed" } else { "open" };
        out.push_str(&format!(
            "  {:<width$}  {}  {}\n",
            account.name,
            account.currency,
            status,
            width = width
        ));
    }
    out
}

fn format_created_account(account: &Account) -> String {
    format!("created account {} ({})\n", account.name, account.currency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tli42::repl::RunOnceOutcome;

    #[test]
    fn write_command_pushes_write_mode() {
        let mut repl = build_repl().expect("repl should build");

        let outcome = repl.run_once("write").expect("run_once should succeed");
        assert_eq!(outcome, RunOnceOutcome::ActionApplied(Action::PushMode(1)));
        assert_eq!(repl.current_mode_id().expect("current mode id"), 1);
    }

    #[test]
    fn question_shows_annotated_write_mode_completions() {
        let mut repl = build_repl().expect("repl should build");
        repl.run_once("write").expect("enter write mode");

        let outcome = repl.run_once("?").expect("completion should succeed");
        assert_eq!(
            outcome,
            RunOnceOutcome::Completions(vec![
                CompletionItem {
                    token: "create".to_string(),
                    doc: Some("create data in the tally database".to_string()),
                },
                CompletionItem {
                    token: "delete-db".to_string(),
                    doc: Some("delete the tally database file".to_string()),
                },
                CompletionItem {
                    token: "init".to_string(),
                    doc: Some("initialize the tally database".to_string()),
                },
            ])
        );
    }

    #[test]
    fn question_shows_annotated_root_completions() {
        let mut repl = build_repl().expect("repl should build");

        let outcome = repl.run_once("?").expect("completion should succeed");
        assert_eq!(
            outcome,
            RunOnceOutcome::Completions(vec![
                CompletionItem {
                    token: "show".to_string(),
                    doc: Some("display read-only information".to_string()),
                },
                CompletionItem {
                    token: "write".to_string(),
                    doc: Some("enter write mode".to_string()),
                },
            ])
        );
    }

    #[test]
    fn show_question_lists_accounts_subcommand() {
        let mut repl = build_repl().expect("repl should build");

        let outcome = repl.run_once("show ?").expect("completion should succeed");
        assert_eq!(
            outcome,
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "accounts".to_string(),
                doc: Some("list accounts".to_string()),
            }])
        );
    }

    #[test]
    fn create_question_lists_account_subcommand() {
        let mut repl = build_repl().expect("repl should build");
        repl.run_once("write").expect("enter write mode");

        let outcome = repl.run_once("create ?").expect("completion should succeed");
        assert_eq!(
            outcome,
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "account".to_string(),
                doc: Some("create an account".to_string()),
            }])
        );
    }

    #[test]
    fn create_account_question_lists_name_label() {
        let mut repl = build_repl().expect("repl should build");
        repl.run_once("write").expect("enter write mode");

        let outcome = repl
            .run_once("create account ?")
            .expect("completion should succeed");
        assert_eq!(
            outcome,
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "name".to_string(),
                doc: Some("set the account name".to_string()),
            }])
        );
    }

    #[test]
    fn create_account_after_name_and_currency_lists_note() {
        let mut repl = build_repl().expect("repl should build");
        repl.run_once("write").expect("enter write mode");

        let outcome = repl
            .run_once("create account name cash currency USD ?")
            .expect("completion should succeed");
        assert_eq!(
            outcome,
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "note".to_string(),
                doc: Some("set the account note".to_string()),
            }])
        );
    }

    #[test]
    fn show_accounts_command_is_registered() {
        let mut repl = build_repl().expect("repl should build");

        let outcome = repl
            .run_once("show accounts")
            .expect("run_once should succeed");
        assert!(matches!(
            outcome,
            RunOnceOutcome::ActionApplied(Action::None) | RunOnceOutcome::HandlerError(_)
        ));
    }

    #[test]
    fn create_account_command_is_registered() {
        let mut repl = build_repl().expect("repl should build");
        repl.run_once("write").expect("enter write mode");

        let outcome = repl
            .run_once("create account name cash currency USD note wallet")
            .expect("run_once should succeed");
        assert!(matches!(
            outcome,
            RunOnceOutcome::ActionApplied(Action::None) | RunOnceOutcome::HandlerError(_)
        ));
    }

    #[test]
    fn format_accounts_renders_empty_state() {
        assert_eq!(format_accounts(&[]), "accounts: (none)\n");
    }

    #[test]
    fn format_accounts_renders_compact_table() {
        let open_id = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let closed_id = uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        let output = format_accounts(&[
            Account {
                id: open_id,
                parent_id: None,
                name: "checking".to_string(),
                currency: "USD".to_string(),
                is_closed: false,
                created_at: "2026-02-28 00:00:00".to_string(),
                note: None,
            },
            Account {
                id: closed_id,
                parent_id: None,
                name: "longer-savings".to_string(),
                currency: "EUR".to_string(),
                is_closed: true,
                created_at: "2026-02-28 00:00:00".to_string(),
                note: Some("archived".to_string()),
            },
        ]);

        assert_eq!(
            output,
            "accounts:\n  checking        USD  open\n  longer-savings  EUR  closed\n"
        );
    }

    #[test]
    fn format_created_account_renders_compact_summary() {
        let account = Account {
            id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            parent_id: None,
            name: "cash".to_string(),
            currency: "USD".to_string(),
            is_closed: false,
            created_at: "2026-02-28 00:00:00".to_string(),
            note: Some("wallet".to_string()),
        };

        assert_eq!(format_created_account(&account), "created account cash (USD)\n");
    }
}
