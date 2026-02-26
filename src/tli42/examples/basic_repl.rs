use tli42::cmd::CmdBuilder;
use tli42::repl::{Action, Repl};

fn main() -> std::io::Result<()> {
    let mut repl = Repl::new();

    let mut hello = CmdBuilder::new();
    hello.literals(&["hello"]).positional_args(1);
    let hello_cmd = hello.build();
    repl.register_mode_command(
        0,
        &hello_cmd,
        Box::new(|_, inputs| {
            let name = inputs.first().map(String::as_str).unwrap_or("world");
            println!("hello, {}", name);
            Ok(Action::None)
        }),
    )
    .expect("register hello command");

    let mut quit = CmdBuilder::new();
    quit.literals(&["quit"]);
    let quit_cmd = quit.build();
    repl.register_mode_command(0, &quit_cmd, Box::new(|_, _| Ok(Action::Exit)))
        .expect("register quit command");

    repl.run()
}
