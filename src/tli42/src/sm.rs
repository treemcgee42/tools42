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
    /// Inputting anything ("variable"), with a required placeholder used in help
    /// completion output (e.g. "<name>").
    Var { placeholder: String },
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

/// Public type used to present a possible completion from e.g. the current state and
/// input. Essentially describes possible edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Completion<'a> {
    /// A single token describing the type. For literal edges, this is the
    /// literal. For variable edges, this is the placeholder.
    pub(crate) token: &'a str,
    /// Documentation for the completion/edge.
    pub(crate) doc: Option<&'a str>,
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
    /// E.g. tried to insert an edge at a state that doesn't exist.
    InvalidState(StateId),
    /// Tried to insert a command at a path / state but a different one already
    /// existed there.
    DuplicateCommandPath {
        existing: CommandId,
        attempted: CommandId,
    },
    /// Tried to insert a literal edge but it already existing with a different
    /// documentation.
    ConflictingLiteralDoc {
        state: StateId,
        literal: String,
        existing: String,
        attempted: String,
    },
    /// Tried to insert a variable edge but it already existed with a different
    /// placeholder.
    ConflictingVarPlaceholder {
        state: StateId,
        existing: String,
        attempted: String,
    },
    /// Tried to insert a variable edge but it already existed with a different
    /// documentation.
    ConflictingVarDoc {
        state: StateId,
        existing: String,
        attempted: String,
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
                Edge::Var { .. } => scan.candidates.push(link),
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
                Edge::Var { .. } => {
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
        self.get_completions_with_docs(current_state, partial_token)
            .into_iter()
            .map(|completion| completion.token)
            .collect()
    }

    pub(crate) fn get_completions_with_docs<'a>(
        &'a self,
        current_state: StateId,
        partial_token: &str,
    ) -> Vec<Completion<'a>> {
        let scan = self.scan_state(current_state, partial_token);
        // All literal matches (exact and prefix).
        let mut completions = scan
            .candidates
            .iter()
            .filter_map(|link| match &link.edge {
                Edge::Literal(token) => Some(Completion {
                    token: token.as_str(),
                    doc: link.doc.as_deref(),
                }),
                Edge::Var { .. } => None,
            })
            .collect::<Vec<_>>();

        // If there is partial input already matching something, then precedence
        // means we won't match the variable edge anyways so there's no point
        // displaying it as a completion (and it might mislead the user).
        if partial_token.is_empty() || completions.is_empty() {
            let mut var = None;
            let mut var_count = 0usize;
            for link in &scan.candidates {
                if matches!(link.edge, Edge::Var { .. }) {
                    var_count += 1;
                    var = Some(*link);
                }
            }
            assert!(
                var_count <= 1,
                "invariant violated: multiple var edges in completion scan"
            );

            if var_count == 1
                && let Some(link) = var
                && let Edge::Var { placeholder } = &link.edge
            {
                completions.push(Completion {
                    token: placeholder.as_str(),
                    doc: link.doc.as_deref(),
                });
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
                Edge::Var { .. } => MatchedEdgeKind::Var,
            },
        })
    }

    /// Starting at `current_state`, if there is an edge matching `edge` already,
    /// return the state it points to. Otherwise, create a new edge and state for it.
    fn ensure_edge(
        &mut self,
        current_state: StateId,
        edge: Edge,
        doc: Option<&str>,
    ) -> Result<StateId, CmdInsertError> {
        let state = self
            .states
            .get(current_state)
            .ok_or(CmdInsertError::InvalidState(current_state))?;

        let mut match_idx: Option<usize> = None;
        let mut count = 0usize;
        for (idx, link) in state.edges.iter().enumerate() {
            let is_match = match (&edge, &link.edge) {
                (Edge::Literal(target), Edge::Literal(existing)) => target == existing,
                (Edge::Var { .. }, Edge::Var { .. }) => true,
                _ => false,
            };
            if is_match {
                count += 1;
                match_idx = Some(idx);
            }
        }
        assert!(
            count <= 1,
            "invariant violated: multiple matching edges in state {} for {:?}",
            current_state, edge
        );

        if let Some(idx) = match_idx {
            let link = &mut self.states[current_state].edges[idx];
            if let (Edge::Var { placeholder: attempted }, Edge::Var { placeholder: existing }) =
                (&edge, &link.edge)
                && existing != attempted
            {
                return Err(CmdInsertError::ConflictingVarPlaceholder {
                    state: current_state,
                    existing: existing.clone(),
                    attempted: attempted.clone(),
                });
            }

            if let Some(attempted) = doc {
                if let Some(existing) = link.doc.as_deref() {
                    if existing != attempted {
                        return match &edge {
                            Edge::Literal(literal) => Err(CmdInsertError::ConflictingLiteralDoc {
                                state: current_state,
                                literal: literal.clone(),
                                existing: existing.to_string(),
                                attempted: attempted.to_string(),
                            }),
                            Edge::Var { .. } => Err(CmdInsertError::ConflictingVarDoc {
                                state: current_state,
                                existing: existing.to_string(),
                                attempted: attempted.to_string(),
                            }),
                        };
                    }
                } else {
                    link.doc = Some(attempted.to_string());
                }
            }

            return Ok(link.next_state);
        }

        let next_state = self.states.len();
        self.states.push(State::default());
        self.states[current_state].edges.push(EdgeLink {
            edge,
            next_state,
            doc: doc.map(str::to_string),
        });
        Ok(next_state)
    }

    /// Starting at `current_state`, if there is a literal edge for `literal` already, return
    /// the state it points to. Otherwise, create a new edge and state for it.
    pub(crate) fn ensure_literal_edge(
        &mut self,
        current_state: StateId,
        literal: &str,
        doc: Option<&str>,
    ) -> Result<StateId, CmdInsertError> {
        self.ensure_edge(current_state, Edge::Literal(literal.to_string()), doc)
    }

    /// Starting at `current_state`, if there is a var edge return the state it points to. Otherwise,
    /// create a new edge and state for it, returning the new state.
    pub(crate) fn ensure_var_edge(
        &mut self,
        current_state: StateId,
        placeholder: &str,
        doc: Option<&str>,
    ) -> Result<StateId, CmdInsertError> {
        self.ensure_edge(
            current_state,
            Edge::Var {
                placeholder: placeholder.to_string(),
            },
            doc,
        )
    }

    /// Register a command to be accepted if input terminates at `state_id`. This
    /// makes `state_id` a terminal State.
    pub(crate) fn set_accept(
        &mut self,
        state_id: StateId,
        id: CommandId,
    ) -> Result<(), CmdInsertError> {
        let state = self
            .states
            .get_mut(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;

        if let Some(existing) = &state.accept && existing.command_id != id {
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

    /// Returns the command, if any, at `state_id`.
    pub(crate) fn accept_at(&self, state_id: StateId) -> Result<Option<CommandId>, CmdInsertError> {
        let state = self
            .states
            .get(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;
        Ok(state.accept.as_ref().map(|a| a.command_id))
    }

    /// Set the documentation for a given literal edge at `current_state`. This is
    /// useful when there's a command stem with multiple commands behind it and you
    /// want to document that commonality. Rather than writing the documentation at
    /// that point during each command insertion, you can do it once.
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
                assert!(
                    match_idx.is_none(),
                    "invariant violated: duplicate literal edge '{}' in state {}",
                    literal,
                    current_state
                );
                match_idx = Some(idx);
            }
        }

        if let Some(idx) = match_idx {
            state.edges[idx].doc = Some(doc);
            return Ok(true);
        }
        Ok(false)
    }

    /// Set the documentation for the command at `state_id`. If there is no command
    /// there, return false. Otherwise set it and return true.
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

    /// Get the command documentation at `state_id`, if any.
    pub(crate) fn command_doc_at(&self, state_id: StateId) -> Result<Option<&str>, CmdInsertError> {
        let state = self
            .states
            .get(state_id)
            .ok_or(CmdInsertError::InvalidState(state_id))?;
        Ok(state.accept.as_ref().and_then(|a| a.doc.as_deref()))
    }

}

#[cfg(test)]
impl Sm {
    /// Returns the next state if input_token resolves uniquely under CLI abbreviation rules.
    pub(crate) fn next_state(&self, current_state: StateId, input_token: &str) -> Option<StateId> {
        self.step(current_state, input_token)
            .map(|step| step.next_state)
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
        }
    }

    fn var_edge(next_state: StateId) -> EdgeLink {
        EdgeLink {
            edge: Edge::Var {
                placeholder: "<arg>".to_string(),
            },
            next_state,
            doc: None,
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
                edge: Edge::Var {
                    placeholder: "<name>".to_string(),
                },
                next_state: 1,
                doc: Some("account name".to_string()),
            }],
            accept: None,
        }]);

        assert_eq!(
            sm.get_completions_with_docs(0, ""),
            vec![Completion {
                token: "<name>",
                doc: Some("account name")
            }]
        );
        assert_eq!(
            sm.get_completions_with_docs(0, "na"),
            vec![Completion {
                token: "<name>",
                doc: Some("account name")
            }]
        );
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

        let first = sm.ensure_literal_edge(0, "show", None).unwrap();
        let second = sm.ensure_literal_edge(0, "show", None).unwrap();

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

        let first = sm.ensure_var_edge(0, "<arg>", None).unwrap();
        let second = sm.ensure_var_edge(0, "<arg>", None).unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 1);
        assert_eq!(sm.states[0].edges.len(), 1);
    }

    #[test]
    fn ensure_literal_edge_rejects_conflicting_doc() {
        let mut sm = sm_with_states(vec![
            State {
                edges: vec![lit_edge("show", 1)],
                accept: None,
            },
            State::default(),
        ]);
        sm.states[0].edges[0].doc = Some("old".to_string());

        let err = sm.ensure_literal_edge(0, "show", Some("new")).unwrap_err();
        assert_eq!(
            err,
            CmdInsertError::ConflictingLiteralDoc {
                state: 0,
                literal: "show".to_string(),
                existing: "old".to_string(),
                attempted: "new".to_string()
            }
        );
    }

    #[test]
    fn ensure_var_edge_rejects_conflicting_placeholder() {
        let mut sm = sm_with_states(vec![
            State {
                edges: vec![var_edge(1)],
                accept: None,
            },
            State::default(),
        ]);

        let err = sm.ensure_var_edge(0, "<name>", None).unwrap_err();
        assert_eq!(
            err,
            CmdInsertError::ConflictingVarPlaceholder {
                state: 0,
                existing: "<arg>".to_string(),
                attempted: "<name>".to_string()
            }
        );
    }

    #[test]
    fn ensure_var_edge_rejects_conflicting_doc() {
        let mut sm = sm_with_states(vec![
            State {
                edges: vec![var_edge(1)],
                accept: None,
            },
            State::default(),
        ]);
        sm.states[0].edges[0].doc = Some("old".to_string());

        let err = sm.ensure_var_edge(0, "<arg>", Some("new")).unwrap_err();
        assert_eq!(
            err,
            CmdInsertError::ConflictingVarDoc {
                state: 0,
                existing: "old".to_string(),
                attempted: "new".to_string()
            }
        );
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
            sm.ensure_literal_edge(99, "show", None).unwrap_err(),
            CmdInsertError::InvalidState(99)
        );
        assert_eq!(
            sm.ensure_var_edge(99, "<arg>", None).unwrap_err(),
            CmdInsertError::InvalidState(99)
        );
        assert_eq!(
            sm.set_accept(99, 1).unwrap_err(),
            CmdInsertError::InvalidState(99)
        );
    }
}
