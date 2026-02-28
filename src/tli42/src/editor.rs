use crate::repl::{CompletionItem, format_completions};
use std::io::{self, Write};

pub(crate) enum EditorRead {
    Line(String),
    Interrupted,
    Eof,
}

pub(crate) trait LineEditor {
    fn read_line(&mut self, prompt: &str) -> io::Result<EditorRead>;

    fn print_completions(&mut self, items: &[CompletionItem]) -> io::Result<()>;

    fn add_history_entry(&mut self, _line: &str) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) struct BasicEditor {
    stdin: io::Stdin,
    stdout: io::Stdout,
}

impl BasicEditor {
    pub(crate) fn new() -> Self {
        Self {
            stdin: io::stdin(),
            stdout: io::stdout(),
        }
    }
}

impl LineEditor for BasicEditor {
    fn read_line(&mut self, prompt: &str) -> io::Result<EditorRead> {
        let mut line = String::new();
        write!(self.stdout, "{}", prompt)?;
        self.stdout.flush()?;

        let bytes = self.stdin.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(EditorRead::Eof);
        }

        Ok(EditorRead::Line(line))
    }

    fn print_completions(&mut self, items: &[CompletionItem]) -> io::Result<()> {
        write!(self.stdout, "{}", format_completions(items))?;
        self.stdout.flush()
    }
}
