use crate::sm;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Atom {
    Literal(String),
    Var,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Sequence(Vec<Atom>),
    // Future:
    // - optional
    // - labeled positional
    // - etc.
}

type CmdId = sm::CommandId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cmd {
    exprs: Vec<Expr>,
    id: CmdId,
}

pub(crate) struct CmdBuilder {
    cmd: Cmd,
}

impl CmdBuilder {
    pub(crate) fn new(id: CmdId) -> Self {
        Self {
            cmd: Cmd {
                exprs: Vec::new(),
                id,
            },
        }
    }

    pub(crate) fn literals(&mut self, literals: &[&str]) -> &mut Self {
        if literals.is_empty() {
            return self;
        }

        let atoms = literals
            .iter()
            .map(|s| Atom::Literal((*s).to_string()))
            .collect::<Vec<_>>();
        self.cmd.exprs.push(Expr::Sequence(atoms));
        self
    }

    pub(crate) fn positional_args(&mut self, num: u8) -> &mut Self {
        if num == 0 {
            return self;
        }

        let atoms = (0..num).map(|_| Atom::Var).collect::<Vec<_>>();
        self.cmd.exprs.push(Expr::Sequence(atoms));
        self
    }

    pub(crate) fn build(self) -> Cmd {
        self.cmd
    }
}

impl sm::Sm {
    fn insert_atom(
        &mut self,
        current_state: sm::StateId,
        atom: &Atom,
    ) -> Result<sm::StateId, sm::CmdInsertError> {
        match atom {
            Atom::Literal(literal) => self.ensure_literal_edge(current_state, literal),
            Atom::Var => self.ensure_var_edge(current_state),
        }
    }

    fn insert_expr(
        &mut self,
        current_state: sm::StateId,
        expr: &Expr,
    ) -> Result<sm::StateId, sm::CmdInsertError> {
        match expr {
            Expr::Sequence(atoms) => {
                let mut next_state = current_state;
                for atom in atoms {
                    next_state = self.insert_atom(next_state, atom)?;
                }
                Ok(next_state)
            }
        }
    }

    pub(crate) fn insert_cmd(&mut self, cmd: &Cmd) -> Result<(), sm::CmdInsertError> {
        let mut current_state: sm::StateId = 0;
        for expr in &cmd.exprs {
            current_state = self.insert_expr(current_state, expr)?;
        }
        self.set_accept(current_state, cmd.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_constructs_concatenated_sequences() {
        let mut builder = CmdBuilder::new(7);
        builder.literals(&["show", "ip"]).positional_args(2);
        let cmd = builder.build();

        assert_eq!(cmd.id, 7);
        assert_eq!(
            cmd.exprs,
            vec![
                Expr::Sequence(vec![
                    Atom::Literal("show".to_string()),
                    Atom::Literal("ip".to_string())
                ]),
                Expr::Sequence(vec![Atom::Var, Atom::Var]),
            ]
        );
    }

    #[test]
    fn insert_cmd_creates_path_and_marks_accept() {
        let mut sm = sm::Sm::new();
        let mut builder = CmdBuilder::new(10);
        builder.literals(&["show", "ip"]).positional_args(1);
        let cmd = builder.build();

        sm.insert_cmd(&cmd).unwrap();

        let s1 = sm.next_state(0, "show").unwrap();
        let s2 = sm.next_state(s1, "ip").unwrap();
        let s3 = sm.next_state(s2, "eth0").unwrap();
        assert_eq!(sm.accept_at(s3).unwrap(), Some(10));
    }

    #[test]
    fn insert_cmd_reuses_shared_prefix() {
        let mut sm = sm::Sm::new();

        let mut a = CmdBuilder::new(1);
        a.literals(&["show", "ip", "route"]);
        let cmd_a = a.build();
        sm.insert_cmd(&cmd_a).unwrap();

        let mut b = CmdBuilder::new(2);
        b.literals(&["show", "ip", "interface"]);
        let cmd_b = b.build();
        sm.insert_cmd(&cmd_b).unwrap();

        let show = sm.next_state(0, "show").unwrap();
        let ip = sm.next_state(show, "ip").unwrap();
        let route = sm.next_state(ip, "route").unwrap();
        let iface = sm.next_state(ip, "interface").unwrap();

        assert_eq!(sm.accept_at(route).unwrap(), Some(1));
        assert_eq!(sm.accept_at(iface).unwrap(), Some(2));
    }

    #[test]
    fn insert_cmd_rejects_duplicate_terminal_path() {
        let mut sm = sm::Sm::new();

        let mut a = CmdBuilder::new(1);
        a.literals(&["show", "version"]);
        let cmd_a = a.build();
        sm.insert_cmd(&cmd_a).unwrap();

        let mut b = CmdBuilder::new(2);
        b.literals(&["show", "version"]);
        let cmd_b = b.build();

        let err = sm.insert_cmd(&cmd_b).unwrap_err();
        assert_eq!(
            err,
            sm::CmdInsertError::DuplicateCommandPath {
                existing: 1,
                attempted: 2
            }
        );
    }

    #[test]
    fn insert_cmd_allows_var_and_literal_branching() {
        let mut sm = sm::Sm::new();

        let mut a = CmdBuilder::new(1);
        a.literals(&["show"]).positional_args(1);
        let cmd_a = a.build();
        sm.insert_cmd(&cmd_a).unwrap();

        let mut b = CmdBuilder::new(2);
        b.literals(&["show", "version"]);
        let cmd_b = b.build();
        sm.insert_cmd(&cmd_b).unwrap();

        let show = sm.next_state(0, "show").unwrap();
        let var_state = sm.next_state(show, "eth0").unwrap();
        let ver_state = sm.next_state(show, "version").unwrap();

        assert_eq!(sm.accept_at(var_state).unwrap(), Some(1));
        assert_eq!(sm.accept_at(ver_state).unwrap(), Some(2));
    }
}
