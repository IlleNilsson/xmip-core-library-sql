//! The verb a statement opens with, which names its kind: `SELECT`,
//! `INSERT`, `CREATE`. One reading for every SQL crate: the SQL contract
//! holds a script to a set of them, and a SQL transport's far end answers
//! a statement by it. Until 2026-10-12 the transport capability upper-cased
//! a statement's first word and the contract kept its own list of kinds.

/// A statement's verb, named by the keyword it begins with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verb {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
    Create,
    Alter,
    Drop,
    Truncate,
    Grant,
    Revoke,
    Begin,
    Start,
    Commit,
    Rollback,
    Call,
    Values,
    Set,
}

impl Verb {
    /// Every verb, in keyword order.
    pub const ALL: [Verb; 18] = [
        Verb::Select,
        Verb::Insert,
        Verb::Update,
        Verb::Delete,
        Verb::Merge,
        Verb::Create,
        Verb::Alter,
        Verb::Drop,
        Verb::Truncate,
        Verb::Grant,
        Verb::Revoke,
        Verb::Begin,
        Verb::Start,
        Verb::Commit,
        Verb::Rollback,
        Verb::Call,
        Verb::Values,
        Verb::Set,
    ];

    /// The verb the keyword `name` is, without regard to case.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Verb> {
        Verb::ALL
            .into_iter()
            .find(|verb| verb.name().eq_ignore_ascii_case(name))
    }

    /// The verb `statement` opens with: its first word as a keyword, or
    /// `None` where that word is none.
    #[must_use]
    pub fn of_statement(statement: &str) -> Option<Verb> {
        Verb::of_name(statement.split_whitespace().next()?)
    }

    /// The verb's keyword, lower-cased.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Verb::Select => "select",
            Verb::Insert => "insert",
            Verb::Update => "update",
            Verb::Delete => "delete",
            Verb::Merge => "merge",
            Verb::Create => "create",
            Verb::Alter => "alter",
            Verb::Drop => "drop",
            Verb::Truncate => "truncate",
            Verb::Grant => "grant",
            Verb::Revoke => "revoke",
            Verb::Begin => "begin",
            Verb::Start => "start",
            Verb::Commit => "commit",
            Verb::Rollback => "rollback",
            Verb::Call => "call",
            Verb::Values => "values",
            Verb::Set => "set",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_is_read_back_from_its_keyword_in_any_case() {
        for verb in Verb::ALL {
            assert_eq!(Verb::of_name(verb.name()), Some(verb));
            assert_eq!(Verb::of_name(&verb.name().to_ascii_uppercase()), Some(verb));
        }
        assert_eq!(Verb::of_name("explain"), None);
        assert_eq!(Verb::of_name(""), None);
    }

    #[test]
    fn a_statement_is_read_by_its_first_word() {
        assert_eq!(Verb::of_statement("  select 1"), Some(Verb::Select));
        assert_eq!(
            Verb::of_statement("INSERT INTO t (c) VALUES (1)"),
            Some(Verb::Insert)
        );
        assert_eq!(Verb::of_statement("WITH a AS (SELECT 1) SELECT 1"), None);
        assert_eq!(Verb::of_statement("SELECT1"), None);
        assert_eq!(Verb::of_statement(""), None);
    }
}
