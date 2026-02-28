use crate::repl::{CompletionItem, CompletionSnapshot, format_completions};
use std::io::{self, IsTerminal, Write};

pub(crate) enum EditorRead {
    Line(String),
    Interrupted,
    Eof,
}

pub(crate) trait LineEditor {
    fn read_line(&mut self, prompt: &str) -> io::Result<EditorRead>;

    fn print_completions(&mut self, items: &[CompletionItem]) -> io::Result<()>;

    fn set_completion_snapshot(&mut self, _snapshot: CompletionSnapshot) -> io::Result<()> {
        Ok(())
    }

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
    state: std::sync::Arc<std::sync::Mutex<RustylineState>>,
}

#[cfg(feature = "rustyline")]
impl RustylineEditor {
    pub(crate) fn new() -> io::Result<Self> {
        use rustyline::EventHandler;

        let mut editor = rustyline::DefaultEditor::new()
            .map_err(|err| io::Error::other(format!("rustyline init error: {}", err)))?;
        let printer = editor
            .create_external_printer()
            .map_err(|err| io::Error::other(format!("rustyline printer error: {}", err)))?;
        let state = std::sync::Arc::new(std::sync::Mutex::new(RustylineState {
            snapshot: None,
            printer: Box::new(printer),
        }));

        editor.bind_sequence(
            rustyline::KeyEvent::from('?'),
            EventHandler::Conditional(Box::new(HelpHandler {
                state: std::sync::Arc::clone(&state),
            })),
        );
        editor.bind_sequence(
            rustyline::KeyEvent(rustyline::KeyCode::Tab, rustyline::Modifiers::NONE),
            EventHandler::Conditional(Box::new(TabHandler {
                state: std::sync::Arc::clone(&state),
            })),
        );

        Ok(Self { editor, state })
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

    fn set_completion_snapshot(&mut self, snapshot: CompletionSnapshot) -> io::Result<()> {
        self.state
            .lock()
            .expect("rustyline state lock poisoned")
            .snapshot = Some(snapshot);
        Ok(())
    }

    fn add_history_entry(&mut self, line: &str) -> io::Result<()> {
        self.editor
            .add_history_entry(line)
            .map(|_| ())
            .map_err(|err| io::Error::other(format!("rustyline history error: {}", err)))
    }
}

#[cfg(feature = "rustyline")]
struct RustylineState {
    snapshot: Option<CompletionSnapshot>,
    printer: Box<dyn rustyline::ExternalPrinter + Send>,
}

#[cfg(feature = "rustyline")]
struct HelpHandler {
    state: std::sync::Arc<std::sync::Mutex<RustylineState>>,
}

#[cfg(feature = "rustyline")]
impl rustyline::ConditionalEventHandler for HelpHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: rustyline::RepeatCount,
        _positive: bool,
        ctx: &rustyline::EventContext,
    ) -> Option<rustyline::Cmd> {
        let prefix = &ctx.line()[..ctx.pos()];
        let mut state = self.state.lock().expect("rustyline state lock poisoned");
        let msg = match state
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.complete_prefix(prefix))
            .transpose()
        {
            Ok(Some(items)) => format_completions(&items),
            Ok(None) => format_completions(&[]),
            Err(err) => format!("\ncompletion error: {:?}\n\n", err),
        };
        let _ = state.printer.print(msg);
        Some(rustyline::Cmd::Noop)
    }
}

#[cfg(feature = "rustyline")]
struct TabHandler {
    state: std::sync::Arc<std::sync::Mutex<RustylineState>>,
}

#[cfg(feature = "rustyline")]
impl rustyline::ConditionalEventHandler for TabHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: rustyline::RepeatCount,
        _positive: bool,
        ctx: &rustyline::EventContext,
    ) -> Option<rustyline::Cmd> {
        let prefix = &ctx.line()[..ctx.pos()];
        let state = self.state.lock().expect("rustyline state lock poisoned");
        let Some(snapshot) = state.snapshot.as_ref() else {
            return Some(rustyline::Cmd::Noop);
        };
        let Some(completion) = snapshot.tab_completion(prefix).ok().flatten() else {
            return Some(rustyline::Cmd::Noop);
        };

        Some(rustyline::Cmd::Insert(1, completion.insert_suffix))
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
