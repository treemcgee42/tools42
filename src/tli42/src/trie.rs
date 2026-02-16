use std::collections::{HashMap, hash_map};

type InternedStringType = u32;

struct  StringInterner {
    string_to_interned_value: HashMap<String, InternedStringType>,
    interned_value_to_string: Vec<String>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            string_to_interned_value: HashMap::new(),
            interned_value_to_string: Vec::new(),
        }
    }

    pub fn intern(&mut self, s: &str) -> InternedStringType {
        if let Some(interned_value) = self.string_to_interned_value.get(s) {
            *interned_value
        } else {
            let interned_value = self.interned_value_to_string.len() as InternedStringType;
            self.interned_value_to_string.push(s.to_string());
            self.string_to_interned_value
                .insert(s.to_string(), interned_value);
            interned_value
        }
    }

    pub fn get_interned(&self, s: &str) -> Option<InternedStringType> {
        self.string_to_interned_value.get(s).copied()
    }

    pub fn resolve(&self, id: InternedStringType) -> Option<&str> {
        self.interned_value_to_string
            .get(id as usize)
            .map(String::as_str)
    }
}

type TrieNodeEdge = InternedStringType;
type TrieNodeValue = u32;
type TrieNodeIdx = usize;

struct TrieNode {
    value: Option<TrieNodeValue>,
    children: HashMap<TrieNodeEdge, TrieNodeIdx>,
}

struct Trie {
    string_interner: StringInterner,
    nodes: Vec<TrieNode>,
    root: TrieNode,
}

struct Completions<'a> {
    partial: &'a str,
    iter: Option<hash_map::Iter<'a, TrieNodeEdge, TrieNodeIdx>>,
    nodes: &'a [TrieNode],
    interner: &'a StringInterner,
}

impl<'a> Completions<'a> {
    fn empty(partial: &'a str, nodes: &'a [TrieNode], interner: &'a StringInterner) -> Self {
        Self {
            partial,
            iter: None,
            nodes,
            interner,
        }
    }
}

impl<'a> Iterator for Completions<'a> {
    type Item = (&'a str, Option<TrieNodeValue>);

    fn next(&mut self) -> Option<Self::Item> {
        let iter = self.iter.as_mut()?;
        while let Some((edge, child_idx)) = iter.next() {
            let token = self.interner.resolve(*edge)?;
            if token.starts_with(self.partial) {
                return Some((token, self.nodes[*child_idx].value));
            }
        }
        None
    }
}

impl Trie {
    pub fn new() -> Self {
        Trie {
            string_interner: StringInterner::new(),
            nodes: Vec::new(),
            root: TrieNode { value: None, children: HashMap::new() },
        }
    }

    pub fn add_string(&mut self, s: &str, value: TrieNodeValue) {
        let mut current_idx: Option<TrieNodeIdx> = None;

        for token in s.split_whitespace() {
            let interned_token = self.string_interner.intern(token);
            let existing_child = match current_idx {
                None => self.root.children.get(&interned_token).copied(),
                Some(node_idx) => self.nodes[node_idx].children.get(&interned_token).copied(),
            };

            if let Some(child_idx) = existing_child {
                current_idx = Some(child_idx);
                continue;
            }

            let new_idx = self.nodes.len();
            self.nodes.push(TrieNode {
                value: None,
                children: HashMap::new(),
            });

            match current_idx {
                None => {
                    self.root.children.insert(interned_token, new_idx);
                }
                Some(node_idx) => {
                    self.nodes[node_idx].children.insert(interned_token, new_idx);
                }
            }

            current_idx = Some(new_idx);
        }

        match current_idx {
            None => {
                self.root.value = Some(value);
            }
            Some(node_idx) => {
                self.nodes[node_idx].value = Some(value);
            }
        }
    }

    pub fn get(&self, s: &str) -> Option<TrieNodeValue> {
        let mut current_idx: Option<TrieNodeIdx> = None;

        for token in s.split_whitespace() {
            let edge = self.string_interner.get_interned(token)?;
            current_idx = match current_idx {
                None => self.root.children.get(&edge).copied(),
                Some(node_idx) => self.nodes[node_idx].children.get(&edge).copied(),
            };

            if current_idx.is_none() {
                return None;
            }
        }

        match current_idx {
            None => self.root.value,
            Some(node_idx) => self.nodes[node_idx].value,
        }
    }

    pub fn get_completions<'a>(&'a self, s: &'a str) -> Completions<'a> {
        let ends_with_whitespace = s.chars().last().is_some_and(char::is_whitespace);
        let mut tokens = s.split_whitespace().collect::<Vec<_>>();

        let partial = if ends_with_whitespace {
            ""
        } else {
            tokens.pop().unwrap_or("")
        };
        let exact_tokens = tokens;

        let mut current_idx: Option<TrieNodeIdx> = None;
        for token in exact_tokens {
            let edge = match self.string_interner.get_interned(token) {
                Some(edge) => edge,
                None => return Completions::empty(partial, &self.nodes, &self.string_interner),
            };

            current_idx = match current_idx {
                None => self.root.children.get(&edge).copied(),
                Some(node_idx) => self.nodes[node_idx].children.get(&edge).copied(),
            };

            if current_idx.is_none() {
                return Completions::empty(partial, &self.nodes, &self.string_interner);
            }
        }

        let children = match current_idx {
            None => &self.root.children,
            Some(node_idx) => &self.nodes[node_idx].children,
        };

        Completions {
            partial,
            iter: Some(children.iter()),
            nodes: &self.nodes,
            interner: &self.string_interner,
        }
    }
}

#[cfg(test)]
mod string_interner_tests {
    use super::*;

    #[test]
    fn interns_same_string_to_same_id() {
        let mut interner = StringInterner::new();
        let first = interner.intern("alpha");
        let second = interner.intern("alpha");

        assert_eq!(first, second);
        assert_eq!(interner.string_to_interned_value.len(), 1);
        assert_eq!(interner.interned_value_to_string.len(), 1);
    }

    #[test]
    fn assigns_incrementing_ids_for_new_strings() {
        let mut interner = StringInterner::new();
        let alpha = interner.intern("alpha");
        let beta = interner.intern("beta");
        let gamma = interner.intern("gamma");

        assert_eq!(alpha, 0);
        assert_eq!(beta, 1);
        assert_eq!(gamma, 2);
        assert_eq!(interner.string_to_interned_value.len(), 3);
        assert_eq!(interner.interned_value_to_string.len(), 3);
    }

    #[test]
    fn interning_existing_string_does_not_advance_counter() {
        let mut interner = StringInterner::new();
        let first = interner.intern("repeat");
        let after_first = interner.interned_value_to_string.len();
        let second = interner.intern("repeat");

        assert_eq!(first, second);
        assert_eq!(after_first, 1);
        assert_eq!(interner.interned_value_to_string.len(), after_first);
    }

    #[test]
    fn treats_whitespace_variants_as_distinct_keys() {
        let mut interner = StringInterner::new();
        let plain = interner.intern("token");
        let padded = interner.intern(" token ");
        let with_newline = interner.intern("token\n");

        assert_ne!(plain, padded);
        assert_ne!(plain, with_newline);
        assert_ne!(padded, with_newline);
        assert_eq!(interner.interned_value_to_string.len(), 3);
    }

    #[test]
    fn get_interned_returns_none_for_missing_key() {
        let mut interner = StringInterner::new();
        interner.intern("known");

        assert_eq!(interner.get_interned("known"), Some(0));
        assert_eq!(interner.get_interned("unknown"), None);
    }

    #[test]
    fn resolve_returns_original_string_for_valid_id() {
        let mut interner = StringInterner::new();
        let alpha = interner.intern("alpha");
        let beta = interner.intern("beta");

        assert_eq!(interner.resolve(alpha), Some("alpha"));
        assert_eq!(interner.resolve(beta), Some("beta"));
    }

    #[test]
    fn resolve_returns_none_for_unknown_id() {
        let mut interner = StringInterner::new();
        interner.intern("alpha");

        assert_eq!(interner.resolve(1), None);
        assert_eq!(interner.resolve(42), None);
    }

    #[test]
    fn intern_get_and_resolve_are_consistent() {
        let mut interner = StringInterner::new();
        interner.intern("alpha");
        interner.intern("beta");
        interner.intern("gamma");

        for token in ["alpha", "beta", "gamma"] {
            let id = interner
                .get_interned(token)
                .expect("token should have been interned");
            assert_eq!(interner.resolve(id), Some(token));
        }
    }
}

#[cfg(test)]
mod trie_tests {
    use super::*;

    fn sorted_completions(trie: &Trie, input: &str) -> Vec<(String, Option<TrieNodeValue>)> {
        let mut results = trie
            .get_completions(input)
            .map(|(token, value)| (token.to_string(), value))
            .collect::<Vec<_>>();
        results.sort_by(|a, b| a.0.cmp(&b.0));
        results
    }

    #[test]
    fn get_returns_inserted_single_token_value() {
        let mut trie = Trie::new();
        trie.add_string("foo", 1);
        assert_eq!(trie.get("foo"), Some(1));
    }

    #[test]
    fn get_returns_none_for_missing_key() {
        let mut trie = Trie::new();
        trie.add_string("foo bar", 1);

        assert_eq!(trie.get("foo baz"), None);
        assert_eq!(trie.get("unknown"), None);
    }

    #[test]
    fn get_handles_shared_prefix_paths() {
        let mut trie = Trie::new();
        trie.add_string("foo bar", 10);
        trie.add_string("foo baz", 20);

        assert_eq!(trie.get("foo bar"), Some(10));
        assert_eq!(trie.get("foo baz"), Some(20));
        assert_eq!(trie.get("foo"), None);
    }

    #[test]
    fn get_keeps_values_for_prefix_and_longer_path() {
        let mut trie = Trie::new();
        trie.add_string("foo", 7);
        trie.add_string("foo bar", 8);

        assert_eq!(trie.get("foo"), Some(7));
        assert_eq!(trie.get("foo bar"), Some(8));
    }

    #[test]
    fn get_reflects_overwritten_value() {
        let mut trie = Trie::new();
        trie.add_string("foo bar", 3);
        trie.add_string("foo bar", 9);

        assert_eq!(trie.get("foo bar"), Some(9));
    }

    #[test]
    fn get_uses_root_value_for_empty_or_whitespace_input() {
        let mut trie = Trie::new();
        trie.add_string("", 42);
        assert_eq!(trie.get(""), Some(42));
        assert_eq!(trie.get("   \n\t"), Some(42));

        trie.add_string(" ", 99);
        assert_eq!(trie.get(""), Some(99));
    }

    #[test]
    fn get_completions_matches_partial_last_token() {
        let mut trie = Trie::new();
        trie.add_string("foo bar", 1);
        trie.add_string("foo baz", 2);
        trie.add_string("foo qux", 3);

        let got = sorted_completions(&trie, "foo ba");
        assert_eq!(
            got,
            vec![("bar".to_string(), Some(1)), ("baz".to_string(), Some(2))]
        );
    }

    #[test]
    fn get_completions_with_trailing_whitespace_returns_all_next_tokens() {
        let mut trie = Trie::new();
        trie.add_string("foo bar", 1);
        trie.add_string("foo baz", 2);
        trie.add_string("foo qux", 3);

        let got = sorted_completions(&trie, "foo ");
        assert_eq!(
            got,
            vec![
                ("bar".to_string(), Some(1)),
                ("baz".to_string(), Some(2)),
                ("qux".to_string(), Some(3))
            ]
        );
    }

    #[test]
    fn get_completions_returns_empty_when_exact_prefix_path_missing() {
        let mut trie = Trie::new();
        trie.add_string("foo bar", 1);

        let got = sorted_completions(&trie, "unknown ba");
        assert!(got.is_empty());
    }

    #[test]
    fn get_completions_from_root_for_single_partial_token() {
        let mut trie = Trie::new();
        trie.add_string("alpha one", 1);
        trie.add_string("beta two", 2);
        trie.add_string("alphabet three", 3);

        let got = sorted_completions(&trie, "alp");
        assert_eq!(
            got,
            vec![
                ("alpha".to_string(), None),
                ("alphabet".to_string(), None)
            ]
        );
    }
}
