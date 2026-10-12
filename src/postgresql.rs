//! `PostgreSQL`: `"…"` identifiers, their case kept; `'…'` literals with
//! the quote doubled and backslashes literal, as
//! `standard_conforming_strings` has had them since 9.1; bytes in the bytea
//! hex form, `\x` and two digits a byte, which a bytea column reads as the
//! bytes and answers under the default `bytea_output`; `$1` for a
//! parameter; a new row's id through `RETURNING`.

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::literal::Literal;
use crate::standard;

/// `PostgreSQL`.
pub const POSTGRESQL: Dialect = Dialect {
    identifier: Delimiter::IDENTIFIER,
    bare: &['_', '.'],
    marker: "$1",
    text: standard::text,
    bytes,
    literal,
    binary_column: str::to_string,
    binary,
    returning: Returning::Returning,
};

/// `bytes` in the bytea hex form, as a string literal: `'\x6bff'`.
fn bytes(bytes: &[u8]) -> String {
    standard::text(&format!("\\x{}", hex::encode(bytes)))
}

/// The bytes a value in the bytea hex form names, or `None` where `text` is
/// not in that form.
fn binary(text: &str) -> Option<Vec<u8>> {
    hex::decode(text.strip_prefix("\\x")?).ok()
}

/// One `'…'` literal, its quotes undoubled, and what follows it: the
/// literal is text, and a bytea value in it is read by [`Dialect::bytes_of`].
fn literal(rest: &str) -> Option<(Literal, &str)> {
    let (text, after) = Delimiter::STRING.unquote_prefix(rest).ok()?;
    Some((Literal::Text(text), after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_through_the_bytea_hex_form() {
        let all: Vec<u8> = (0..=255).collect();
        let literal = POSTGRESQL.quote_bytes(&all);
        assert!(literal.starts_with("'\\x000102"));
        let Some((Literal::Text(value), "")) = POSTGRESQL.read_literal(&literal) else {
            panic!("a string literal");
        };
        assert_eq!(POSTGRESQL.bytes_of(&value), Some(all));
        assert_eq!(POSTGRESQL.quote_bytes(b""), "'\\x'");
        assert_eq!(POSTGRESQL.bytes_of("\\x"), Some(Vec::new()));
        assert_eq!(POSTGRESQL.bytes_of("plain"), None);
        assert_eq!(POSTGRESQL.bytes_of("\\xabc"), None, "an odd digit count");
        assert_eq!(POSTGRESQL.bytes_of("\\xzz"), None, "not hex");
    }

    #[test]
    fn quoting_doubles_the_delimiter_and_nothing_else() {
        assert_eq!(POSTGRESQL.quote_text("it's"), "'it''s'");
        assert_eq!(POSTGRESQL.quote_text("back\\slash"), "'back\\slash'");
        assert_eq!(POSTGRESQL.quote_text(""), "''");
        assert_eq!(POSTGRESQL.quote_identifier("in\"box"), "\"in\"\"box\"");
        assert_eq!(POSTGRESQL.quote_identifier("Inbox"), "\"Inbox\"");
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| POSTGRESQL.parse_insert_literal(sql);
        let text = |value: &str| Literal::Text(value.into());
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES ('it''s here');"),
            Some(("inbox".into(), "payload".into(), text("it's here")))
        );
        assert_eq!(
            parse("insert into \"In box\" ( \"Payload\" ) values ( '' )"),
            Some(("In box".into(), "Payload".into(), text("")))
        );
        assert_eq!(
            parse("INSERT INTO \"in\"\"box\" (\"a\") VALUES ('x')"),
            Some(("in\"box".into(), "a".into(), text("x"))),
            "a doubled quote is one, as quote_identifier writes it"
        );
        assert_eq!(
            parse("INSERT INTO public.inbox (payload) VALUES ('a\nb')"),
            Some(("public.inbox".into(), "payload".into(), text("a\nb")))
        );
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES ('open").is_none());
        assert!(parse("UPDATE inbox SET a = 'x'").is_none());
    }
}
