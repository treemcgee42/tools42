use std::collections::HashMap;

type InternedStringType = u32;

struct  StringInterner {
    next_interned_value: InternedStringType,
    string_to_interned_value: HashMap<String, InternedStringType>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            next_interned_value: 0,
            string_to_interned_value: HashMap::new(),
        }
    }

    pub fn intern(&mut self, s: &str) -> InternedStringType {
        if let Some(interned_value) = self.string_to_interned_value.get(s) {
            *interned_value
        } else {
            self.string_to_interned_value.insert(s.to_string(), self.next_interned_value);
            self.next_interned_value += 1;
            self.next_interned_value - 1
        }
    }

    pub fn get_interned(&self, s: &str) -> Option<InternedStringType> {
        self.string_to_interned_value.get(s).copied()
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
        assert_eq!(interner.next_interned_value, 1);
        assert_eq!(interner.string_to_interned_value.len(), 1);
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
        assert_eq!(interner.next_interned_value, 3);
        assert_eq!(interner.string_to_interned_value.len(), 3);
    }

    #[test]
    fn interning_existing_string_does_not_advance_counter() {
        let mut interner = StringInterner::new();
        let first = interner.intern("repeat");
        let after_first = interner.next_interned_value;
        let second = interner.intern("repeat");

        assert_eq!(first, second);
        assert_eq!(after_first, 1);
        assert_eq!(interner.next_interned_value, after_first);
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
        assert_eq!(interner.next_interned_value, 3);
    }

    #[test]
    fn get_interned_returns_none_for_missing_key() {
        let mut interner = StringInterner::new();
        interner.intern("known");

        assert_eq!(interner.get_interned("known"), Some(0));
        assert_eq!(interner.get_interned("unknown"), None);
    }
}

#[cfg(test)]
mod trie_tests {
    use super::*;

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
}
