mod core;

use core::Core;
use tli42::cmd::CmdBuilder;
use tli42::repl::{Action, Repl, ReplError};

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
    write.literals(&["write"]);
    let write_cmd = write.build();

    repl.register_mode_command(
        0,
        &write_cmd,
        Box::new(move |_, _| Ok(Action::PushMode(write_mode_id))),
    )?;

    Ok(())
}

fn register_write_mode_commands(repl: &mut Repl, write_mode_id: u32) -> Result<(), ReplError> {
    let mut init = CmdBuilder::new();
    init.literals(&["init"]);
    let init_cmd = init.build();
    repl.register_mode_command(
        write_mode_id,
        &init_cmd,
        Box::new(|_, _| {
            init_command_or_exit();
            Ok(Action::None)
        }),
    )?;

    let mut delete_db = CmdBuilder::new();
    delete_db.literals(&["delete-db"]);
    let delete_db_cmd = delete_db.build();
    repl.register_mode_command(
        write_mode_id,
        &delete_db_cmd,
        Box::new(|_, _| {
            delete_db_or_exit();
            Ok(Action::None)
        }),
    )?;

    Ok(())
}

fn init_command_or_exit() {
    let core = Core::from_environment().unwrap_or_else(|err| {
        eprintln!("error: failed to initialize core: {err}");
        std::process::exit(1);
    });
    core.init().unwrap_or_else(|err| {
        eprintln!("error: failed to initialize core: {err}");
        std::process::exit(1);
    });
    println!("initialized database at {}", core.db_path().display());
}

fn delete_db_or_exit() {
    match Core::delete_db_from_environment() {
        Ok((path, true)) => println!("deleted database at {}", path.display()),
        Ok((path, false)) => println!("database not found at {}", path.display()),
        Err(err) => {
            eprintln!("error: failed to delete database: {err}");
            std::process::exit(1);
        }
    }
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
}
