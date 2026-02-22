mod core;

use clap::{Parser, Subcommand};
use core::UserDataManager;

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
    let user_data_manager = user_data_manager_or_exit();

    match args.command {
        Command::Init => init_command_or_exit(&user_data_manager),
        Command::DeleteDb => delete_db_or_exit(&user_data_manager),
    }
}

fn user_data_manager_or_exit() -> UserDataManager {
    UserDataManager::from_environment().unwrap_or_else(|err| {
        eprintln!("error: failed to resolve user data directory: {err}");
        std::process::exit(1);
    })
}

fn init_command_or_exit(manager: &UserDataManager) {
    manager.open_db().unwrap_or_else(|err| {
        eprintln!("error: failed to initialize database: {err}");
        std::process::exit(1);
    });
    println!("initialized database at {}", manager.db_path().display());
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
