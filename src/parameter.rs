//! A parameter bound in by text: the dialect's first parameter marker in a
//! statement replaced by a literal, where it is a parameter and nowhere
//! else. A SQL Receive Location's `accept` statement names its row this way
//! (`xmip-core-transport`'s `sql::accept`).
//!
//! Where a literal ends is the dialect's to say, so it is read by the
//! dialect's own literal reader — a `MariaDB` `'it\'s ?'` is one literal,
//! not one that ends after the backslash, nor is a `PostgreSQL` `E'…'` or
//! `$$…$$` or an Oracle `q'[…]'` — and a comment, `--` and the
//! dialect's other line comments or `/* … */`, binds nothing. A literal,
//! quoted identifier or comment that never closes keeps the rest of the
//! statement as written: nothing is bound where the server reads text.

use crate::dialect::Dialect;

impl Dialect {
    /// `statement` with every [`Dialect::marker`] outside its string
    /// literals, quoted identifiers and comments replaced by `literal`; a
    /// marker followed by a digit (`$10` for `$1`) is another parameter and
    /// stays.
    #[must_use]
    pub fn bound(&self, statement: &str, literal: &str) -> String {
        let mut written = String::with_capacity(statement.len() + literal.len());
        let mut rest = statement;
        // Whether the text so far ends inside a word: a literal's prefix
        // (`E'`, `q'`, `$$`) opens one only at a word's start.
        let mut word = false;
        while let Some(next) = rest.chars().next() {
            if let Some(after) = self.unbound(rest, word) {
                written.push_str(&rest[..rest.len() - after.len()]);
                rest = after;
                word = false;
                continue;
            }
            if let Some(after) = rest.strip_prefix(self.marker)
                && !after.starts_with(|c: char| c.is_ascii_digit())
            {
                written.push_str(literal);
                rest = after;
                word = false;
                continue;
            }
            written.push(next);
            rest = &rest[next.len_utf8()..];
            word = next.is_alphanumeric() || matches!(next, '_' | '$');
        }
        written
    }

    /// What follows the run `rest` opens with where nothing is bound — a
    /// comment, a quoted identifier, a literal — and `""` where that run
    /// never closes or its literal is refused; `None` where `rest` opens
    /// with none. A quote always opens a literal; a prefix only where `word`
    /// says the text is not inside a word.
    fn unbound<'a>(&self, rest: &'a str, word: bool) -> Option<&'a str> {
        if self.line_comments.iter().any(|open| rest.starts_with(open)) {
            return Some(rest.find('\n').map_or("", |end| &rest[end..]));
        }
        if let Some(body) = rest.strip_prefix("/*") {
            return Some(body.find("*/").map_or("", |end| &body[end + 2..]));
        }
        let next = rest.chars().next()?;
        if next == self.identifier.open {
            let read = self.identifier.unquote_prefix(rest);
            return Some(read.map_or("", |(_, after)| after));
        }
        let quoted = matches!(next, '\'' | '"');
        ((quoted || !word) && (self.opens)(rest))
            .then(|| self.read_literal(rest).map_or("", |(_, after)| after))
    }
}

#[cfg(test)]
mod tests {
    use crate::{DB2, MARIADB, MARIADB_NO_BACKSLASH_ESCAPES, ORACLE, POSTGRESQL, TSQL};

    #[test]
    fn the_marker_is_replaced_by_the_literal_where_it_is_a_parameter() {
        assert_eq!(
            POSTGRESQL.bound("DELETE FROM inbox WHERE id = $1", "'41'"),
            "DELETE FROM inbox WHERE id = '41'"
        );
        assert_eq!(
            POSTGRESQL.bound("UPDATE inbox SET done = $1 WHERE id = $1", "'x'"),
            "UPDATE inbox SET done = 'x' WHERE id = 'x'"
        );
        assert_eq!(
            MARIADB.bound("DELETE FROM t WHERE id = ? AND note <> '?'", "'7'"),
            "DELETE FROM t WHERE id = '7' AND note <> '?'"
        );
        assert_eq!(
            DB2.bound("UPDATE t SET a = 'x' || ? WHERE id = ?", "'7'"),
            "UPDATE t SET a = 'x' || '7' WHERE id = '7'"
        );
    }

    #[test]
    fn a_marker_in_a_literal_a_quoted_name_or_a_longer_parameter_stays() {
        assert_eq!(
            POSTGRESQL.bound(
                "UPDATE \"t$1\" SET note = 'cost $1', n = $10 WHERE id = $1",
                "'7'"
            ),
            "UPDATE \"t$1\" SET note = 'cost $1', n = $10 WHERE id = '7'"
        );
        assert_eq!(
            POSTGRESQL.bound("DELETE FROM inbox", "'7'"),
            "DELETE FROM inbox"
        );
    }

    #[test]
    fn a_backslash_escaped_quote_does_not_end_the_literal_early() {
        // Read by doubling alone, the literal ended after `it\'`, the `?`
        // inside it was bound, and the bound quotes ran the name as SQL.
        let statement = "UPDATE t SET note = 'it\\'s ?' WHERE id = ?";
        let hostile = MARIADB.quote_text("x' OR '1'='1");
        assert_eq!(
            MARIADB.bound(statement, &hostile),
            format!("UPDATE t SET note = 'it\\'s ?' WHERE id = {hostile}")
        );
        assert_eq!(
            MARIADB.bound("SET a = \"?\\\"?\" WHERE id = ?", "'7'"),
            "SET a = \"?\\\"?\" WHERE id = '7'"
        );
        assert_eq!(
            MARIADB_NO_BACKSLASH_ESCAPES.bound("SET a = 'c:\\' WHERE id = ?", "'7'"),
            "SET a = 'c:\\' WHERE id = '7'",
            "without the escapes a backslash ends nothing"
        );
    }

    #[test]
    fn a_marker_in_a_comment_stays_and_a_quote_there_opens_nothing() {
        assert_eq!(
            MARIADB.bound("DELETE FROM t -- don't ?\nWHERE id = ? # it's ?", "'7'"),
            "DELETE FROM t -- don't ?\nWHERE id = '7' # it's ?"
        );
        assert_eq!(
            TSQL.bound("DELETE /* it's @P1 */ FROM t WHERE id = @P1", "N'7'"),
            "DELETE /* it's @P1 */ FROM t WHERE id = N'7'"
        );
        assert_eq!(
            ORACLE.bound("DELETE FROM t WHERE id = :1 # :1", "'7'"),
            "DELETE FROM t WHERE id = '7' # '7'",
            "a # is a comment only where the dialect says"
        );
    }

    #[test]
    fn what_never_closes_keeps_the_rest_unbound() {
        for statement in [
            "DELETE FROM t WHERE note = 'open ? ",
            "DELETE FROM t /* open ? ",
            "DELETE FROM `open ? ",
        ] {
            assert_eq!(MARIADB.bound(statement, "'7'"), statement);
        }
    }
}
