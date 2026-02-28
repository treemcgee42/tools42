use crate::repl::{CompletionItem, format_completions};
use std::io::{self, IsTerminal, Write};

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

pub(crate) fn should_use_rustyline_backend(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
    term: Option<&str>,
) -> bool {
    #[cfg(feature = "rustyline")]
    {
        stdin_is_terminal && stdout_is_terminal && !matches!(term, Some("dumb") | None)
    }

    #[cfg(not(feature = "rustyline"))]
    {
        let _ = (stdin_is_terminal, stdout_is_terminal, term);
        false
    }
}

pub(crate) fn prefer_rustyline_backend() -> bool {
    should_use_rustyline_backend(
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
        std::env::var("TERM").ok().as_deref(),
    )
}

#[cfg(feature = "rustyline")]
pub(crate) struct RustylineEditor {
    editor: rustyline::DefaultEditor,
}

#[cfg(feature = "rustyline")]
impl RustylineEditor {
    pub(crate) fn new() -> io::Result<Self> {
        let editor = rustyline::DefaultEditor::new()
            .map_err(|err| io::Error::other(format!("rustyline init error: {}", err)))?;
        Ok(Self { editor })
    }
}

#[cfg(feature = "rustyline")]
impl LineEditor for RustylineEditor {
    fn read_line(&mut self, prompt: &str) -> io::Result<EditorRead> {
        match self.editor.readline(prompt) {
            Ok(line) => Ok(EditorRead::Line(line)),
            Err(rustyline::error::ReadlineError::Interrupted) => Ok(EditorRead::Interrupted),
            Err(rustyline::error::ReadlineError::Eof) => Ok(EditorRead::Eof),
            Err(err) => Err(io::Error::other(format!("rustyline read error: {}", err))),
        }
    }

    fn print_completions(&mut self, items: &[CompletionItem]) -> io::Result<()> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", format_completions(items))?;
        stdout.flush()
    }

    fn add_history_entry(&mut self, line: &str) -> io::Result<()> {
        self.editor
            .add_history_entry(line)
            .map(|_| ())
            .map_err(|err| io::Error::other(format!("rustyline history error: {}", err)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_rustyline_backend_requires_interactive_non_dumb_terminal() {
        assert!(!should_use_rustyline_backend(false, true, Some("xterm-256color")));
        assert!(!should_use_rustyline_backend(true, false, Some("xterm-256color")));
        assert!(!should_use_rustyline_backend(true, true, Some("dumb")));
        assert!(!should_use_rustyline_backend(true, true, None));

        #[cfg(feature = "rustyline")]
        assert!(should_use_rustyline_backend(
            true,
            true,
            Some("xterm-256color")
        ));

        #[cfg(not(feature = "rustyline"))]
        assert!(!should_use_rustyline_backend(
            true,
            true,
            Some("xterm-256color")
        ));
    }
}
