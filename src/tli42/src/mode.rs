use crate::{cmd, sm};

pub(crate) type ModeId = u32;

#[derive(Debug)]
pub(crate) struct Mode {
    id: ModeId,
    name: String,
    sm: sm::Sm,
}

impl Mode {
    pub(crate) fn new(id: ModeId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            sm: sm::Sm::new(),
        }
    }

    pub(crate) fn id(&self) -> ModeId {
        self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn root_state(&self) -> sm::StateId {
        0
    }

    pub(crate) fn insert_cmd(
        &mut self,
        cmd: &cmd::Cmd,
        command_id: sm::CommandId,
    ) -> Result<(), sm::CmdInsertError> {
        self.sm.insert_cmd(cmd, command_id)
    }

    pub(crate) fn next_state(
        &self,
        current_state: sm::StateId,
        input_token: &str,
    ) -> Option<sm::StateId> {
        self.sm.next_state(current_state, input_token)
    }

    pub(crate) fn step(
        &self,
        current_state: sm::StateId,
        input_token: &str,
    ) -> Option<sm::StepResult> {
        self.sm.step(current_state, input_token)
    }

    pub(crate) fn get_completions<'a>(
        &'a self,
        current_state: sm::StateId,
        partial_token: &str,
    ) -> Vec<&'a str> {
        self.sm.get_completions(current_state, partial_token)
    }

    pub(crate) fn get_completions_with_docs<'a>(
        &'a self,
        current_state: sm::StateId,
        partial_token: &str,
    ) -> Vec<(&'a str, Option<&'a str>)> {
        self.sm.get_completions_with_docs(current_state, partial_token)
    }

    pub(crate) fn accept_at(
        &self,
        state_id: sm::StateId,
    ) -> Result<Option<sm::CommandId>, sm::CmdInsertError> {
        self.sm.accept_at(state_id)
    }

    pub(crate) fn set_literal_edge_doc(
        &mut self,
        current_state: sm::StateId,
        literal: &str,
        doc: String,
    ) -> Result<bool, sm::CmdInsertError> {
        self.sm.set_literal_edge_doc(current_state, literal, doc)
    }

    pub(crate) fn set_command_doc(
        &mut self,
        state_id: sm::StateId,
        doc: String,
    ) -> Result<bool, sm::CmdInsertError> {
        self.sm.set_command_doc(state_id, doc)
    }

    pub(crate) fn command_doc_at(
        &self,
        state_id: sm::StateId,
    ) -> Result<Option<&str>, sm::CmdInsertError> {
        self.sm.command_doc_at(state_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_cmd(literals: &[&str], positional_args: u8) -> cmd::Cmd {
        let mut builder = cmd::CmdBuilder::new();
        builder.literals(literals).positional_args(positional_args);
        builder.build()
    }

    #[test]
    fn mode_new_sets_id_name_and_root_state() {
        let mode = Mode::new(7, "exec");

        assert_eq!(mode.id(), 7);
        assert_eq!(mode.name(), "exec");
        assert_eq!(mode.root_state(), 0);
    }

    #[test]
    fn mode_insert_cmd_delegates_to_sm_and_parses_tokens() {
        let mut mode = Mode::new(1, "exec");
        let cmd = build_cmd(&["show", "ip"], 1);

        mode.insert_cmd(&cmd, 10).unwrap();

        let s1 = mode.next_state(mode.root_state(), "show").unwrap();
        let s2 = mode.next_state(s1, "ip").unwrap();
        let s3 = mode.next_state(s2, "eth0").unwrap();
        assert!(mode.next_state(s3, "extra").is_none());

        let completions = mode.get_completions(mode.root_state(), "sh");
        assert_eq!(completions, vec!["show"]);
    }

    #[test]
    fn mode_step_delegates_to_sm() {
        let mut mode = Mode::new(1, "exec");
        let cmd = build_cmd(&["show"], 1);
        mode.insert_cmd(&cmd, 10).unwrap();

        let show = mode.step(mode.root_state(), "sh").unwrap();
        assert_eq!(show.matched, sm::MatchedEdgeKind::Literal);
        let var = mode.step(show.next_state, "eth0").unwrap();
        assert_eq!(var.matched, sm::MatchedEdgeKind::Var);
        assert_eq!(mode.accept_at(var.next_state).unwrap(), Some(10));
    }

    #[test]
    fn mode_get_completions_is_mode_scoped() {
        let mut exec = Mode::new(1, "exec");
        let mut config = Mode::new(2, "config");

        let exec_cmd = build_cmd(&["show", "version"], 0);
        let cfg_cmd = build_cmd(&["set", "hostname"], 1);

        exec.insert_cmd(&exec_cmd, 1).unwrap();
        config.insert_cmd(&cfg_cmd, 2).unwrap();

        assert_eq!(exec.get_completions(exec.root_state(), "s"), vec!["show"]);
        assert_eq!(config.get_completions(config.root_state(), "s"), vec!["set"]);
        assert!(exec.next_state(exec.root_state(), "set").is_none());
        assert!(config.next_state(config.root_state(), "show").is_none());
    }

    #[test]
    fn mode_insert_cmd_returns_duplicate_error() {
        let mut mode = Mode::new(1, "exec");
        let a = build_cmd(&["show", "version"], 0);
        let b = build_cmd(&["show", "version"], 0);

        mode.insert_cmd(&a, 1).unwrap();
        let err = mode.insert_cmd(&b, 2).unwrap_err();

        assert_eq!(
            err,
            sm::CmdInsertError::DuplicateCommandPath {
                existing: 1,
                attempted: 2
            }
        );
    }

    #[test]
    fn modes_are_isolated_even_with_same_command_paths() {
        let mut exec = Mode::new(1, "exec");
        let mut config = Mode::new(2, "config");
        let exec_cmd = build_cmd(&["show", "version"], 0);
        let cfg_cmd = build_cmd(&["show", "version"], 0);

        exec.insert_cmd(&exec_cmd, 10).unwrap();
        config.insert_cmd(&cfg_cmd, 20).unwrap();

        let exec_show = exec.next_state(exec.root_state(), "show").unwrap();
        let exec_version = exec.next_state(exec_show, "version").unwrap();
        let cfg_show = config.next_state(config.root_state(), "show").unwrap();
        let cfg_version = config.next_state(cfg_show, "version").unwrap();

        assert_ne!(exec.id(), config.id());
        assert_eq!(exec.get_completions(exec_version, ""), Vec::<&str>::new());
        assert_eq!(config.get_completions(cfg_version, ""), Vec::<&str>::new());
    }
}
