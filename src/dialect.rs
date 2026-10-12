//! What one SQL server writes its own way, as one value: [`Dialect`].
//!
//! A dialect is a choice, not a copy (ADR-0044, amendment 2026-09-24): the
//! doubling of a delimiter, the shape of an INSERT and the binding of a
//! parameter are the same everywhere and are written once, here; what a
//! dialect names is which delimiter, which marker, how a literal and a
//! binary value are spelled, and how an INSERT answers its new row's id.
//! Until 2026-10-12 the transport capability and the archive capability
//! each had a dialect of their own, and every SQL technology filled both.
//!
//! The dialects Xmip speaks are this crate's constants: [`crate::STANDARD`],
//! [`crate::TSQL`], [`crate::POSTGRESQL`], [`crate::MARIADB`] and
//! [`crate::SQLITE`]. What a server's wire is — its protocol, its login,
//! its URI scheme — stays with the technology that speaks it.

use crate::delimiter::Delimiter;
use crate::literal::Literal;

/// What one SQL server writes its own way. Each is a constant of this
/// crate; nothing tells one dialect from another by name.
#[derive(Clone, Copy, Debug)]
pub struct Dialect {
    /// The delimiters a quoted identifier is written between.
    pub(crate) identifier: Delimiter,
    /// What a bare identifier takes besides letters and digits.
    pub(crate) bare: &'static [char],
    /// The first parameter as a statement writes it: `$1`, `?`, `@P1`.
    pub(crate) marker: &'static str,
    /// A text value as a string literal.
    pub(crate) text: fn(&str) -> String,
    /// Bytes as the literal a binary column stores as the bytes.
    pub(crate) bytes: fn(&[u8]) -> String,
    /// One literal at the start of a statement's text, and what follows.
    pub(crate) literal: fn(&str) -> Option<(Literal, &str)>,
    /// The expression a SELECT names a binary column by, so the server
    /// answers it as text in the form `binary` reads.
    pub(crate) binary_column: fn(&str) -> String,
    /// The bytes a binary column's answer names, or `None` where it is
    /// not in the dialect's binary form.
    pub(crate) binary: fn(&str) -> Option<Vec<u8>>,
    /// How an INSERT answers its new row's id.
    pub(crate) returning: Returning,
}

/// How an INSERT answers the id of the row it made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Returning {
    /// T-SQL's `OUTPUT INSERTED.<column>`, between the columns and `VALUES`.
    Output,
    /// `RETURNING <column>`, after the values.
    Returning,
    /// The INSERT answers no row; the id is asked for after it.
    Separately,
}

impl Returning {
    /// What an INSERT says between its columns and `VALUES` to answer
    /// `column` of its new row.
    #[must_use]
    pub fn before_values(self, column: &str) -> String {
        match self {
            Self::Output => format!(" OUTPUT INSERTED.{column}"),
            Self::Returning | Self::Separately => String::new(),
        }
    }

    /// What an INSERT says after its values to answer `column` of its new
    /// row.
    #[must_use]
    pub fn after_values(self, column: &str) -> String {
        match self {
            Self::Returning => format!(" RETURNING {column}"),
            Self::Output | Self::Separately => String::new(),
        }
    }
}

impl Dialect {
    /// The dialect's first parameter as a statement writes it.
    #[must_use]
    pub const fn marker(&self) -> &'static str {
        self.marker
    }

    /// How an INSERT answers its new row's id.
    #[must_use]
    pub const fn returning(&self) -> Returning {
        self.returning
    }

    /// `name` as one quoted identifier, every closing delimiter doubled,
    /// so whatever a target names is one identifier and nothing else.
    #[must_use]
    pub fn quote_identifier(&self, name: &str) -> String {
        self.identifier.quote(name)
    }

    /// A table name quoted segment by segment, so `audit.archive` stays a
    /// table in a schema and `Archive` keeps its case.
    #[must_use]
    pub fn quote_table(&self, table: &str) -> String {
        table
            .split('.')
            .map(|segment| self.quote_identifier(segment))
            .collect::<Vec<_>>()
            .join(".")
    }

    /// `text` as this dialect's string literal.
    #[must_use]
    pub fn quote_text(&self, text: &str) -> String {
        (self.text)(text)
    }

    /// `bytes` as the literal this dialect's binary column stores as them.
    #[must_use]
    pub fn quote_bytes(&self, bytes: &[u8]) -> String {
        (self.bytes)(bytes)
    }

    /// One literal at the start of `rest`, as this dialect writes it, and
    /// what follows it; `None` where `rest` opens with none.
    #[must_use]
    pub fn read_literal<'a>(&self, rest: &'a str) -> Option<(Literal, &'a str)> {
        (self.literal)(rest)
    }

    /// The expression a SELECT names the binary column `column` by, so the
    /// answer is text [`Dialect::bytes_of`] reads.
    #[must_use]
    pub fn binary_column(&self, column: &str) -> String {
        (self.binary_column)(column)
    }

    /// The bytes a binary column's answer names, or `None` where `text` is
    /// not in this dialect's binary form: nothing else is taken for bytes
    /// (ADR-0038).
    #[must_use]
    pub fn bytes_of(&self, text: &str) -> Option<Vec<u8>> {
        (self.binary)(text)
    }

    /// `INSERT INTO <table> (<column>) VALUES (<value>)`, the table and
    /// column quoted. `value` is what goes in `VALUES`: a bind marker, or a
    /// literal this dialect wrote; never text taken from a target.
    #[must_use]
    pub fn insert(&self, table: &str, column: &str, value: &str) -> String {
        format!(
            "INSERT INTO {} ({}) VALUES ({value})",
            self.quote_identifier(table),
            self.quote_identifier(column)
        )
    }

    /// One identifier, bare or quoted with its closing delimiter doubled,
    /// and what follows it.
    fn read_identifier<'a>(&self, rest: &'a str) -> Option<(String, &'a str)> {
        let rest = rest.trim_start();
        if rest.starts_with(self.identifier.open) {
            return self.identifier.unquote_prefix(rest).ok();
        }
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || self.bare.contains(&c)))
            .unwrap_or(rest.len());
        (end > 0).then(|| (rest[..end].to_string(), &rest[end..]))
    }

    /// `INSERT INTO <table> (<column>) VALUES (<value>)` taken apart: the
    /// table, the column and the value. Identifiers are read as this
    /// dialect writes them and `value` reads one value, returning what
    /// follows it; anything else, or more than one column, is `None`.
    #[must_use]
    pub fn parse_insert<'a, V>(
        &self,
        sql: &'a str,
        value: impl Fn(&'a str) -> Option<(V, &'a str)>,
    ) -> Option<(String, String, V)> {
        let rest = sql.trim().trim_end_matches(';');
        let rest = strip_word(rest, "INSERT")?;
        let rest = strip_word(rest, "INTO")?;
        let (table, rest) = self.read_identifier(rest)?;
        let rest = rest.trim_start().strip_prefix('(')?;
        let (column, rest) = self.read_identifier(rest)?;
        let rest = rest.trim_start().strip_prefix(')')?;
        let rest = strip_word(rest, "VALUES")?;
        let rest = rest.trim_start().strip_prefix('(')?.trim_start();
        let (value, tail) = value(rest)?;
        (tail.trim() == ")").then_some((table, column, value))
    }

    /// An INSERT of one literal taken apart as [`Dialect::parse_insert`]
    /// does, the literal read as this dialect writes one.
    #[must_use]
    pub fn parse_insert_literal(&self, sql: &str) -> Option<(String, String, Literal)> {
        self.parse_insert(sql, |rest| self.read_literal(rest))
    }
}

/// `rest` after `word`, matched without regard to case; `None` where
/// `rest` does not open with it.
fn strip_word<'a>(rest: &'a str, word: &str) -> Option<&'a str> {
    let rest = rest.trim_start();
    let head = rest.get(..word.len())?;
    head.eq_ignore_ascii_case(word).then(|| &rest[word.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TSQL;

    fn quoted(rest: &str) -> Option<(String, &str)> {
        Delimiter::STRING.unquote_prefix(rest).ok()
    }

    #[test]
    fn an_insert_quotes_what_a_target_names_so_it_stays_one_identifier() {
        fn marker(rest: &str) -> Option<((), &str)> {
            rest.strip_prefix(":1").map(|tail| ((), tail))
        }
        let sql = TSQL.insert("in]box; DROP TABLE x --", "pay load", ":1");
        assert_eq!(
            sql,
            "INSERT INTO [in]]box; DROP TABLE x --] ([pay load]) VALUES (:1)"
        );
        assert_eq!(
            TSQL.parse_insert(&sql, marker),
            Some(("in]box; DROP TABLE x --".into(), "pay load".into(), ()))
        );
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart_with_the_value_handed_in() {
        assert_eq!(
            TSQL.parse_insert("insert into inbox ( payload ) values ( 'x' );", quoted),
            Some(("inbox".into(), "payload".into(), "x".into()))
        );
        assert_eq!(
            TSQL.parse_insert("INSERT INTO dbo.inbox ([a]) VALUES ('x')", quoted),
            Some(("dbo.inbox".into(), "a".into(), "x".into()))
        );
        let parse = |sql| TSQL.parse_insert(sql, quoted);
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES ('open").is_none());
        assert!(parse("INSERT INTO [open (a) VALUES ('x')").is_none());
        assert!(parse("UPDATE inbox SET a = 'x'").is_none());
        assert_eq!(strip_word("  Values (", "VALUES"), Some(" ("));
        assert_eq!(strip_word("VAL", "VALUES"), None);
    }

    #[test]
    fn a_table_is_quoted_by_segment_and_the_id_is_asked_where_the_dialect_can() {
        assert_eq!(TSQL.quote_table("audit.archive"), "[audit].[archive]");
        assert_eq!(Returning::Output.before_values("id"), " OUTPUT INSERTED.id");
        assert_eq!(Returning::Output.after_values("id"), "");
        assert_eq!(Returning::Returning.before_values("id"), "");
        assert_eq!(Returning::Returning.after_values("id"), " RETURNING id");
        assert_eq!(Returning::Separately.before_values("id"), "");
        assert_eq!(Returning::Separately.after_values("id"), "");
    }
}
