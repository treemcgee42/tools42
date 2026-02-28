use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use tli42::cmd::CmdBuilder;
use tli42::repl::{Action, CommandInputs, CompletionItem, Repl, RunOnceOutcome};

#[test]
fn public_repl_register_and_run_once_captures_vars() {
    let mut repl = Repl::new();
    let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_clone = Rc::clone(&seen);

    let mut builder = CmdBuilder::new();
    builder.literals(&["show", "ip"]).positional_args(2);
    let cmd = builder.build();

    repl.register_mode_command(
        0,
        &cmd,
        Box::new(move |_, inputs| {
            *seen_clone.borrow_mut() = inputs.positionals.clone();
            Ok(Action::None)
        }),
    )
    .expect("register command");

    let outcome = repl.run_once("show ip eth0 brief").expect("run_once");
    assert_eq!(outcome, RunOnceOutcome::ActionApplied(Action::None));
    assert_eq!(
        &*seen.borrow(),
        &vec!["eth0".to_string(), "brief".to_string()]
    );
}

#[test]
fn public_repl_register_and_run_once_captures_labeled_args() {
    let mut repl = Repl::new();
    let seen: Rc<RefCell<Option<CommandInputs>>> = Rc::new(RefCell::new(None));
    let seen_clone = Rc::clone(&seen);

    let mut builder = CmdBuilder::new();
    builder
        .literals(&["create", "account"])
        .labeled_arg("name")
        .labeled_arg("currency");
    let cmd = builder.build();

    repl.register_mode_command(
        0,
        &cmd,
        Box::new(move |_, inputs| {
            *seen_clone.borrow_mut() = Some(inputs.clone());
            Ok(Action::None)
        }),
    )
    .expect("register command");

    let outcome = repl
        .run_once("create account name \"cash account\" currency USD")
        .expect("run_once");
    assert_eq!(outcome, RunOnceOutcome::ActionApplied(Action::None));
    assert_eq!(
        *seen.borrow(),
        Some(CommandInputs {
            positionals: vec![],
            labeled: BTreeMap::from([
                ("currency".to_string(), "USD".to_string()),
                ("name".to_string(), "cash account".to_string()),
            ]),
        })
    );
}

#[test]
fn public_repl_meta_exit_returns_exit_action_at_root() {
    let mut repl = Repl::new();

    let outcome = repl.run_once("exit").expect("run_once");
    assert_eq!(outcome, RunOnceOutcome::ActionApplied(Action::Exit));
}

#[test]
fn public_repl_question_returns_completions() {
    let mut repl = Repl::new();

    let mut builder = CmdBuilder::new();
    builder.literals(&["write"]);
    let write_cmd = builder.build();
    repl.register_mode_command(0, &write_cmd, Box::new(|_, _| Ok(Action::None)))
        .expect("register write");

    let mut builder = CmdBuilder::new();
    builder.literals(&["show"]);
    let show_cmd = builder.build();
    repl.register_mode_command(0, &show_cmd, Box::new(|_, _| Ok(Action::None)))
        .expect("register show");

    let outcome = repl.run_once("?").expect("run_once");
    assert_eq!(
        outcome,
        RunOnceOutcome::Completions(vec![
            CompletionItem {
                token: "show".to_string(),
                doc: None,
            },
            CompletionItem {
                token: "write".to_string(),
                doc: None,
            },
        ])
    );
}

#[test]
fn public_repl_question_returns_doc_annotated_completion() {
    let mut repl = Repl::new();

    let mut builder = CmdBuilder::new();
    builder.literals(&["write"]);
    let write_cmd = builder.build();
    repl.register_mode_command(0, &write_cmd, Box::new(|_, _| Ok(Action::None)))
        .expect("register write");
    repl.set_edge_doc(0, "write", "enter write mode")
        .expect("set edge doc");

    let outcome = repl.run_once("?").expect("run_once");
    assert_eq!(
        outcome,
        RunOnceOutcome::Completions(vec![CompletionItem {
            token: "write".to_string(),
            doc: Some("enter write mode".to_string()),
        }])
    );
}

#[test]
fn public_repl_question_includes_ret_for_accepting_prefix_with_doc() {
    let mut repl = Repl::new();

    let mut builder = CmdBuilder::new();
    builder.literals(&["foo"]);
    let foo_cmd = builder.build();
    repl.register_mode_command(0, &foo_cmd, Box::new(|_, _| Ok(Action::None)))
        .expect("register foo");

    let mut builder = CmdBuilder::new();
    builder.literals(&["foo", "bar"]);
    let bar_cmd = builder.build();
    repl.register_mode_command(0, &bar_cmd, Box::new(|_, _| Ok(Action::None)))
        .expect("register foo bar");

    repl.set_command_doc(0, "foo", "run foo")
        .expect("set command doc");
    repl.set_edge_doc(0, "foo bar", "run bar")
        .expect("set edge doc");

    let outcome = repl.run_once("foo ?").expect("run_once");
    assert_eq!(
        outcome,
        RunOnceOutcome::Completions(vec![
            CompletionItem {
                token: "RET".to_string(),
                doc: Some("run foo".to_string()),
            },
            CompletionItem {
                token: "bar".to_string(),
                doc: Some("run bar".to_string()),
            },
        ])
    );
}
