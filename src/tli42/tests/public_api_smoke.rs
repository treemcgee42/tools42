use std::cell::RefCell;
use std::rc::Rc;

use tli42::cmd::CmdBuilder;
use tli42::repl::{Action, Repl, RunOnceOutcome};

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
            *seen_clone.borrow_mut() = inputs.to_vec();
            Ok(Action::None)
        }),
    )
    .expect("register command");

    let outcome = repl.run_once("show ip eth0 brief").expect("run_once");
    assert_eq!(outcome, RunOnceOutcome::ActionApplied(Action::None));
    assert_eq!(&*seen.borrow(), &vec!["eth0".to_string(), "brief".to_string()]);
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
        RunOnceOutcome::Completions(vec!["show".to_string(), "write".to_string()])
    );
}
