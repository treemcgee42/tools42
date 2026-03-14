use crate::sm;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CaptureKind {
    Positional {
        name: Option<String>,
        doc: Option<String>,
    },
    Labeled {
        label: String,
        doc: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CmdSchemaError {
    DuplicateLabeledArg { label: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Atom {
    Literal {
        token: String,
        doc: Option<String>,
    },
    Var {
        name: Option<String>,
        doc: Option<String>,
    },
    LabeledVar {
        label: String,
        doc: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Sequence(Vec<Atom>),
    // Future:
    // - optional
    // - labeled positional
    // - etc.
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    exprs: Vec<Expr>,
    command_doc: Option<String>,
}

pub struct CmdBuilder {
    cmd: Cmd,
}

impl Default for CmdBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CmdBuilder {
    pub fn new() -> Self {
        Self {
            cmd: Cmd {
                exprs: Vec::new(),
                command_doc: None,
            },
        }
    }

    pub fn literals(&mut self, literals: &[&str]) -> &mut Self {
        if literals.is_empty() {
            return self;
        }

        let atoms = literals
            .iter()
            .map(|s| Atom::Literal {
                token: (*s).to_string(),
                doc: None,
            })
            .collect::<Vec<_>>();
        self.cmd.exprs.push(Expr::Sequence(atoms));
        self
    }

    pub fn literal_with_doc(&mut self, literal: &str, doc: impl Into<String>) -> &mut Self {
        self.cmd.exprs.push(Expr::Sequence(vec![Atom::Literal {
            token: literal.to_string(),
            doc: Some(doc.into()),
        }]));
        self
    }

    pub fn positional_args(&mut self, num: u8) -> &mut Self {
        if num == 0 {
            return self;
        }

        let atoms = (0..num)
            .map(|_| Atom::Var {
                name: None,
                doc: None,
            })
            .collect::<Vec<_>>();
        self.cmd.exprs.push(Expr::Sequence(atoms));
        self
    }

    pub fn positional_arg_with_doc(&mut self, name: &str, doc: impl Into<String>) -> &mut Self {
        self.cmd.exprs.push(Expr::Sequence(vec![Atom::Var {
            name: Some(name.to_string()),
            doc: Some(doc.into()),
        }]));
        self
    }

    pub fn labeled_arg(&mut self, label: &str) -> &mut Self {
        self.cmd.exprs.push(Expr::Sequence(vec![
            Atom::Literal {
                token: label.to_string(),
                doc: None,
            },
            Atom::LabeledVar {
                label: label.to_string(),
                doc: None,
            },
        ]));
        self
    }

    pub fn labeled_arg_with_doc(&mut self, label: &str, doc: impl Into<String>) -> &mut Self {
        let doc = doc.into();
        self.cmd.exprs.push(Expr::Sequence(vec![
            Atom::Literal {
                token: label.to_string(),
                doc: Some(doc.clone()),
            },
            Atom::LabeledVar {
                label: label.to_string(),
                doc: Some(doc),
            },
        ]));
        self
    }

    pub fn command_doc(&mut self, doc: impl Into<String>) -> &mut Self {
        self.cmd.command_doc = Some(doc.into());
        self
    }

    pub fn build(self) -> Cmd {
        self.cmd
    }
}

impl Cmd {
    pub(crate) fn capture_spec(&self) -> Result<Vec<CaptureKind>, CmdSchemaError> {
        let mut capture_spec = Vec::new();
        let mut seen_labeled = BTreeSet::new();

        for expr in &self.exprs {
            match expr {
                Expr::Sequence(atoms) => {
                    for atom in atoms {
                        match atom {
                            Atom::Literal { .. } => {}
                            Atom::Var { name, doc } => capture_spec.push(CaptureKind::Positional {
                                name: name.clone(),
                                doc: doc.clone(),
                            }),
                            Atom::LabeledVar { label, doc } => {
                                if !seen_labeled.insert(label.clone()) {
                                    return Err(CmdSchemaError::DuplicateLabeledArg {
                                        label: label.clone(),
                                    });
                                }
                                capture_spec.push(CaptureKind::Labeled {
                                    label: label.clone(),
                                    doc: doc.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(capture_spec)
    }

    pub(crate) fn command_doc(&self) -> Option<&str> {
        self.command_doc.as_deref()
    }
}

impl sm::Sm {
    fn insert_atom(
        &mut self,
        current_state: sm::StateId,
        atom: &Atom,
    ) -> Result<sm::StateId, sm::CmdInsertError> {
        match atom {
            Atom::Literal { token, doc } => {
                self.ensure_literal_edge(current_state, token, doc.as_deref())
            }
            Atom::Var { name, doc } => {
                let placeholder = format!("<{}>", name.as_deref().unwrap_or("arg"));
                self.ensure_var_edge(current_state, &placeholder, doc.as_deref())
            }
            Atom::LabeledVar { label, doc } => {
                let placeholder = format!("<{}>", label);
                self.ensure_var_edge(current_state, &placeholder, doc.as_deref())
            }
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

    pub(crate) fn insert_cmd(
        &mut self,
        cmd: &Cmd,
        command_id: sm::CommandId,
    ) -> Result<(), sm::CmdInsertError> {
        let mut current_state: sm::StateId = 0;
        for expr in &cmd.exprs {
            current_state = self.insert_expr(current_state, expr)?;
        }
        self.set_accept(current_state, command_id)?;
        if let Some(doc) = cmd.command_doc() {
            let _ = self.set_command_doc(current_state, doc.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_constructs_concatenated_sequences() {
        let mut builder = CmdBuilder::new();
        builder.literals(&["show", "ip"]).positional_args(2);
        let cmd = builder.build();

        assert_eq!(
            cmd.exprs,
            vec![
                Expr::Sequence(vec![
                    Atom::Literal {
                        token: "show".to_string(),
                        doc: None
                    },
                    Atom::Literal {
                        token: "ip".to_string(),
                        doc: None
                    }
                ]),
                Expr::Sequence(vec![
                    Atom::Var {
                        name: None,
                        doc: None
                    },
                    Atom::Var {
                        name: None,
                        doc: None
                    }
                ]),
            ]
        );
    }

    #[test]
    fn builder_constructs_labeled_arg_sequences() {
        let mut builder = CmdBuilder::new();
        builder
            .literals(&["create", "account"])
            .labeled_arg("name")
            .labeled_arg("currency");
        let cmd = builder.build();

        assert_eq!(
            cmd.exprs,
            vec![
                Expr::Sequence(vec![
                    Atom::Literal {
                        token: "create".to_string(),
                        doc: None
                    },
                    Atom::Literal {
                        token: "account".to_string(),
                        doc: None
                    }
                ]),
                Expr::Sequence(vec![
                    Atom::Literal {
                        token: "name".to_string(),
                        doc: None
                    },
                    Atom::LabeledVar {
                        label: "name".to_string(),
                        doc: None
                    }
                ]),
                Expr::Sequence(vec![
                    Atom::Literal {
                        token: "currency".to_string(),
                        doc: None
                    },
                    Atom::LabeledVar {
                        label: "currency".to_string(),
                        doc: None
                    }
                ]),
            ]
        );
    }

    #[test]
    fn capture_spec_returns_positional_and_labeled_kinds() {
        let mut builder = CmdBuilder::new();
        builder
            .literals(&["set"])
            .positional_args(1)
            .labeled_arg("value");
        let cmd = builder.build();

        assert_eq!(
            cmd.capture_spec().unwrap(),
            vec![
                CaptureKind::Positional {
                    name: None,
                    doc: None
                },
                CaptureKind::Labeled {
                    label: "value".to_string(),
                    doc: None
                }
            ]
        );
    }

    #[test]
    fn builder_supports_integrated_docs() {
        let mut builder = CmdBuilder::new();
        builder
            .literal_with_doc("show", "show things")
            .labeled_arg_with_doc("name", "account name")
            .positional_arg_with_doc("target", "target value")
            .command_doc("run the command");
        let cmd = builder.build();

        assert_eq!(
            cmd.exprs,
            vec![
                Expr::Sequence(vec![Atom::Literal {
                    token: "show".to_string(),
                    doc: Some("show things".to_string())
                }]),
                Expr::Sequence(vec![
                    Atom::Literal {
                        token: "name".to_string(),
                        doc: Some("account name".to_string())
                    },
                    Atom::LabeledVar {
                        label: "name".to_string(),
                        doc: Some("account name".to_string())
                    }
                ]),
                Expr::Sequence(vec![Atom::Var {
                    name: Some("target".to_string()),
                    doc: Some("target value".to_string())
                }]),
            ]
        );
        assert_eq!(cmd.command_doc(), Some("run the command"));
    }

    #[test]
    fn capture_spec_retains_doc_metadata() {
        let mut builder = CmdBuilder::new();
        builder
            .positional_arg_with_doc("iface", "interface name")
            .labeled_arg_with_doc("value", "configured value");
        let cmd = builder.build();

        assert_eq!(
            cmd.capture_spec().unwrap(),
            vec![
                CaptureKind::Positional {
                    name: Some("iface".to_string()),
                    doc: Some("interface name".to_string())
                },
                CaptureKind::Labeled {
                    label: "value".to_string(),
                    doc: Some("configured value".to_string())
                }
            ]
        );
    }

    #[test]
    fn capture_spec_rejects_duplicate_labeled_args() {
        let mut builder = CmdBuilder::new();
        builder
            .literals(&["create", "account"])
            .labeled_arg("name")
            .labeled_arg("name");
        let cmd = builder.build();

        assert_eq!(
            cmd.capture_spec().unwrap_err(),
            CmdSchemaError::DuplicateLabeledArg {
                label: "name".to_string()
            }
        );
    }

    #[test]
    fn insert_cmd_creates_path_and_marks_accept() {
        let mut sm = sm::Sm::new();
        let mut builder = CmdBuilder::new();
        builder.literals(&["show", "ip"]).positional_args(1);
        let cmd = builder.build();

        sm.insert_cmd(&cmd, 10).unwrap();

        let s1 = sm.next_state(0, "show").unwrap();
        let s2 = sm.next_state(s1, "ip").unwrap();
        let s3 = sm.next_state(s2, "eth0").unwrap();
        assert_eq!(sm.accept_at(s3).unwrap(), Some(10));
    }

    #[test]
    fn insert_cmd_reuses_shared_prefix() {
        let mut sm = sm::Sm::new();

        let mut a = CmdBuilder::new();
        a.literals(&["show", "ip", "route"]);
        let cmd_a = a.build();
        sm.insert_cmd(&cmd_a, 1).unwrap();

        let mut b = CmdBuilder::new();
        b.literals(&["show", "ip", "interface"]);
        let cmd_b = b.build();
        sm.insert_cmd(&cmd_b, 2).unwrap();

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

        let mut a = CmdBuilder::new();
        a.literals(&["show", "version"]);
        let cmd_a = a.build();
        sm.insert_cmd(&cmd_a, 1).unwrap();

        let mut b = CmdBuilder::new();
        b.literals(&["show", "version"]);
        let cmd_b = b.build();

        let err = sm.insert_cmd(&cmd_b, 2).unwrap_err();
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

        let mut a = CmdBuilder::new();
        a.literals(&["show"]).positional_args(1);
        let cmd_a = a.build();
        sm.insert_cmd(&cmd_a, 1).unwrap();

        let mut b = CmdBuilder::new();
        b.literals(&["show", "version"]);
        let cmd_b = b.build();
        sm.insert_cmd(&cmd_b, 2).unwrap();

        let show = sm.next_state(0, "show").unwrap();
        let var_state = sm.next_state(show, "eth0").unwrap();
        let ver_state = sm.next_state(show, "version").unwrap();

        assert_eq!(sm.accept_at(var_state).unwrap(), Some(1));
        assert_eq!(sm.accept_at(ver_state).unwrap(), Some(2));
    }

    #[test]
    fn insert_cmd_supports_labeled_args() {
        let mut sm = sm::Sm::new();

        let mut builder = CmdBuilder::new();
        builder
            .literals(&["create", "account"])
            .labeled_arg("name")
            .labeled_arg("currency");
        let cmd = builder.build();

        sm.insert_cmd(&cmd, 7).unwrap();

        let create = sm.next_state(0, "create").unwrap();
        let account = sm.next_state(create, "account").unwrap();
        let name = sm.next_state(account, "name").unwrap();
        let value = sm.next_state(name, "cash").unwrap();
        let currency = sm.next_state(value, "currency").unwrap();
        let usd = sm.next_state(currency, "USD").unwrap();

        assert_eq!(sm.accept_at(usd).unwrap(), Some(7));
    }
}
