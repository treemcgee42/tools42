mod cmd;
mod mode;
mod repl;
mod sm;
mod trie;

use std::io;

fn main() -> io::Result<()> {
    let mut repl = repl::Repl::new();
    repl.run()
}
