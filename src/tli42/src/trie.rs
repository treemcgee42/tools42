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
}
