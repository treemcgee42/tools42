use crate::{cmd, editor, mode, sm};
use std::fmt;
use std::collections::BTreeMap;
use std::io;

pub type ModeId = u32;
pub type CommandId = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    PushMode(ModeId),
    PopMode,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerError(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInputs {
    pub positionals: Vec<String>,
    pub labeled: BTreeMap<String, String>,
}

pub type HandlerResult = Result<Action, HandlerError>;
pub type Handler = Box<dyn FnMut(&mut Repl, &CommandInputs) -> HandlerResult>;
const RET_COMPLETION_TOKEN: &str = "RET";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRegistrationError {
    InvalidState(u32),
    MultipleVarEdges(u32),
    DuplicateLiteralEdges {
        state: u32,
        literal: String,
    },
    DuplicateCommandPath {
        existing: CommandId,
        attempted: CommandId,
    },
    DuplicateLabeledArg {
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplError {
    InvalidModeId(ModeId),
    InvalidCommandId(CommandId),
    InvalidDocStem,
    DocPathNotFound(String),
    DocPathAmbiguous(String),
    CommandDocTargetNotTerminal(String),
    EmptyModeStack,
    CannotPopRootMode,
    CmdInsert(CommandRegistrationError),
}

impl From<sm::CmdInsertError> for ReplError {
    fn from(value: sm::CmdInsertError) -> Self {
        let mapped = match value {
            sm::CmdInsertError::InvalidState(state) => {
                CommandRegistrationError::InvalidState(state as u32)
            }
            sm::CmdInsertError::MultipleVarEdges(state) => {
                CommandRegistrationError::MultipleVarEdges(state as u32)
            }
            sm::CmdInsertError::DuplicateLiteralEdges { state, literal } => {
                CommandRegistrationError::DuplicateLiteralEdges {
                    state: state as u32,
                    literal,
                }
            }
            sm::CmdInsertError::DuplicateCommandPath {
                existing,
                attempted,
            } => CommandRegistrationError::DuplicateCommandPath {
                existing,
                attempted,
            },
        };
        Self::CmdInsert(mapped)
    }
}

impl From<cmd::CmdSchemaError> for ReplError {
    fn from(value: cmd::CmdSchemaError) -> Self {
        let mapped = match value {
            cmd::CmdSchemaError::DuplicateLabeledArg { label } => {
                CommandRegistrationError::DuplicateLabeledArg { label }
            }
        };
        Self::CmdInsert(mapped)
    }
}

pub struct Repl {
    modes: Vec<mode::Mode>,
    stack: Vec<ModeId>,
    handlers: Vec<Handler>,
    capture_specs: Vec<Vec<cmd::CaptureKind>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub token: String,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOnceOutcome {
    Noop,
    Completions(Vec<CompletionItem>),
    UnknownCommand,
    IncompleteCommand,
    ParseError(ParseLineError),
    HandlerError(HandlerError),
    ActionApplied(Action),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletionRequest {
    exact_tokens: Vec<String>,
    partial: String,
}

enum ParsedCompletionRequest {
    NotARequest,
    Disabled,
    Request(CompletionRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseLineError {
    UnterminatedQuote,
    UnexpectedQuote,
    TrailingCharactersAfterQuote,
}

impl fmt::Display for ParseLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedQuote => write!(f, "unterminated quote"),
            Self::UnexpectedQuote => write!(f, "unexpected quote in unquoted token"),
            Self::TrailingCharactersAfterQuote => {
                write!(f, "unexpected trailing characters after closing quote")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedLine {
    tokens: Vec<String>,
    ends_with_whitespace: bool,
    ended_after_quoted_token: bool,
}

enum ParseState {
    Outside,
    Bare(String),
    Quoted(String),
    AfterQuoted(String),
}

fn parse_line(line: &str) -> Result<ParsedLine, ParseLineError> {
    let mut tokens = Vec::new();
    let mut state = ParseState::Outside;
    let mut ends_with_whitespace = false;

    for ch in line.chars() {
        ends_with_whitespace = ch.is_whitespace();
        state = match state {
            ParseState::Outside => {
                if ch.is_whitespace() {
                    ParseState::Outside
                } else if ch == '"' {
                    ParseState::Quoted(String::new())
                } else {
                    let mut buf = String::new();
                    buf.push(ch);
                    ParseState::Bare(buf)
                }
            }
            ParseState::Bare(mut buf) => {
                if ch.is_whitespace() {
                    tokens.push(buf);
                    ParseState::Outside
                } else if ch == '"' {
                    return Err(ParseLineError::UnexpectedQuote);
                } else {
                    buf.push(ch);
                    ParseState::Bare(buf)
                }
            }
            ParseState::Quoted(mut buf) => {
                if ch == '"' {
                    ParseState::AfterQuoted(buf)
                } else {
                    buf.push(ch);
                    ParseState::Quoted(buf)
                }
            }
            ParseState::AfterQuoted(buf) => {
                if ch.is_whitespace() {
                    tokens.push(buf);
                    ParseState::Outside
                } else {
                    return Err(ParseLineError::TrailingCharactersAfterQuote);
                }
            }
        };
    }

    let ended_after_quoted_token = matches!(state, ParseState::AfterQuoted(_));

    match state {
        ParseState::Outside => {}
        ParseState::Bare(buf) | ParseState::AfterQuoted(buf) => tokens.push(buf),
        ParseState::Quoted(_) => return Err(ParseLineError::UnterminatedQuote),
    }

    Ok(ParsedLine {
        tokens,
        ends_with_whitespace,
        ended_after_quoted_token,
    })
}

pub(crate) fn format_completions(items: &[CompletionItem]) -> String {
    let mut out = String::new();
    out.push('\n');
    out.push_str("Possible completions:\n");

    if items.is_empty() {
        out.push_str("  (none)\n\n");
        return out;
    }

    let width = items
        .iter()
        .filter(|item| !item.token.is_empty())
        .map(|item| item.token.len())
        .max()
        .unwrap_or(0);

    for item in items {
        match item.doc.as_deref() {
            Some(doc) => {
                out.push_str(&format!(
                    "  {:<width$}  {}\n",
                    item.token,
                    doc,
                    width = width
                ));
            }
            None => out.push_str(&format!("  {}\n", item.token)),
        }
    }

    out.push('\n');
    out
}

#[derive(Debug, Clone)]
pub(crate) struct CompletionSnapshot {
    modes: Vec<mode::Mode>,
    stack: Vec<ModeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabCompletion {
    pub insert_suffix: String,
}

impl CompletionSnapshot {
    fn current_mode_id(&self) -> Result<ModeId, ReplError> {
        self.stack.last().copied().ok_or(ReplError::EmptyModeStack)
    }

    fn current_mode(&self) -> Result<&mode::Mode, ReplError> {
        let id = self.current_mode_id()?;
        self.modes
            .get(id as usize)
            .ok_or(ReplError::InvalidModeId(id))
    }

    pub(crate) fn complete_prefix(&self, prefix: &str) -> Result<Vec<CompletionItem>, ReplError> {
        let Some(req) = Repl::completion_request_from_prefix(prefix) else {
            return Ok(Vec::new());
        };
        self.complete_request(&req)
    }

    fn complete_request(&self, req: &CompletionRequest) -> Result<Vec<CompletionItem>, ReplError> {
        let mode = self.current_mode()?;
        let mut state = mode.root_state();

        for token in &req.exact_tokens {
            let step = match mode.step(state, token) {
                Some(step) => step,
                None => return Ok(Vec::new()),
            };
            state = step.next_state;
        }

        let mut completions = mode
            .get_completions_with_docs(state, &req.partial)
            .into_iter()
            .map(|(token, doc)| CompletionItem {
                token: token.to_string(),
                doc: doc.map(str::to_string),
            })
            .collect::<Vec<_>>();

        if let Some(doc) = mode.command_doc_at(state)? {
            completions.push(CompletionItem {
                token: RET_COMPLETION_TOKEN.to_string(),
                doc: Some(doc.to_string()),
            });
        }
        completions.sort_by(|a, b| a.token.cmp(&b.token));
        Ok(completions)
    }

    pub(crate) fn tab_completion(&self, prefix: &str) -> Result<Option<TabCompletion>, ReplError> {
        let Some(req) = Repl::completion_request_from_prefix(prefix) else {
            return Ok(None);
        };
        let mode = self.current_mode()?;
        let mut state = mode.root_state();

        for token in &req.exact_tokens {
            let step = match mode.step(state, token) {
                Some(step) => step,
                None => return Ok(None),
            };
            state = step.next_state;
        }

        let mut candidates = mode
            .get_completions(state, &req.partial)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        candidates.sort();
        if candidates.is_empty() {
            return Ok(None);
        }

        let replacement = if candidates.len() == 1 {
            candidates[0].clone()
        } else {
            longest_common_prefix(&candidates)
        };

        if replacement == req.partial {
            return Ok(None);
        }

        let Some(insert_suffix) = replacement.strip_prefix(&req.partial) else {
            return Ok(None);
        };
        Ok(Some(TabCompletion {
            insert_suffix: insert_suffix.to_string(),
        }))
    }
}

fn longest_common_prefix(candidates: &[String]) -> String {
    let Some(first) = candidates.first() else {
        return String::new();
    };

    let mut prefix = first.clone();
    for candidate in &candidates[1..] {
        let matched_bytes = prefix
            .char_indices()
            .zip(candidate.chars())
            .take_while(|((_, a), b)| *a == *b)
            .map(|((idx, ch), _)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0);
        prefix.truncate(matched_bytes);
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

impl Repl {
    pub fn new() -> Self {
        Self {
            modes: vec![mode::Mode::new(0, "global")],
            stack: vec![0],
            handlers: Vec::new(),
            capture_specs: Vec::new(),
        }
    }

    pub fn current_mode_id(&self) -> Result<ModeId, ReplError> {
        self.stack.last().copied().ok_or(ReplError::EmptyModeStack)
    }

    fn current_mode(&self) -> Result<&mode::Mode, ReplError> {
        let id = self.current_mode_id()?;
        self.get_mode(id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn current_mode_mut(&mut self) -> Result<&mut mode::Mode, ReplError> {
        let id = self.current_mode_id()?;
        self.get_mode_mut(id)
    }

    pub fn add_mode(&mut self, name: impl Into<String>) -> ModeId {
        let id = self.modes.len() as ModeId;
        self.modes.push(mode::Mode::new(id, name));
        id
    }

    fn get_mode(&self, id: ModeId) -> Result<&mode::Mode, ReplError> {
        self.modes
            .get(id as usize)
            .ok_or(ReplError::InvalidModeId(id))
    }

    fn get_mode_mut(&mut self, id: ModeId) -> Result<&mut mode::Mode, ReplError> {
        self.modes
            .get_mut(id as usize)
            .ok_or(ReplError::InvalidModeId(id))
    }

    pub fn push_mode(&mut self, id: ModeId) -> Result<(), ReplError> {
        if (id as usize) >= self.modes.len() {
            return Err(ReplError::InvalidModeId(id));
        }
        self.stack.push(id);
        Ok(())
    }

    pub fn pop_mode(&mut self) -> Result<ModeId, ReplError> {
        if self.stack.is_empty() {
            return Err(ReplError::EmptyModeStack);
        }
        if self.stack.len() == 1 {
            return Err(ReplError::CannotPopRootMode);
        }
        Ok(self.stack.pop().expect("stack length checked above"))
    }

    fn register_handler(
        &mut self,
        handler: Handler,
        capture_spec: Vec<cmd::CaptureKind>,
    ) -> CommandId {
        let id = self.handlers.len() as CommandId;
        self.handlers.push(handler);
        self.capture_specs.push(capture_spec);
        id
    }

    pub fn register_command_in_mode(
        &mut self,
        mode_id: ModeId,
        cmd: &cmd::Cmd,
        command_id: CommandId,
    ) -> Result<(), ReplError> {
        cmd.capture_spec()?;
        let mode = self.get_mode_mut(mode_id)?;
        mode.insert_cmd(cmd, command_id)?;
        Ok(())
    }

    pub fn register_mode_command(
        &mut self,
        mode_id: ModeId,
        cmd: &cmd::Cmd,
        handler: Handler,
    ) -> Result<CommandId, ReplError> {
        let capture_spec = cmd.capture_spec()?;
        let command_id = self.register_handler(handler, capture_spec);
        if let Err(err) = self.register_command_in_mode(mode_id, cmd, command_id) {
            let _ = self.handlers.pop();
            let _ = self.capture_specs.pop();
            return Err(err);
        }
        Ok(command_id)
    }

    pub fn set_edge_doc(
        &mut self,
        mode_id: ModeId,
        stem: &str,
        doc: impl Into<String>,
    ) -> Result<(), ReplError> {
        let tokens = Self::normalize_stem(stem)?;
        let (parent_state, literal) = self.resolve_edge_doc_target(mode_id, &tokens)?;
        let mode = self.get_mode_mut(mode_id)?;
        let found = mode.set_literal_edge_doc(parent_state, literal, doc.into())?;
        if found {
            Ok(())
        } else {
            Err(ReplError::DocPathNotFound(tokens.join(" ")))
        }
    }

    pub fn set_command_doc(
        &mut self,
        mode_id: ModeId,
        stem: &str,
        doc: impl Into<String>,
    ) -> Result<(), ReplError> {
        let tokens = Self::normalize_stem(stem)?;
        let state = self.resolve_state_path(mode_id, &tokens)?;
        let mode = self.get_mode_mut(mode_id)?;
        let found = mode.set_command_doc(state, doc.into())?;
        if found {
            Ok(())
        } else {
            Err(ReplError::CommandDocTargetNotTerminal(tokens.join(" ")))
        }
    }

    fn prompt(&self) -> Result<String, ReplError> {
        if self.stack.is_empty() {
            return Err(ReplError::EmptyModeStack);
        }

        let mut names = Vec::with_capacity(self.stack.len());
        for mode_id in &self.stack {
            names.push(self.get_mode(*mode_id)?.name().to_string());
        }
        Ok(format!("{}> ", names.join("/")))
    }

    fn normalize_stem(stem: &str) -> Result<Vec<String>, ReplError> {
        let tokens = stem
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return Err(ReplError::InvalidDocStem);
        }
        Ok(tokens)
    }

    fn resolve_state_path(
        &self,
        mode_id: ModeId,
        tokens: &[String],
    ) -> Result<sm::StateId, ReplError> {
        let mode = self.get_mode(mode_id)?;
        let mut state = mode.root_state();
        for token in tokens {
            let Some(step) = mode.step(state, token) else {
                return Err(ReplError::DocPathNotFound(tokens.join(" ")));
            };
            state = step.next_state;
        }
        Ok(state)
    }

    fn resolve_edge_doc_target<'a>(
        &self,
        mode_id: ModeId,
        tokens: &'a [String],
    ) -> Result<(sm::StateId, &'a str), ReplError> {
        let mode = self.get_mode(mode_id)?;
        let mut state = mode.root_state();
        for token in &tokens[..tokens.len() - 1] {
            let Some(step) = mode.step(state, token) else {
                return Err(ReplError::DocPathNotFound(tokens.join(" ")));
            };
            state = step.next_state;
        }

        let literal = tokens.last().expect("tokens validated non-empty");
        let completions = mode.get_completions(state, literal);
        if !completions.iter().any(|candidate| *candidate == literal) {
            if completions.is_empty() {
                return Err(ReplError::DocPathNotFound(tokens.join(" ")));
            }
            return Err(ReplError::DocPathAmbiguous(tokens.join(" ")));
        }
        Ok((state, literal.as_str()))
    }

    fn parse_completion_request(&self, line: &str) -> ParsedCompletionRequest {
        let line = line.trim_end_matches(['\n', '\r']);
        let q_count = line.chars().filter(|&c| c == '?').count();
        if q_count != 1 || !line.ends_with('?') {
            return ParsedCompletionRequest::NotARequest;
        }

        let prefix = &line[..line.len() - 1];
        match Self::completion_request_from_prefix(prefix) {
            Some(req) => ParsedCompletionRequest::Request(req),
            None => ParsedCompletionRequest::Disabled,
        }
    }

    fn completion_request_from_prefix(prefix: &str) -> Option<CompletionRequest> {
        let parsed = parse_line(prefix).ok()?;

        if parsed.tokens.is_empty() {
            return Some(CompletionRequest {
                exact_tokens: Vec::new(),
                partial: String::new(),
            });
        }

        if parsed.ends_with_whitespace || parsed.ended_after_quoted_token {
            return Some(CompletionRequest {
                exact_tokens: parsed.tokens,
                partial: String::new(),
            });
        }

        let mut tokens = parsed.tokens;
        let partial = tokens.pop().unwrap_or_default();
        Some(CompletionRequest {
            exact_tokens: tokens,
            partial,
        })
    }

    fn complete_request(&self, req: &CompletionRequest) -> Result<Vec<CompletionItem>, ReplError> {
        self.completion_snapshot().complete_request(req)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn complete_prefix(&self, prefix: &str) -> Result<Vec<CompletionItem>, ReplError> {
        self.completion_snapshot().complete_prefix(prefix)
    }

    pub(crate) fn completion_snapshot(&self) -> CompletionSnapshot {
        CompletionSnapshot {
            modes: self.modes.clone(),
            stack: self.stack.clone(),
        }
    }

    fn complete_line(&self, line: &str) -> Result<Option<Vec<CompletionItem>>, ReplError> {
        match self.parse_completion_request(line) {
            ParsedCompletionRequest::NotARequest => Ok(None),
            ParsedCompletionRequest::Disabled => Ok(Some(Vec::new())),
            ParsedCompletionRequest::Request(req) => Ok(Some(self.complete_request(&req)?)),
        }
    }

    fn build_command_inputs(
        &self,
        command_id: CommandId,
        captures: &[String],
    ) -> Result<CommandInputs, HandlerError> {
        let Some(capture_spec) = self.capture_specs.get(command_id as usize) else {
            return Err(HandlerError(format!("invalid command id {}", command_id)));
        };
        if capture_spec.len() != captures.len() {
            return Err(HandlerError("internal capture mismatch".to_string()));
        }

        let mut positionals = Vec::new();
        let mut labeled = BTreeMap::new();
        for (kind, value) in capture_spec.iter().zip(captures) {
            match kind {
                cmd::CaptureKind::Positional { .. } => positionals.push(value.clone()),
                cmd::CaptureKind::Labeled { label, .. } => {
                    labeled.insert(label.clone(), value.clone());
                }
            }
        }

        Ok(CommandInputs {
            positionals,
            labeled,
        })
    }

    fn apply(&mut self, action: Action) -> Result<Action, ReplError> {
        match action {
            Action::None => Ok(Action::None),
            Action::PushMode(mode_id) => {
                self.push_mode(mode_id)?;
                Ok(Action::PushMode(mode_id))
            }
            Action::PopMode => {
                self.pop_mode()?;
                Ok(Action::PopMode)
            }
            Action::Exit => Ok(Action::Exit),
        }
    }

    fn invoke_handler(
        &mut self,
        command_id: CommandId,
        inputs: &CommandInputs,
    ) -> Result<Action, HandlerError> {
        let idx = command_id as usize;
        if idx >= self.handlers.len() || idx >= self.capture_specs.len() {
            return Err(HandlerError(format!("invalid command id {}", command_id)));
        }

        let mut handler = self.handlers.swap_remove(idx);
        let result = handler(self, inputs);

        if idx == self.handlers.len() {
            self.handlers.push(handler);
        } else {
            self.handlers.push(handler);
            let last = self.handlers.len() - 1;
            self.handlers.swap(idx, last);
        }

        result
    }

    fn should_add_history_entry(&self, line: &str) -> bool {
        if line.trim().is_empty() {
            return false;
        }
        matches!(
            self.parse_completion_request(line),
            ParsedCompletionRequest::NotARequest
        )
    }

    fn run_with_editor<E: editor::LineEditor>(&mut self, editor: &mut E) -> io::Result<()> {
        loop {
            editor.set_completion_snapshot(self.completion_snapshot())?;
            let prompt = self
                .prompt()
                .map_err(|e| io::Error::other(format!("repl prompt error: {:?}", e)))?;

            let line = match editor.read_line(&prompt)? {
                editor::EditorRead::Line(line) => line,
                editor::EditorRead::Interrupted => continue,
                editor::EditorRead::Eof => break,
            };

            if self.should_add_history_entry(&line) {
                editor.add_history_entry(&line)?;
            }

            match self
                .run_once(&line)
                .map_err(|e| io::Error::other(format!("repl runtime error: {:?}", e)))?
            {
                RunOnceOutcome::Noop => {}
                RunOnceOutcome::Completions(items) => {
                    editor.print_completions(&items)?;
                }
                RunOnceOutcome::UnknownCommand => {
                    println!("unknown command");
                }
                RunOnceOutcome::IncompleteCommand => {
                    println!("incomplete command");
                }
                RunOnceOutcome::ParseError(err) => {
                    println!("parse error: {}", err);
                }
                RunOnceOutcome::HandlerError(err) => {
                    println!("handler error: {}", err.0);
                }
                RunOnceOutcome::ActionApplied(Action::Exit) => break,
                RunOnceOutcome::ActionApplied(_) => {}
            }
        }

        Ok(())
    }

    pub fn run_once(&mut self, line: &str) -> Result<RunOnceOutcome, ReplError> {
        if let Some(completions) = self.complete_line(line)? {
            return Ok(RunOnceOutcome::Completions(completions));
        }

        let parsed = match parse_line(line) {
            Ok(parsed) => parsed,
            Err(err) => return Ok(RunOnceOutcome::ParseError(err)),
        };
        if parsed.tokens.is_empty() {
            return Ok(RunOnceOutcome::Noop);
        }
        let tokens = parsed.tokens;

        if tokens.first().map(String::as_str) == Some("exit") {
            let action = if self.current_mode_id()? == 0 {
                Action::Exit
            } else {
                Action::PopMode
            };
            let applied = self.apply(action)?;
            return Ok(RunOnceOutcome::ActionApplied(applied));
        }

        let (command_id, captures) = {
            let mode = self.current_mode()?;
            let mut state = mode.root_state();
            let mut captures = Vec::new();

            for token in &tokens {
                let step = match mode.step(state, token) {
                    Some(step) => step,
                    None => return Ok(RunOnceOutcome::UnknownCommand),
                };
                if step.matched == sm::MatchedEdgeKind::Var {
                    captures.push(token.clone());
                }
                state = step.next_state;
            }

            let command_id = match mode.accept_at(state)? {
                Some(command_id) => command_id,
                None => return Ok(RunOnceOutcome::IncompleteCommand),
            };

            (command_id, captures)
        };

        let inputs = match self.build_command_inputs(command_id, &captures) {
            Ok(inputs) => inputs,
            Err(err) => return Ok(RunOnceOutcome::HandlerError(err)),
        };

        let action = match self.invoke_handler(command_id, &inputs) {
            Ok(action) => action,
            Err(err) => return Ok(RunOnceOutcome::HandlerError(err)),
        };

        let applied = self.apply(action)?;
        Ok(RunOnceOutcome::ActionApplied(applied))
    }

    pub fn run(&mut self) -> io::Result<()> {
        if editor::prefer_rustyline_backend() {
            #[cfg(feature = "rustyline")]
            {
                let mut editor = editor::RustylineEditor::new()?;
                return self.run_with_editor(&mut editor);
            }
        }

        let mut editor = editor::BasicEditor::new();
        self.run_with_editor(&mut editor)
    }

    #[cfg(test)]
    fn modes_len(&self) -> usize {
        self.modes.len()
    }

    #[cfg(test)]
    fn handlers_len(&self) -> usize {
        self.handlers.len()
    }

    #[cfg(test)]
    fn capture_specs_len(&self) -> usize {
        self.capture_specs.len()
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor;

    fn noop_handler() -> Handler {
        Box::new(|_, _| Ok(Action::None))
    }

    fn build_cmd(literals: &[&str], positional_args: u8) -> cmd::Cmd {
        let mut builder = cmd::CmdBuilder::new();
        builder.literals(literals).positional_args(positional_args);
        builder.build()
    }

    fn build_labeled_cmd(literals: &[&str], labels: &[&str]) -> cmd::Cmd {
        let mut builder = cmd::CmdBuilder::new();
        builder.literals(literals);
        for label in labels {
            builder.labeled_arg(label);
        }
        builder.build()
    }

    fn completion_items(tokens: &[&str]) -> Vec<CompletionItem> {
        tokens
            .iter()
            .map(|token| CompletionItem {
                token: (*token).to_string(),
                doc: None,
            })
            .collect()
    }

    fn completion_request(exact_tokens: &[&str], partial: &str) -> CompletionRequest {
        CompletionRequest {
            exact_tokens: exact_tokens
                .iter()
                .map(|token| (*token).to_string())
                .collect(),
            partial: partial.to_string(),
        }
    }

    #[test]
    fn parse_line_splits_bare_tokens() {
        assert_eq!(
            parse_line("show ip route").unwrap(),
            ParsedLine {
                tokens: vec!["show".to_string(), "ip".to_string(), "route".to_string()],
                ends_with_whitespace: false,
                ended_after_quoted_token: false,
            }
        );
    }

    #[test]
    fn parse_line_keeps_quoted_text_as_single_token() {
        assert_eq!(
            parse_line("note \"foo bar\"").unwrap(),
            ParsedLine {
                tokens: vec!["note".to_string(), "foo bar".to_string()],
                ends_with_whitespace: false,
                ended_after_quoted_token: true,
            }
        );
    }

    #[test]
    fn parse_line_accepts_empty_quoted_token() {
        assert_eq!(
            parse_line("\"\"").unwrap(),
            ParsedLine {
                tokens: vec![String::new()],
                ends_with_whitespace: false,
                ended_after_quoted_token: true,
            }
        );
    }

    #[test]
    fn parse_line_rejects_unterminated_quote() {
        assert_eq!(parse_line("note \"foo").unwrap_err(), ParseLineError::UnterminatedQuote);
    }

    #[test]
    fn parse_line_rejects_quote_inside_bare_token() {
        assert_eq!(parse_line("foo\"bar").unwrap_err(), ParseLineError::UnexpectedQuote);
    }

    #[test]
    fn parse_line_rejects_trailing_characters_after_quote() {
        assert_eq!(
            parse_line("\"foo\"bar").unwrap_err(),
            ParseLineError::TrailingCharactersAfterQuote
        );
    }

    #[test]
    fn new_initializes_global_mode_and_stack() {
        let repl = Repl::new();

        assert_eq!(repl.current_mode_id().unwrap(), 0);
        assert_eq!(repl.current_mode().unwrap().id(), 0);
        assert_eq!(repl.current_mode().unwrap().name(), "global");
        assert_eq!(repl.modes_len(), 1);
        assert_eq!(repl.handlers_len(), 0);
        assert_eq!(repl.capture_specs_len(), 0);
    }

    #[test]
    fn add_mode_assigns_sequential_ids_matching_index() {
        let mut repl = Repl::new();

        let exec = repl.add_mode("exec");
        let config = repl.add_mode("config");

        assert_eq!(exec, 1);
        assert_eq!(config, 2);
        assert_eq!(repl.modes_len(), 3);
        assert_eq!(repl.get_mode(exec).unwrap().id(), exec);
        assert_eq!(repl.get_mode(exec).unwrap().name(), "exec");
        assert_eq!(repl.get_mode(config).unwrap().id(), config);
        assert_eq!(repl.get_mode(config).unwrap().name(), "config");
    }

    #[test]
    fn push_mode_switches_current_mode() {
        let mut repl = Repl::new();
        let config = repl.add_mode("config");

        repl.push_mode(config).unwrap();

        assert_eq!(repl.current_mode_id().unwrap(), config);
        assert_eq!(repl.current_mode().unwrap().name(), "config");
    }

    #[test]
    fn pop_mode_returns_to_previous_mode() {
        let mut repl = Repl::new();
        let config = repl.add_mode("config");
        repl.push_mode(config).unwrap();

        let popped = repl.pop_mode().unwrap();

        assert_eq!(popped, config);
        assert_eq!(repl.current_mode_id().unwrap(), 0);
        assert_eq!(repl.current_mode().unwrap().name(), "global");
    }

    #[test]
    fn pop_mode_rejects_root_pop() {
        let mut repl = Repl::new();

        assert_eq!(repl.pop_mode().unwrap_err(), ReplError::CannotPopRootMode);
    }

    #[test]
    fn push_mode_rejects_invalid_id() {
        let mut repl = Repl::new();

        assert_eq!(
            repl.push_mode(99).unwrap_err(),
            ReplError::InvalidModeId(99)
        );
    }

    #[test]
    fn get_mode_and_get_mode_mut_reject_invalid_id() {
        let mut repl = Repl::new();

        assert_eq!(repl.get_mode(99).unwrap_err(), ReplError::InvalidModeId(99));
        assert_eq!(
            repl.get_mode_mut(99).unwrap_err(),
            ReplError::InvalidModeId(99)
        );
    }

    #[test]
    fn current_mode_errors_if_stack_corrupted() {
        let mut repl = Repl::new();
        repl.stack.clear();

        assert_eq!(
            repl.current_mode_id().unwrap_err(),
            ReplError::EmptyModeStack
        );
        assert_eq!(repl.current_mode().unwrap_err(), ReplError::EmptyModeStack);
        assert_eq!(repl.pop_mode().unwrap_err(), ReplError::EmptyModeStack);
    }

    #[test]
    fn current_mode_mut_returns_mutable_mode_reference() {
        let mut repl = Repl::new();

        let mode = repl.current_mode_mut().unwrap();
        assert_eq!(mode.name(), "global");
    }

    #[test]
    fn prompt_formats_mode_stack_path() {
        let mut repl = Repl::new();
        let config = repl.add_mode("config");
        let iface = repl.add_mode("interface");
        repl.push_mode(config).unwrap();
        repl.push_mode(iface).unwrap();

        assert_eq!(repl.prompt().unwrap(), "global/config/interface> ");
    }

    #[test]
    fn register_handler_returns_sequential_command_ids() {
        let mut repl = Repl::new();

        let a = repl.register_handler(noop_handler(), Vec::new());
        let b = repl.register_handler(
            noop_handler(),
            vec![cmd::CaptureKind::Positional {
                name: None,
                doc: None,
            }],
        );
        let c = repl.register_handler(noop_handler(), Vec::new());

        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 2);
        assert_eq!(repl.handlers_len(), 3);
        assert_eq!(repl.capture_specs_len(), 3);
    }

    #[test]
    fn builder_integrated_literal_and_command_docs_are_applied_on_registration() {
        let mut repl = Repl::new();
        let mut builder = cmd::CmdBuilder::new();
        builder
            .literal_with_doc("show", "show data")
            .literal_with_doc("version", "show version")
            .command_doc("show software version");
        let cmd = builder.build();

        repl.register_mode_command(0, &cmd, noop_handler()).unwrap();

        assert_eq!(
            repl.run_once("?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "show".to_string(),
                doc: Some("show data".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("show ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "version".to_string(),
                doc: Some("show version".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("show version ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: RET_COMPLETION_TOKEN.to_string(),
                doc: Some("show software version".to_string())
            }])
        );
    }

    #[test]
    fn builder_integrated_docs_apply_to_literals_after_vars() {
        let mut repl = Repl::new();
        let mut builder = cmd::CmdBuilder::new();
        builder
            .literal_with_doc("create", "create data")
            .literal_with_doc("account", "create account")
            .labeled_arg_with_doc("name", "account name")
            .labeled_arg_with_doc("currency", "account currency")
            .labeled_arg_with_doc("note", "account note");
        let cmd = builder.build();

        repl.register_mode_command(0, &cmd, noop_handler()).unwrap();

        assert_eq!(
            repl.run_once("create ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "account".to_string(),
                doc: Some("create account".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("create account ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "name".to_string(),
                doc: Some("account name".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("create account name ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "<name>".to_string(),
                doc: Some("account name".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("create account name cash ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "currency".to_string(),
                doc: Some("account currency".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("create account name cash currency ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "<currency>".to_string(),
                doc: Some("account currency".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("create account name cash currency USD ?")
                .unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "note".to_string(),
                doc: Some("account note".to_string())
            }])
        );
        assert_eq!(
            repl.run_once("create account name cash currency USD note ?")
                .unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "<note>".to_string(),
                doc: Some("account note".to_string())
            }])
        );
    }

    #[test]
    fn register_command_in_mode_registers_syntax_with_given_command_id() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);
        let command_id = repl.register_handler(noop_handler(), cmd.capture_spec().unwrap());

        repl.register_command_in_mode(0, &cmd, command_id).unwrap();

        let mode = repl.get_mode(0).unwrap();
        let show = mode.next_state(mode.root_state(), "show").unwrap();
        let version = mode.next_state(show, "version").unwrap();
        assert!(mode.get_completions(version, "").is_empty());
    }

    #[test]
    fn register_mode_command_returns_command_id_and_inserts_command() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "ip"], 1);

        let command_id = repl.register_mode_command(0, &cmd, noop_handler()).unwrap();

        assert_eq!(command_id, 0);
        assert_eq!(repl.handlers_len(), 1);
        assert_eq!(repl.capture_specs_len(), 1);

        let mode = repl.get_mode(0).unwrap();
        let show = mode.next_state(mode.root_state(), "show").unwrap();
        let ip = mode.next_state(show, "ip").unwrap();
        assert!(mode.next_state(ip, "eth0").is_some());
    }

    #[test]
    fn register_mode_command_rolls_back_handler_on_duplicate_command_path() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);

        let first_id = repl.register_mode_command(0, &cmd, noop_handler()).unwrap();
        assert_eq!(first_id, 0);
        assert_eq!(repl.handlers_len(), 1);
        assert_eq!(repl.capture_specs_len(), 1);

        let err = repl
            .register_mode_command(0, &cmd, noop_handler())
            .unwrap_err();
        assert_eq!(
            err,
            ReplError::CmdInsert(CommandRegistrationError::DuplicateCommandPath {
                existing: 0,
                attempted: 1,
            })
        );
        assert_eq!(repl.handlers_len(), 1);
        assert_eq!(repl.capture_specs_len(), 1);
    }

    #[test]
    fn register_command_in_mode_invalid_mode_id_returns_repl_error() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show"], 0);

        let err = repl.register_command_in_mode(99, &cmd, 0).unwrap_err();
        assert_eq!(err, ReplError::InvalidModeId(99));
    }

    #[test]
    fn repl_error_from_cmd_insert_is_wrapped() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);

        repl.register_command_in_mode(0, &cmd, 0).unwrap();
        let err = repl.register_command_in_mode(0, &cmd, 1).unwrap_err();

        assert_eq!(
            err,
            ReplError::CmdInsert(CommandRegistrationError::DuplicateCommandPath {
                existing: 0,
                attempted: 1,
            })
        );
    }

    #[test]
    fn register_mode_command_rejects_duplicate_labeled_args() {
        let mut repl = Repl::new();
        let cmd = build_labeled_cmd(&["create", "account"], &["name", "name"]);

        let err = repl
            .register_mode_command(0, &cmd, noop_handler())
            .unwrap_err();

        assert_eq!(
            err,
            ReplError::CmdInsert(CommandRegistrationError::DuplicateLabeledArg {
                label: "name".to_string()
            })
        );
        assert_eq!(repl.handlers_len(), 0);
        assert_eq!(repl.capture_specs_len(), 0);
    }

    #[test]
    fn run_once_returns_noop_for_empty_or_whitespace_input() {
        let mut repl = Repl::new();

        assert_eq!(repl.run_once("").unwrap(), RunOnceOutcome::Noop);
        assert_eq!(repl.run_once("   ").unwrap(), RunOnceOutcome::Noop);
    }

    #[test]
    fn run_once_question_returns_root_completions() {
        let mut repl = Repl::new();
        let show = build_cmd(&["show"], 0);
        let write = build_cmd(&["write"], 0);
        repl.register_mode_command(0, &write, noop_handler())
            .unwrap();
        repl.register_mode_command(0, &show, noop_handler())
            .unwrap();

        assert_eq!(
            repl.run_once("?").unwrap(),
            RunOnceOutcome::Completions(completion_items(&["show", "write"]))
        );
    }

    #[test]
    fn run_once_question_after_whitespace_completes_next_token() {
        let mut repl = Repl::new();
        let show_ip = build_cmd(&["show", "ip"], 0);
        let show_version = build_cmd(&["show", "version"], 0);
        repl.register_mode_command(0, &show_version, noop_handler())
            .unwrap();
        repl.register_mode_command(0, &show_ip, noop_handler())
            .unwrap();

        assert_eq!(
            repl.run_once("show ?").unwrap(),
            RunOnceOutcome::Completions(completion_items(&["ip", "version"]))
        );
    }

    #[test]
    fn run_once_inline_question_completes_partial_token() {
        let mut repl = Repl::new();
        let delete_db = build_cmd(&["delete-db"], 0);
        let describe = build_cmd(&["describe"], 0);
        repl.register_mode_command(0, &delete_db, noop_handler())
            .unwrap();
        repl.register_mode_command(0, &describe, noop_handler())
            .unwrap();

        assert_eq!(
            repl.run_once("de?").unwrap(),
            RunOnceOutcome::Completions(completion_items(&["delete-db", "describe"]))
        );
    }

    #[test]
    fn run_once_completion_uses_abbrev_semantics_for_prior_tokens() {
        let mut repl = Repl::new();
        let route = build_cmd(&["show", "ip", "route"], 0);
        let iface = build_cmd(&["show", "ip", "interface"], 0);
        repl.register_mode_command(0, &route, noop_handler())
            .unwrap();
        repl.register_mode_command(0, &iface, noop_handler())
            .unwrap();

        assert_eq!(
            repl.run_once("sh ip ?").unwrap(),
            RunOnceOutcome::Completions(completion_items(&["interface", "route"]))
        );
    }

    #[test]
    fn run_once_completion_invalid_prefix_path_returns_empty_completions() {
        let mut repl = Repl::new();
        let show = build_cmd(&["show"], 0);
        repl.register_mode_command(0, &show, noop_handler())
            .unwrap();

        assert_eq!(
            repl.run_once("bogus ?").unwrap(),
            RunOnceOutcome::Completions(Vec::new())
        );
    }

    #[test]
    fn completion_request_from_prefix_parses_empty_partial_after_whitespace() {
        assert_eq!(
            Repl::completion_request_from_prefix("show ip "),
            Some(completion_request(&["show", "ip"], ""))
        );
    }

    #[test]
    fn completion_request_from_prefix_parses_trailing_partial_token() {
        assert_eq!(
            Repl::completion_request_from_prefix("show ver"),
            Some(completion_request(&["show"], "ver"))
        );
    }

    #[test]
    fn completion_request_from_prefix_treats_closed_quoted_token_as_complete() {
        assert_eq!(
            Repl::completion_request_from_prefix("note \"foo bar\""),
            Some(completion_request(&["note", "foo bar"], ""))
        );
    }

    #[test]
    fn completion_request_from_prefix_disables_completion_inside_open_quote() {
        assert_eq!(Repl::completion_request_from_prefix("note \"foo"), None);
    }

    #[test]
    fn completion_request_from_prefix_disables_completion_for_malformed_quote() {
        assert_eq!(Repl::completion_request_from_prefix("\"foo\"bar"), None);
    }

    #[test]
    fn run_once_completion_inside_open_quote_returns_empty_completions() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["note"], 1), noop_handler())
            .unwrap();

        assert_eq!(
            repl.run_once("note \"foo ?").unwrap(),
            RunOnceOutcome::Completions(Vec::new())
        );
    }

    #[test]
    fn complete_prefix_matches_root_and_nested_help_queries() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["foo"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["foo", "bar"], 0), noop_handler())
            .unwrap();
        repl.set_command_doc(0, "foo", "foo doc").unwrap();
        repl.set_edge_doc(0, "foo bar", "bar doc").unwrap();

        assert_eq!(
            repl.complete_prefix("").unwrap(),
            completion_items(&["foo"])
        );
        assert_eq!(
            repl.complete_prefix("foo ").unwrap(),
            vec![
                CompletionItem {
                    token: RET_COMPLETION_TOKEN.to_string(),
                    doc: Some("foo doc".to_string())
                },
                CompletionItem {
                    token: "bar".to_string(),
                    doc: Some("bar doc".to_string())
                }
            ]
        );
    }

    #[test]
    fn completion_snapshot_tab_completion_returns_single_match_replacement() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["show"], 0), noop_handler())
            .unwrap();

        assert_eq!(
            repl.completion_snapshot().tab_completion("sh").unwrap(),
            Some(TabCompletion {
                insert_suffix: "ow".to_string()
            })
        );
    }

    #[test]
    fn completion_snapshot_tab_completion_returns_longest_common_prefix() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["delete-db"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["describe"], 0), noop_handler())
            .unwrap();

        assert_eq!(
            repl.completion_snapshot().tab_completion("d").unwrap(),
            Some(TabCompletion {
                insert_suffix: "e".to_string()
            })
        );
        assert_eq!(
            repl.completion_snapshot().tab_completion("de").unwrap(),
            None
        );
    }

    #[test]
    fn longest_common_prefix_handles_disjoint_and_shared_candidates() {
        assert_eq!(longest_common_prefix(&[]), "");
        assert_eq!(
            longest_common_prefix(&["bar".to_string(), "baz".to_string()]),
            "ba"
        );
        assert_eq!(
            longest_common_prefix(&["alpha".to_string(), "beta".to_string()]),
            ""
        );
    }

    #[test]
    fn format_completions_renders_doc_table_and_empty_state() {
        assert_eq!(
            format_completions(&[]),
            "\nPossible completions:\n  (none)\n\n"
        );
        assert_eq!(
            format_completions(&[
                CompletionItem {
                    token: "RET".to_string(),
                    doc: Some("run foo".to_string())
                },
                CompletionItem {
                    token: "bar".to_string(),
                    doc: None
                }
            ]),
            "\nPossible completions:\n  RET  run foo\n  bar\n\n"
        );
    }

    struct MockEditor {
        reads: Vec<editor::EditorRead>,
        prompts: Vec<String>,
        printed: Vec<Vec<CompletionItem>>,
        history: Vec<String>,
    }

    impl MockEditor {
        fn new(reads: Vec<editor::EditorRead>) -> Self {
            Self {
                reads,
                prompts: Vec::new(),
                printed: Vec::new(),
                history: Vec::new(),
            }
        }
    }

    impl editor::LineEditor for MockEditor {
        fn read_line(&mut self, prompt: &str) -> io::Result<editor::EditorRead> {
            self.prompts.push(prompt.to_string());
            Ok(self.reads.remove(0))
        }

        fn print_completions(&mut self, items: &[CompletionItem]) -> io::Result<()> {
            self.printed.push(items.to_vec());
            Ok(())
        }

        fn add_history_entry(&mut self, line: &str) -> io::Result<()> {
            self.history.push(line.to_string());
            Ok(())
        }
    }

    #[test]
    fn run_with_editor_routes_help_output_and_history() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["show"], 0), noop_handler())
            .unwrap();

        let mut editor = MockEditor::new(vec![
            editor::EditorRead::Line("?\n".to_string()),
            editor::EditorRead::Line("show\n".to_string()),
            editor::EditorRead::Eof,
        ]);

        repl.run_with_editor(&mut editor).unwrap();

        assert_eq!(editor.prompts, vec!["global> ", "global> ", "global> "]);
        assert_eq!(editor.printed, vec![completion_items(&["show"])]);
        assert_eq!(editor.history, vec!["show\n".to_string()]);
    }

    #[test]
    fn run_once_completion_on_terminal_state_returns_empty_completions() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);
        repl.register_mode_command(0, &cmd, noop_handler()).unwrap();

        assert_eq!(
            repl.run_once("show version ?").unwrap(),
            RunOnceOutcome::Completions(Vec::new())
        );
    }

    #[test]
    fn run_once_completion_on_terminal_state_returns_command_doc_when_present() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);
        repl.register_mode_command(0, &cmd, noop_handler()).unwrap();
        repl.set_command_doc(0, "show version", "show software version")
            .unwrap();

        assert_eq!(
            repl.run_once("show version ?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: RET_COMPLETION_TOKEN.to_string(),
                doc: Some("show software version".to_string()),
            }])
        );
    }

    #[test]
    fn run_once_completion_on_accepting_state_with_literal_children_includes_ret_and_literals() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["foo"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["foo", "bar"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["foo", "baz"], 0), noop_handler())
            .unwrap();
        repl.set_edge_doc(0, "foo bar", "bar doc").unwrap();
        repl.set_edge_doc(0, "foo baz", "baz doc").unwrap();
        repl.set_command_doc(0, "foo", "foo doc").unwrap();

        assert_eq!(
            repl.run_once("foo ?").unwrap(),
            RunOnceOutcome::Completions(vec![
                CompletionItem {
                    token: RET_COMPLETION_TOKEN.to_string(),
                    doc: Some("foo doc".to_string())
                },
                CompletionItem {
                    token: "bar".to_string(),
                    doc: Some("bar doc".to_string())
                },
                CompletionItem {
                    token: "baz".to_string(),
                    doc: Some("baz doc".to_string())
                }
            ])
        );
    }

    #[test]
    fn run_once_inline_partial_completion_includes_ret_when_state_accepts() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["foo"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["foo", "bar"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["foo", "baz"], 0), noop_handler())
            .unwrap();
        repl.set_edge_doc(0, "foo bar", "bar doc").unwrap();
        repl.set_edge_doc(0, "foo baz", "baz doc").unwrap();
        repl.set_command_doc(0, "foo", "foo doc").unwrap();

        assert_eq!(
            repl.run_once("foo b?").unwrap(),
            RunOnceOutcome::Completions(vec![
                CompletionItem {
                    token: RET_COMPLETION_TOKEN.to_string(),
                    doc: Some("foo doc".to_string())
                },
                CompletionItem {
                    token: "bar".to_string(),
                    doc: Some("bar doc".to_string())
                },
                CompletionItem {
                    token: "baz".to_string(),
                    doc: Some("baz doc".to_string())
                }
            ])
        );
    }

    #[test]
    fn set_edge_doc_annotates_completion_items() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["write"], 0), noop_handler())
            .unwrap();
        repl.register_mode_command(0, &build_cmd(&["show"], 0), noop_handler())
            .unwrap();
        repl.set_edge_doc(0, "write", "enter write mode").unwrap();

        assert_eq!(
            repl.run_once("?").unwrap(),
            RunOnceOutcome::Completions(vec![
                CompletionItem {
                    token: "show".to_string(),
                    doc: None
                },
                CompletionItem {
                    token: "write".to_string(),
                    doc: Some("enter write mode".to_string())
                }
            ])
        );
    }

    #[test]
    fn set_edge_doc_overwrites_existing_doc() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["write"], 0), noop_handler())
            .unwrap();
        repl.set_edge_doc(0, "write", "old").unwrap();
        repl.set_edge_doc(0, "write", "new").unwrap();

        assert_eq!(
            repl.run_once("?").unwrap(),
            RunOnceOutcome::Completions(vec![CompletionItem {
                token: "write".to_string(),
                doc: Some("new".to_string())
            }])
        );
    }

    #[test]
    fn set_edge_doc_rejects_invalid_mode() {
        let mut repl = Repl::new();
        assert_eq!(
            repl.set_edge_doc(99, "write", "doc").unwrap_err(),
            ReplError::InvalidModeId(99)
        );
    }

    #[test]
    fn set_edge_doc_rejects_invalid_or_missing_stem() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["write"], 0), noop_handler())
            .unwrap();

        assert_eq!(
            repl.set_edge_doc(0, "   ", "doc").unwrap_err(),
            ReplError::InvalidDocStem
        );
        assert_eq!(
            repl.set_edge_doc(0, "missing", "doc").unwrap_err(),
            ReplError::DocPathNotFound("missing".to_string())
        );
    }

    #[test]
    fn set_command_doc_stores_for_terminal_path() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["show", "version"], 0), noop_handler())
            .unwrap();

        repl.set_command_doc(0, "show version", "show version help")
            .unwrap();

        let mode = repl.get_mode(0).unwrap();
        let show = mode.step(mode.root_state(), "show").unwrap();
        let version = mode.step(show.next_state, "version").unwrap();
        assert_eq!(
            mode.command_doc_at(version.next_state).unwrap(),
            Some("show version help")
        );
    }

    #[test]
    fn set_command_doc_rejects_non_terminal_path() {
        let mut repl = Repl::new();
        repl.register_mode_command(0, &build_cmd(&["show", "version"], 0), noop_handler())
            .unwrap();

        assert_eq!(
            repl.set_command_doc(0, "show", "doc").unwrap_err(),
            ReplError::CommandDocTargetNotTerminal("show".to_string())
        );
    }

    #[test]
    fn run_once_exit_in_root_returns_exit_action() {
        let mut repl = Repl::new();

        assert_eq!(
            repl.run_once("exit").unwrap(),
            RunOnceOutcome::ActionApplied(Action::Exit)
        );
        assert_eq!(repl.current_mode_id().unwrap(), 0);
    }

    #[test]
    fn run_once_exit_in_submode_pops_mode() {
        let mut repl = Repl::new();
        let cfg = repl.add_mode("config");
        repl.push_mode(cfg).unwrap();

        assert_eq!(
            repl.run_once("exit").unwrap(),
            RunOnceOutcome::ActionApplied(Action::PopMode)
        );
        assert_eq!(repl.current_mode_id().unwrap(), 0);
    }

    #[test]
    fn run_once_returns_unknown_for_unmatched_command() {
        let mut repl = Repl::new();

        assert_eq!(
            repl.run_once("nope").unwrap(),
            RunOnceOutcome::UnknownCommand
        );
    }

    #[test]
    fn run_once_returns_incomplete_for_non_terminal_match() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);
        repl.register_mode_command(0, &cmd, noop_handler()).unwrap();

        assert_eq!(
            repl.run_once("show").unwrap(),
            RunOnceOutcome::IncompleteCommand
        );
    }

    #[test]
    fn run_once_invokes_handler_with_captured_var_inputs() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut repl = Repl::new();
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = Rc::clone(&seen);
        let cmd = build_cmd(&["show"], 2);

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(move |_, inputs| {
                *seen_clone.borrow_mut() = inputs.positionals.clone();
                Ok(Action::None)
            }),
        )
        .unwrap();

        assert_eq!(
            repl.run_once("show a b").unwrap(),
            RunOnceOutcome::ActionApplied(Action::None)
        );
        assert_eq!(&*seen.borrow(), &vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn run_once_invokes_handler_with_quoted_var_input() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut repl = Repl::new();
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = Rc::clone(&seen);
        let cmd = build_cmd(&["note"], 1);

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(move |_, inputs| {
                *seen_clone.borrow_mut() = inputs.positionals.clone();
                Ok(Action::None)
            }),
        )
        .unwrap();

        assert_eq!(
            repl.run_once("note \"foo bar\"").unwrap(),
            RunOnceOutcome::ActionApplied(Action::None)
        );
        assert_eq!(&*seen.borrow(), &vec!["foo bar".to_string()]);
    }

    #[test]
    fn run_once_does_not_capture_literal_tokens() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut repl = Repl::new();
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = Rc::clone(&seen);
        let cmd = build_cmd(&["show", "ip"], 1);

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(move |_, inputs| {
                *seen_clone.borrow_mut() = inputs.positionals.clone();
                Ok(Action::None)
            }),
        )
        .unwrap();

        let _ = repl.run_once("show ip eth0").unwrap();
        assert_eq!(&*seen.borrow(), &vec!["eth0".to_string()]);
    }

    #[test]
    fn run_once_invokes_handler_with_labeled_inputs() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut repl = Repl::new();
        let seen: Rc<RefCell<BTreeMap<String, String>>> = Rc::new(RefCell::new(BTreeMap::new()));
        let seen_clone = Rc::clone(&seen);
        let cmd = build_labeled_cmd(&["create", "account"], &["name", "currency"]);

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(move |_, inputs| {
                *seen_clone.borrow_mut() = inputs.labeled.clone();
                Ok(Action::None)
            }),
        )
        .unwrap();

        assert_eq!(
            repl.run_once("create account name cash currency USD")
                .unwrap(),
            RunOnceOutcome::ActionApplied(Action::None)
        );
        assert_eq!(
            &*seen.borrow(),
            &BTreeMap::from([
                ("currency".to_string(), "USD".to_string()),
                ("name".to_string(), "cash".to_string())
            ])
        );
    }

    #[test]
    fn run_once_invokes_handler_with_quoted_labeled_inputs() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut repl = Repl::new();
        let seen: Rc<RefCell<BTreeMap<String, String>>> = Rc::new(RefCell::new(BTreeMap::new()));
        let seen_clone = Rc::clone(&seen);
        let cmd = build_labeled_cmd(&["create", "account"], &["name", "currency"]);

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(move |_, inputs| {
                *seen_clone.borrow_mut() = inputs.labeled.clone();
                Ok(Action::None)
            }),
        )
        .unwrap();

        assert_eq!(
            repl.run_once("create account name \"cash account\" currency USD")
                .unwrap(),
            RunOnceOutcome::ActionApplied(Action::None)
        );
        assert_eq!(
            seen.borrow().get("name"),
            Some(&"cash account".to_string())
        );
    }

    #[test]
    fn run_once_invokes_handler_with_mixed_positional_and_labeled_inputs() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut repl = Repl::new();
        let seen: Rc<RefCell<Option<CommandInputs>>> = Rc::new(RefCell::new(None));
        let seen_clone = Rc::clone(&seen);
        let mut builder = cmd::CmdBuilder::new();
        builder.literals(&["set"]).positional_args(1).labeled_arg("value");
        let cmd = builder.build();

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(move |_, inputs| {
                *seen_clone.borrow_mut() = Some(inputs.clone());
                Ok(Action::None)
            }),
        )
        .unwrap();

        assert_eq!(
            repl.run_once("set hostname value \"router one\"").unwrap(),
            RunOnceOutcome::ActionApplied(Action::None)
        );
        assert_eq!(
            seen.borrow().as_ref(),
            Some(&CommandInputs {
                positionals: vec!["hostname".to_string()],
                labeled: BTreeMap::from([(
                    "value".to_string(),
                    "router one".to_string()
                )]),
            })
        );
    }

    #[test]
    fn run_once_applies_push_mode_action() {
        let mut repl = Repl::new();
        let cfg = repl.add_mode("config");
        let cmd = build_cmd(&["configure"], 0);

        repl.register_mode_command(0, &cmd, Box::new(move |_, _| Ok(Action::PushMode(cfg))))
            .unwrap();

        assert_eq!(
            repl.run_once("configure").unwrap(),
            RunOnceOutcome::ActionApplied(Action::PushMode(cfg))
        );
        assert_eq!(repl.current_mode_id().unwrap(), cfg);
    }

    #[test]
    fn run_once_applies_pop_mode_action() {
        let mut repl = Repl::new();
        let cfg = repl.add_mode("config");
        repl.push_mode(cfg).unwrap();
        let cmd = build_cmd(&["end"], 0);

        repl.register_mode_command(cfg, &cmd, Box::new(|_, _| Ok(Action::PopMode)))
            .unwrap();

        assert_eq!(
            repl.run_once("end").unwrap(),
            RunOnceOutcome::ActionApplied(Action::PopMode)
        );
        assert_eq!(repl.current_mode_id().unwrap(), 0);
    }

    #[test]
    fn run_once_returns_handler_error_outcome() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["boom"], 0);

        repl.register_mode_command(
            0,
            &cmd,
            Box::new(|_, _| Err(HandlerError("boom".to_string()))),
        )
        .unwrap();

        assert_eq!(
            repl.run_once("boom").unwrap(),
            RunOnceOutcome::HandlerError(HandlerError("boom".to_string()))
        );
        assert_eq!(repl.current_mode_id().unwrap(), 0);
    }

    #[test]
    fn run_once_returns_parse_error_for_unterminated_quote() {
        let mut repl = Repl::new();

        assert_eq!(
            repl.run_once("note \"foo").unwrap(),
            RunOnceOutcome::ParseError(ParseLineError::UnterminatedQuote)
        );
        assert_eq!(repl.current_mode_id().unwrap(), 0);
    }
}
