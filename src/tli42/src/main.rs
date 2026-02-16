mod trie;

use std::io::{self, Write};

struct Repl;

impl Repl {
    fn new() -> Self {
        Self
    }

    fn run(&self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut line = String::new();

        loop {
            line.clear();
            print!("> ");
            stdout.flush()?;

            let bytes = stdin.read_line(&mut line)?;
            if bytes == 0 {
                break;
            }

            let input = line.trim();
            if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                break;
            }

            println!("echo: {}", input);
        }

        Ok(())
    }
}

fn main() -> io::Result<()> {
    let repl = Repl::new();
    repl.run()
}
