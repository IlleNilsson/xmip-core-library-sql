//! A parameter bound in by text: the dialect's first parameter marker in a
//! statement replaced by a literal, where it is a parameter and nowhere
//! else. A SQL Receive Location's `accept` statement names its row this way
//! (`xmip-core-transport`'s `sql::accept`).

use crate::delimiter::Delimiter;
use crate::dialect::Dialect;

impl Dialect {
    /// `statement` with every [`Dialect::marker`] outside its string
    /// literals and quoted identifiers replaced by `literal`; a marker
    /// followed by a digit (`$10` for `$1`) is another parameter and stays.
    #[must_use]
    pub fn bound(&self, statement: &str, literal: &str) -> String {
        let mut written = String::with_capacity(statement.len() + literal.len());
        let mut rest = statement;
        while let Some(next) = rest.chars().next() {
            let quoted = [Delimiter::STRING, self.identifier]
                .into_iter()
                .find(|delimiter| delimiter.open == next)
                .and_then(|delimiter| delimiter.unquote_prefix(rest).ok());
            if let Some((_, after)) = quoted {
                written.push_str(&rest[..rest.len() - after.len()]);
                rest = after;
                continue;
            }
            if let Some(after) = rest.strip_prefix(self.marker)
                && !after.starts_with(|c: char| c.is_ascii_digit())
            {
                written.push_str(literal);
                rest = after;
                continue;
            }
            written.push(next);
            rest = &rest[next.len_utf8()..];
        }
        written
    }
}

#[cfg(test)]
mod tests {
    use crate::{MARIADB, POSTGRESQL};

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
}
