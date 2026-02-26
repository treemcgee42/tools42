use crate::{cmd, mode, sm};

pub(crate) type ModeId = mode::ModeId;
pub(crate) type CommandId = sm::CommandId;
pub(crate) type ParsedInputs = Vec<String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    PushMode(ModeId),
    PopMode,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandlerError(pub(crate) String);

pub(crate) type HandlerResult = Result<Action, HandlerError>;
pub(crate) type Handler = Box<dyn FnMut(&mut Repl, &[String]) -> HandlerResult>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReplError {
    InvalidModeId(ModeId),
    EmptyModeStack,
    CannotPopRootMode,
    CmdInsert(sm::CmdInsertError),
}

impl From<sm::CmdInsertError> for ReplError {
    fn from(value: sm::CmdInsertError) -> Self {
        Self::CmdInsert(value)
    }
}

pub(crate) struct Repl {
    modes: Vec<mode::Mode>,
    stack: Vec<ModeId>,
    handlers: Vec<Handler>,
}

impl Repl {
    pub(crate) fn new() -> Self {
        Self {
            modes: vec![mode::Mode::new(0, "global")],
            stack: vec![0],
            handlers: Vec::new(),
        }
    }

    pub(crate) fn current_mode_id(&self) -> Result<ModeId, ReplError> {
        self.stack.last().copied().ok_or(ReplError::EmptyModeStack)
    }

    pub(crate) fn current_mode(&self) -> Result<&mode::Mode, ReplError> {
        let id = self.current_mode_id()?;
        self.get_mode(id)
    }

    pub(crate) fn current_mode_mut(&mut self) -> Result<&mut mode::Mode, ReplError> {
        let id = self.current_mode_id()?;
        self.get_mode_mut(id)
    }

    pub(crate) fn add_mode(&mut self, name: impl Into<String>) -> ModeId {
        let id = self.modes.len() as ModeId;
        self.modes.push(mode::Mode::new(id, name));
        id
    }

    pub(crate) fn get_mode(&self, id: ModeId) -> Result<&mode::Mode, ReplError> {
        self.modes
            .get(id as usize)
            .ok_or(ReplError::InvalidModeId(id))
    }

    pub(crate) fn get_mode_mut(&mut self, id: ModeId) -> Result<&mut mode::Mode, ReplError> {
        self.modes
            .get_mut(id as usize)
            .ok_or(ReplError::InvalidModeId(id))
    }

    pub(crate) fn push_mode(&mut self, id: ModeId) -> Result<(), ReplError> {
        if (id as usize) >= self.modes.len() {
            return Err(ReplError::InvalidModeId(id));
        }
        self.stack.push(id);
        Ok(())
    }

    pub(crate) fn pop_mode(&mut self) -> Result<ModeId, ReplError> {
        if self.stack.is_empty() {
            return Err(ReplError::EmptyModeStack);
        }
        if self.stack.len() == 1 {
            return Err(ReplError::CannotPopRootMode);
        }
        Ok(self.stack.pop().expect("stack length checked above"))
    }

    pub(crate) fn register_handler(&mut self, handler: Handler) -> CommandId {
        let id = self.handlers.len() as CommandId;
        self.handlers.push(handler);
        id
    }

    pub(crate) fn register_command_in_mode(
        &mut self,
        mode_id: ModeId,
        cmd: &cmd::Cmd,
        command_id: CommandId,
    ) -> Result<(), ReplError> {
        let mode = self.get_mode_mut(mode_id)?;
        mode.insert_cmd(cmd, command_id)?;
        Ok(())
    }

    pub(crate) fn register_mode_command(
        &mut self,
        mode_id: ModeId,
        cmd: &cmd::Cmd,
        handler: Handler,
    ) -> Result<CommandId, ReplError> {
        let command_id = self.register_handler(handler);
        if let Err(err) = self.register_command_in_mode(mode_id, cmd, command_id) {
            let _ = self.handlers.pop();
            return Err(err);
        }
        Ok(command_id)
    }

    #[cfg(test)]
    fn modes_len(&self) -> usize {
        self.modes.len()
    }

    #[cfg(test)]
    fn handlers_len(&self) -> usize {
        self.handlers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_handler() -> Handler {
        Box::new(|_, _| Ok(Action::None))
    }

    fn build_cmd(literals: &[&str], positional_args: u8) -> cmd::Cmd {
        let mut builder = cmd::CmdBuilder::new();
        builder.literals(literals).positional_args(positional_args);
        builder.build()
    }

    #[test]
    fn new_initializes_global_mode_and_stack() {
        let repl = Repl::new();

        assert_eq!(repl.current_mode_id().unwrap(), 0);
        assert_eq!(repl.current_mode().unwrap().id(), 0);
        assert_eq!(repl.current_mode().unwrap().name(), "global");
        assert_eq!(repl.modes_len(), 1);
        assert_eq!(repl.handlers_len(), 0);
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

        assert_eq!(repl.push_mode(99).unwrap_err(), ReplError::InvalidModeId(99));
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

        assert_eq!(repl.current_mode_id().unwrap_err(), ReplError::EmptyModeStack);
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
    fn register_handler_returns_sequential_command_ids() {
        let mut repl = Repl::new();

        let a = repl.register_handler(noop_handler());
        let b = repl.register_handler(noop_handler());
        let c = repl.register_handler(noop_handler());

        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 2);
        assert_eq!(repl.handlers_len(), 3);
    }

    #[test]
    fn register_command_in_mode_registers_syntax_with_given_command_id() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show", "version"], 0);
        let command_id = repl.register_handler(noop_handler());

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

        let err = repl
            .register_mode_command(0, &cmd, noop_handler())
            .unwrap_err();
        assert_eq!(
            err,
            ReplError::CmdInsert(sm::CmdInsertError::DuplicateCommandPath {
                existing: 0,
                attempted: 1,
            })
        );
        assert_eq!(repl.handlers_len(), 1);
    }

    #[test]
    fn register_command_in_mode_invalid_mode_id_returns_repl_error() {
        let mut repl = Repl::new();
        let cmd = build_cmd(&["show"], 0);

        let err = repl
            .register_command_in_mode(99, &cmd, 0)
            .unwrap_err();
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
            ReplError::CmdInsert(sm::CmdInsertError::DuplicateCommandPath {
                existing: 0,
                attempted: 1,
            })
        );
    }
}
