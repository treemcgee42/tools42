use crate::mode;

pub(crate) type ModeId = mode::ModeId;
pub(crate) type Handler = ();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReplError {
    InvalidModeId(ModeId),
    EmptyModeStack,
    CannotPopRootMode,
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
}
