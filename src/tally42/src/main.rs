mod core;

use clap::{Parser, Subcommand};
use core::Core;

#[derive(Parser, Debug)]
#[command(name = "tally42")]
#[command(about = "Introspect personal financial statements", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
    DeleteDb,
}

fn main() {
    let args = Args::parse();

    match args.command {
        Command::Init => init_command_or_exit(),
        Command::DeleteDb => delete_db_or_exit(),
    }
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
