//! This module contains the data structure for the REPL command position. We use
//! something like a state machine or DFA. The "state" is where the user currently is
//! in the command graph. It is represented by `State` and referenced by
//! `StateId`. Movement to another state is only possible through an "edge", which
//! represents the action the user must take, e.g. what they must type, in order to
//! reach the next state. Any state may also be terminal, in the sense that stopping
//! at the current state represents a valid command.

pub(crate) type StateId = usize;
pub(crate) type CommandId = u32;

// TODO: do we check coherence during construction?
/// Criterion to move from one state to another. Note that a collection of edges for
/// a state is not automatically coherent; e.g. it does not make sense to have
/// multiple `Var` edges as they could both match.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Edge {
    /// Inputting a literal string. We do not implement (unique) partial completion
    /// matching at this level; the user of the SM should implement that themselves.
    Literal(String),
    /// Inputting anything ("variable").
    Var,
}

/// An enriched edge that's more practical to use in the state type. It essentially
/// groups associated data and metadata for an edge.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EdgeLink {
    edge: Edge,
    next_state: StateId,
    // TODO: how do we reconcile the documentation of the edge to a terminal with
    // the documentation of the terminal's command itself?
    /// Optional documentation. For instance, if `next_state` is terminal this could
    /// describe the command at that state. If there are multiple possible commands
    /// after the next state, this could document the umbrella all these commands are
    /// grouped under.
    doc: Option<String>,
    // TODO: should doc and var_completion be unified into a single type?
    /// A public identifier for the input needed to match this edge. Used in
    /// conjunction with the documentation. E.g. if we want to accept a variable that
    /// is the name of the output file, this could be `output-file`.
    var_completion: Option<String>,
}

/// Terminal data for a state, e.g. the command and documentation for the command if
/// the SM is exited at a given state.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AcceptMeta {
    command_id: CommandId,
    doc: Option<String>,
}

/// Represents a state in the SM graph / DFA.
#[derive(Debug, Clone, Default)]
struct State {
    /// All possible ways to advance the state. Note the edges should be coherent
    /// (see documentation there).
    edges: Vec<EdgeLink>,
    /// If provided, this state is a "terminal" and exiting here means e.g. invoking
    /// the enclosed command.
    accept: Option<AcceptMeta>,
}

/// Represents the SM / DFA. It is pretty much stateless after construction; it is up
/// to the user of this type to store the "current state". Transitions between
/// states, e.g. via the `step` method, simply return what the next state would be,
/// and it is up to the user to decide what to do with this (e.g. set that to their
/// current state).
#[derive(Debug, Clone, Default)]
pub(crate) struct Sm {
    /// List of states in the SM. The index is a stable identifier; we currently do
    /// not support reordering. The initial state is at index 0.
    states: Vec<State>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchedEdgeKind {
    Literal,
    Var,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StepResult {
    pub(crate) next_state: StateId,
    pub(crate) matched: MatchedEdgeKind,
}

/// Result of querying possible transitions, e.g. from a given state and given a
/// partial input.
#[derive(Debug, Default)]
struct ScanResult<'a> {
    candidates: Vec<&'a EdgeLink>,
    winner: Option<&'a EdgeLink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CmdInsertError {
    InvalidState(StateId),
    MultipleVarEdges(StateId),
    DuplicateLiteralEdges {
        state: StateId,
        literal: String,
    },
    DuplicateCommandPath {
        existing: CommandId,
        attempted: CommandId,
    },
}

impl Sm {
    pub(crate) fn new() -> Self {
        Self {
            states: vec![State::default()],
        }
    }

    /// Determine the completions and winner from the current state given the input
    /// token.
    fn scan_state<'a>(&'a self, current_state: StateId, input_token: &str) -> ScanResult<'a> {
        let state = self
            .states
            .get(current_state)
            .expect("invalid state id in SM");
        let mut scan = ScanResult::default();

        for link in &state.edges {
            match &link.edge {
                Edge::Literal(complete_token) => {
                    if complete_token.starts_with(input_token) {
                        scan.candidates.push(link);
                    }
                }
                Edge::Var => scan.candidates.push(link),
            }
        }
        scan.winner = Self::resolve_winner(&scan.candidates, input_token);

        scan
    }

    /// Given a list of completion candidates for the input token, determine the one that should be
    /// chosen to advance on. This uses the following precedence rules, in descending order:
    /// - exact match
    /// - prefix match
    /// - variable match
    ///
    /// If there are multiple matches of the same precedence, or else we cannot
    /// determine a clear winner, we return `None`.
    fn resolve_winner<'a>(candidates: &[&'a EdgeLink], input_token: &str) -> Option<&'a EdgeLink> {
        let mut exact = None;
        let mut exact_count = 0usize;
        let mut prefix = None;
        let mut prefix_count = 0usize;
        let mut var = None;
        let mut var_count = 0usize;

        for link in candidates {
            match &link.edge {
                Edge::Literal(token) if token == input_token => {
                    exact_count += 1;
                    if exact_count == 1 {
                        exact = Some(*link);
                    } else {
                        exact = None;
                    }
                }
                Edge::Literal(_) => {
                    prefix_count += 1;
                    if prefix_count == 1 {
                        prefix = Some(*link);
                    } else {
                        prefix = None;
                    }
                }
                Edge::Var => {
                    var_count += 1;
                    if var_count == 1 {
                        var = Some(*link);
                    } else {
                        var = None;
                    }
                }
            }
        }

        if exact_count == 1 {
            return exact;
        }
        if exact_count > 1 {
            return None;
        }
        if prefix_count == 1 {
            return prefix;
        }
        if prefix_count > 1 {
            return None;
        }
        if var_count == 1 {
            return var;
        }

        None
    }

    /// Get a list of all possible next literal tokens.
    pub(crate) fn get_completions<'a>(
        &'a self,
        current_state: StateId,
        partial_token: &str,
    ) -> Vec<&'a str> {
        self.scan_state(current_state, partial_token)
            .candidates
            .into_iter()
            .filter_map(|link| match &link.edge {
                Edge::Literal(token) => Some(token.as_str()),
                Edge::Var => None,
            })
            .collect()
    }

    pub(crate) fn get_completions_with_docs<'a>(
        &'a self,
        current_state: StateId,
        partial_token: &str,
    ) -> Vec<(&'a str, Option<&'a str>)> {
        let scan = self.scan_state(current_state, partial_token);
        // All literal matches (exact and prefix).
        let mut completions = scan
            .candidates
            .iter()
            .filter_map(|link| match &link.edge {
                Edge::Literal(token) => Some((token.as_str(), link.doc.as_deref())),
                Edge::Var => None,
            })
            .collect::<Vec<_>>();

        // TODO: why do we only add var if the partial is empty?
        if partial_token.is_empty() {
            let mut var = None;
            let mut var_count = 0usize;
            for link in &scan.candidates {
                if matches!(link.edge, Edge::Var) {
                    var_count += 1;
                    if var_count == 1 {
                        var = Some(*link);
                    } else {
                        var = None;
                    }
                }
            }

            // TODO: isn't it an error to have multiple var? Should we just assert?
            if var_count == 1
                && let Some(link) = var
                && let Some(token) = link.var_completion.as_deref()
            {
                completions.push((token, link.doc.as_deref()));
            }
        }

        completions
    }

    /// Returns the chosen edge kind + next state under CLI abbreviation rules.
    pub(crate) fn step(&self, current_state: StateId, input_token: &str) -> Option<StepResult> {
        let scan = self.scan_state(current_state, input_token);
        let winner = scan.winner?;
        Some(StepResult {
            next_state: winner.next_state,
            matched: match winner.edge {
                Edge::Literal(_) => MatchedEdgeKind::Literal,
                Edge::Var => MatchedEdgeKind::Var,
            },
        })
    }

    /// Returns the next state if input_token resolves uniquely under CLI abbreviation rules.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn next_state(&self, current_state: StateId, input_token: &str) -> Option<StateId> {
        self.step(current_state, input_token)
            .map(|step| step.next_state)
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
        for link in &state.edges {
            if let Edge::Literal(existing_literal) = &link.edge
                && existing_literal == literal
            {
                literal_count += 1;
                if literal_count == 1 {
                    existing = Some(link.next_state);
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
        self.states[current_state].edges.push(EdgeLink {
            edge: Edge::Literal(literal.to_string()),
            next_state: new_state,
            doc: None,
            var_completion: None,
        });
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
        for link in &state.edges {
            if matches!(link.edge, Edge::Var) {
                var_count += 1;
                if var_count == 1 {
                    existing = Some(link.next_state);
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
        self.states[current_state].edges.push(EdgeLink {
            edge: Edge::Var,
            next_state: new_state,
            doc: None,
            var_completion: None,
        });
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

        if let Some(existing) = &state.accept {
            return Err(CmdInsertError::DuplicateCommandPath {
                existing: existing.command_id,
                attempted: id,
            });
        }

        state.accept = Some(AcceptMeta {
            command_id: id,
            doc: None,
        });
        Ok(())
    }

    pub(crate) fn accept_at(&self, state_id: StateId) -> Result<Option<CommandId>, CmdInsertError> {
        let state = self
            .states
            .get(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;
        Ok(state.accept.as_ref().map(|a| a.command_id))
    }

    pub(crate) fn set_literal_edge_doc(
        &mut self,
        current_state: StateId,
        literal: &str,
        doc: String,
    ) -> Result<bool, CmdInsertError> {
        let state = self
            .states
            .get_mut(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut match_idx: Option<usize> = None;
        for (idx, link) in state.edges.iter().enumerate() {
            if let Edge::Literal(existing_literal) = &link.edge
                && existing_literal == literal
            {
                if match_idx.is_some() {
                    return Err(CmdInsertError::DuplicateLiteralEdges {
                        state: current_state,
                        literal: literal.to_string(),
                    });
                }
                match_idx = Some(idx);
            }
        }

        if let Some(idx) = match_idx {
            state.edges[idx].doc = Some(doc);
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn literal_edge_state(
        &self,
        current_state: StateId,
        literal: &str,
    ) -> Result<Option<StateId>, CmdInsertError> {
        let state = self
            .states
            .get(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut found = None;
        for link in &state.edges {
            if let Edge::Literal(existing_literal) = &link.edge
                && existing_literal == literal
            {
                if found.is_some() {
                    return Err(CmdInsertError::DuplicateLiteralEdges {
                        state: current_state,
                        literal: literal.to_string(),
                    });
                }
                found = Some(link.next_state);
            }
        }

        Ok(found)
    }

    pub(crate) fn var_edge_state(&self, current_state: StateId) -> Result<Option<StateId>, CmdInsertError> {
        let state = self
            .states
            .get(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut found = None;
        let mut count = 0usize;
        for link in &state.edges {
            if matches!(link.edge, Edge::Var) {
                count += 1;
                if count == 1 {
                    found = Some(link.next_state);
                }
            }
        }

        if count > 1 {
            return Err(CmdInsertError::MultipleVarEdges(current_state));
        }

        Ok(found)
    }

    pub(crate) fn set_command_doc(
        &mut self,
        state_id: StateId,
        doc: String,
    ) -> Result<bool, CmdInsertError> {
        let state = self
            .states
            .get_mut(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;
        let Some(accept) = &mut state.accept else {
            return Ok(false);
        };
        accept.doc = Some(doc);
        Ok(true)
    }

    pub(crate) fn set_var_edge_doc(
        &mut self,
        current_state: StateId,
        completion: String,
        doc: String,
    ) -> Result<bool, CmdInsertError> {
        let state = self
            .states
            .get_mut(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut match_idx: Option<usize> = None;
        let mut var_count = 0usize;
        for (idx, link) in state.edges.iter().enumerate() {
            if matches!(link.edge, Edge::Var) {
                var_count += 1;
                if var_count == 1 {
                    match_idx = Some(idx);
                }
            }
        }

        if var_count > 1 {
            return Err(CmdInsertError::MultipleVarEdges(current_state));
        }

        if let Some(idx) = match_idx {
            state.edges[idx].var_completion = Some(completion);
            state.edges[idx].doc = Some(doc);
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn command_doc_at(&self, state_id: StateId) -> Result<Option<&str>, CmdInsertError> {
        let state = self
            .states
            .get(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;
        Ok(state.accept.as_ref().and_then(|a| a.doc.as_deref()))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit_edge(s: &str, next_state: StateId) -> EdgeLink {
        EdgeLink {
            edge: Edge::Literal(s.to_string()),
            next_state,
            doc: None,
            var_completion: None,
        }
    }

    fn var_edge(next_state: StateId) -> EdgeLink {
        EdgeLink {
            edge: Edge::Var,
            next_state,
            doc: None,
            var_completion: None,
        }
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
            edges: vec![lit_edge("show", 1), var_edge(2), lit_edge("shell", 3)],
            accept: None,
        }]);

        let completions = sorted_strings(sm.get_completions(0, "sh"));
        assert_eq!(completions, vec!["shell", "show"]);
    }

    #[test]
    fn get_completions_with_docs_includes_var_placeholder_when_available() {
        let sm = sm_with_states(vec![State {
            edges: vec![EdgeLink {
                edge: Edge::Var,
                next_state: 1,
                doc: Some("account name".to_string()),
                var_completion: Some("<name>".to_string()),
            }],
            accept: None,
        }]);

        assert_eq!(
            sm.get_completions_with_docs(0, ""),
            vec![("<name>", Some("account name"))]
        );
        assert!(sm.get_completions_with_docs(0, "na").is_empty());
    }

    #[test]
    fn next_state_prefers_exact_literal_over_var() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), var_edge(2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "show"), Some(1));
    }

    #[test]
    fn next_state_accepts_unique_literal_prefix() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), var_edge(2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "sh"), Some(1));
    }

    #[test]
    fn next_state_rejects_ambiguous_literal_prefix() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), lit_edge("shell", 2), var_edge(3)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "sh"), None);
    }

    #[test]
    fn next_state_falls_back_to_var_when_no_literal_matches() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), var_edge(2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "interface0"), Some(2));
    }

    #[test]
    fn next_state_rejects_multiple_var_edges() {
        let sm = sm_with_states(vec![State {
            edges: vec![var_edge(1), var_edge(2)],
            accept: None,
        }]);

        assert_eq!(sm.next_state(0, "anything"), None);
    }

    #[test]
    fn step_reports_literal_match_kind() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), var_edge(2)],
            accept: None,
        }]);

        assert_eq!(
            sm.step(0, "sh"),
            Some(StepResult {
                next_state: 1,
                matched: MatchedEdgeKind::Literal
            })
        );
    }

    #[test]
    fn step_reports_var_match_kind() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), var_edge(2)],
            accept: None,
        }]);

        assert_eq!(
            sm.step(0, "eth0"),
            Some(StepResult {
                next_state: 2,
                matched: MatchedEdgeKind::Var
            })
        );
    }

    #[test]
    fn step_reuses_next_state_precedence() {
        let sm = sm_with_states(vec![State {
            edges: vec![lit_edge("show", 1), lit_edge("shell", 2), var_edge(3)],
            accept: None,
        }]);

        assert_eq!(
            sm.step(0, "show"),
            Some(StepResult {
                next_state: 1,
                matched: MatchedEdgeKind::Literal
            })
        );
        assert_eq!(sm.step(0, "sh"), None);
    }

    #[test]
    #[should_panic(expected = "invalid state id in SM")]
    fn invalid_state_panics() {
        let sm = sm_with_states(vec![State::default()]);
        let _ = sm.next_state(9, "show");
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
                accept: Some(AcceptMeta {
                    command_id: 42,
                    doc: None,
                }),
            },
        ]);

        assert_eq!(sm.states[0].accept, None);
        assert_eq!(
            sm.states[1].accept,
            Some(AcceptMeta {
                command_id: 42,
                doc: None
            })
        );
    }

    #[test]
    fn ensure_literal_edge_reuses_existing_edge() {
        let mut sm = sm_with_states(vec![
            State {
                edges: vec![lit_edge("show", 1)],
                accept: None,
            },
            State::default(),
        ]);

        let first = sm.ensure_literal_edge(0, "show").unwrap();
        let second = sm.ensure_literal_edge(0, "show").unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 1);
        assert_eq!(sm.states[0].edges.len(), 1);
    }

    #[test]
    fn ensure_var_edge_reuses_existing_edge() {
        let mut sm = sm_with_states(vec![
            State {
                edges: vec![var_edge(1)],
                accept: None,
            },
            State::default(),
        ]);

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
