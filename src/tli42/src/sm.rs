pub(crate) type StateId = usize;
pub(crate) type CommandId = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Edge {
    Literal(String),
    Var,
}

#[derive(Debug, Default)]
struct State {
    edges: Vec<(Edge, StateId)>,
    accept: Option<CommandId>,
}

#[derive(Debug, Default)]
pub(crate) struct Sm {
    // Initial state is 0.
    states: Vec<State>,
}

#[derive(Debug, Default)]
struct StateScan<'a> {
    exact_literal: Option<StateId>,
    exact_literal_count: usize,
    prefix_literal: Option<StateId>,
    prefix_literal_count: usize,
    var_match: Option<StateId>,
    var_match_count: usize,
    completions: Vec<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CmdInsertError {
    InvalidState(StateId),
    MultipleVarEdges(StateId),
    DuplicateLiteralEdges { state: StateId, literal: String },
    DuplicateCommandPath { existing: CommandId, attempted: CommandId },
}

impl Sm {
    pub(crate) fn new() -> Self {
        Self {
            states: vec![State::default()],
        }
    }

    fn scan_state<'a>(
        &'a self,
        current_state: StateId,
        input_token: &str,
        collect_completions: bool,
    ) -> Option<StateScan<'a>> {
        let state = self.states.get(current_state)?;
        let mut scan = StateScan::default();

        for (edge, state_id) in &state.edges {
            match edge {
                Edge::Literal(complete_token) => {
                    if collect_completions && complete_token.starts_with(input_token) {
                        scan.completions.push(complete_token.as_str());
                    }

                    if complete_token == input_token {
                        scan.exact_literal_count += 1;
                        if scan.exact_literal_count == 1 {
                            scan.exact_literal = Some(*state_id);
                        } else {
                            scan.exact_literal = None;
                        }
                    }

                    if complete_token.starts_with(input_token) {
                        scan.prefix_literal_count += 1;
                        if scan.prefix_literal_count == 1 {
                            scan.prefix_literal = Some(*state_id);
                        } else {
                            scan.prefix_literal = None;
                        }
                    }
                }
                Edge::Var => {
                    scan.var_match_count += 1;
                    if scan.var_match_count == 1 {
                        scan.var_match = Some(*state_id);
                    } else {
                        scan.var_match = None;
                    }
                }
            }
        }

        Some(scan)
    }

    /// Get a list of all possible next literal tokens.
    pub(crate) fn get_completions<'a>(
        &'a self,
        current_state: StateId,
        partial_token: &str,
    ) -> Vec<&'a str> {
        self.scan_state(current_state, partial_token, true)
            .map(|scan| scan.completions)
            .unwrap_or_default()
    }

    /// Returns the next state if input_token resolves uniquely under CLI abbreviation rules.
    pub(crate) fn next_state(&self, current_state: StateId, input_token: &str) -> Option<StateId> {
        let scan = self.scan_state(current_state, input_token, false)?;

        if scan.exact_literal_count == 1 {
            return scan.exact_literal;
        }
        if scan.exact_literal_count > 1 {
            return None;
        }

        if scan.prefix_literal_count == 1 {
            return scan.prefix_literal;
        }
        if scan.prefix_literal_count > 1 {
            return None;
        }

        if scan.var_match_count == 1 {
            return scan.var_match;
        }

        None
    }

    /// Starting at `current_state`, if there is a literal edge for `literal` already, return
    /// the state it points to. Otherwise, create a new edge and state for it.
    pub(crate) fn ensure_literal_edge(
        &mut self,
        current_state: StateId,
        literal: &str,
    ) -> Result<StateId, CmdInsertError> {
        let state = self
            .states
            .get(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut existing: Option<StateId> = None;
        let mut literal_count = 0usize;
        for (edge, next_state) in &state.edges {
            if let Edge::Literal(existing_literal) = edge {
                if existing_literal == literal {
                    literal_count += 1;
                    if literal_count == 1 {
                        existing = Some(*next_state);
                    }
                }
            }
        }

        if literal_count > 1 {
            return Err(CmdInsertError::DuplicateLiteralEdges {
                state: current_state,
                literal: literal.to_string(),
            });
        }
        if let Some(state_id) = existing {
            return Ok(state_id);
        }

        let new_state = self.states.len();
        self.states.push(State::default());
        self.states[current_state]
            .edges
            .push((Edge::Literal(literal.to_string()), new_state));
        Ok(new_state)
    }

    /// Starting at `current_state`, if there is a var edge return the state it points to. Otherwise,
    /// create a new edge and state for it, returning the new state.
    pub(crate) fn ensure_var_edge(
        &mut self,
        current_state: StateId,
    ) -> Result<StateId, CmdInsertError> {
        let state = self
            .states
            .get(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut existing: Option<StateId> = None;
        let mut var_count = 0usize;
        for (edge, next_state) in &state.edges {
            if matches!(edge, Edge::Var) {
                var_count += 1;
                if var_count == 1 {
                    existing = Some(*next_state);
                }
            }
        }

        if var_count > 1 {
            return Err(CmdInsertError::MultipleVarEdges(current_state));
        }
        if let Some(state_id) = existing {
            return Ok(state_id);
        }

        let new_state = self.states.len();
        self.states.push(State::default());
        self.states[current_state].edges.push((Edge::Var, new_state));
        Ok(new_state)
    }

    pub(crate) fn set_accept(
        &mut self,
        state_id: StateId,
        id: CommandId,
    ) -> Result<(), CmdInsertError> {
        let state = self
            .states
            .get_mut(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;

        if let Some(existing) = state.accept {
            return Err(CmdInsertError::DuplicateCommandPath {
                existing,
                attempted: id,
            });
        }

        state.accept = Some(id);
        Ok(())
    }

    pub(crate) fn accept_at(
        &self,
        state_id: StateId,
    ) -> Result<Option<CommandId>, CmdInsertError> {
        let state = self
            .states
            .get(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;
        Ok(state.accept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(s: &str) -> Edge {
        Edge::Literal(s.to_string())
    }

    fn sm_with_states(states: Vec<State>) -> Sm {
        Sm { states }
    }

    fn sorted_strings(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_unstable();
        v
    }

    #[test]
    fn get_completions_returns_matching_literals_only() {
        let sm = sm_with_states(vec![State {
            edges: vec![(lit("show"), 1), (Edge::Var, 2), (lit("shell"), 3)],
            accept: None,
        }]);

        let completions = sorted_strings(sm.get_completions(0, "sh"));
        assert_eq!(completions, vec!["shell", "show"]);
    }

    #[test]
    fn next_state_prefers_exact_literal_over_var() {
        let sm = sm_with_states(vec![State {
            edges: vec![(lit("show"), 1), (Edge::Var, 2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "show"), Some(1));
    }

    #[test]
    fn next_state_accepts_unique_literal_prefix() {
        let sm = sm_with_states(vec![State {
            edges: vec![(lit("show"), 1), (Edge::Var, 2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "sh"), Some(1));
    }

    #[test]
    fn next_state_rejects_ambiguous_literal_prefix() {
        let sm = sm_with_states(vec![State {
            edges: vec![(lit("show"), 1), (lit("shell"), 2), (Edge::Var, 3)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "sh"), None);
    }

    #[test]
    fn next_state_falls_back_to_var_when_no_literal_matches() {
        let sm = sm_with_states(vec![State {
            edges: vec![(lit("show"), 1), (Edge::Var, 2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "interface0"), Some(2));
    }

    #[test]
    fn next_state_rejects_multiple_var_edges() {
        let sm = sm_with_states(vec![State {
            edges: vec![(Edge::Var, 1), (Edge::Var, 2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "anything"), None);
    }

    #[test]
    fn invalid_state_returns_none_and_empty_completions() {
        let sm = sm_with_states(vec![State::default()]);

        assert_eq!(sm.next_state(9, "show"), None);
        assert!(sm.get_completions(9, "sh").is_empty());
    }

    #[test]
    fn accepting_state_metadata_is_stored() {
        let sm = sm_with_states(vec![
            State {
                edges: vec![],
                accept: None,
            },
            State {
                edges: vec![],
                accept: Some(42),
            },
        ]);

        assert_eq!(sm.states[0].accept, None);
        assert_eq!(sm.states[1].accept, Some(42));
    }

    #[test]
    fn ensure_literal_edge_reuses_existing_edge() {
        let mut sm = sm_with_states(vec![State {
            edges: vec![(lit("show"), 1)],
            accept: None,
        }, State::default()]);

        let first = sm.ensure_literal_edge(0, "show").unwrap();
        let second = sm.ensure_literal_edge(0, "show").unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 1);
        assert_eq!(sm.states[0].edges.len(), 1);
    }

    #[test]
    fn ensure_var_edge_reuses_existing_edge() {
        let mut sm = sm_with_states(vec![State {
            edges: vec![(Edge::Var, 1)],
            accept: None,
        }, State::default()]);

        let first = sm.ensure_var_edge(0).unwrap();
        let second = sm.ensure_var_edge(0).unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 1);
        assert_eq!(sm.states[0].edges.len(), 1);
    }

    #[test]
    fn set_accept_rejects_duplicate_terminal_registration() {
        let mut sm = sm_with_states(vec![State::default()]);

        sm.set_accept(0, 10).unwrap();
        let err = sm.set_accept(0, 11).unwrap_err();

        assert_eq!(
            err,
            CmdInsertError::DuplicateCommandPath {
                existing: 10,
                attempted: 11
            }
        );
    }

    #[test]
    fn registration_helpers_return_invalid_state_error() {
        let mut sm = Sm::new();

        assert_eq!(
            sm.ensure_literal_edge(99, "show").unwrap_err(),
            CmdInsertError::InvalidState(99)
        );
        assert_eq!(
            sm.ensure_var_edge(99).unwrap_err(),
            CmdInsertError::InvalidState(99)
        );
        assert_eq!(
            sm.set_accept(99, 1).unwrap_err(),
            CmdInsertError::InvalidState(99)
        );
    }
}
