//! `MariaDB`: identifiers in backticks with a backtick doubled, or bare with
//! `_ .` in them; `'…'` literals with the backslash escapes the server reads
//! unless `NO_BACKSLASH_ESCAPES` is on; bytes as the hexadecimal literal
//! `X'…'` a BLOB or VARBINARY column stores as the bytes. Over the text
//! protocol a BLOB comes back raw, which is not text, so a SELECT spells it
//! out — `CONCAT('0x', HEX(payload))` — and the answer must be in a hex
//! form, `X'…'` or `0x…`. `?` for a parameter; the INSERT answers no row,
//! and a new row's id is asked for after it.

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::literal::Literal;
use crate::standard;

/// `MariaDB`.
pub const MARIADB: Dialect = Dialect {
    identifier: Delimiter::BACKTICK,
    bare: &['_', '.'],
    marker: "?",
    text,
    bytes: standard::bytes,
    literal,
    binary_column,
    binary,
    returning: Returning::Separately,
};

/// `text` as a string literal: quoted, a quote, a backslash, a NUL and the
/// line ends each written as its escape.
fn text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\0' => out.push_str("\\0"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{1a}' => out.push_str("\\Z"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

/// `CONCAT('0x', HEX(<column>))`: a BLOB spelled out in a hex form.
fn binary_column(column: &str) -> String {
    format!("CONCAT('0x', HEX({column}))")
}

/// The bytes a value in a hex form names — `X'…'`, `x'…'` or `0x…` — or
/// `None` when `text` is not in one.
fn binary(text: &str) -> Option<Vec<u8>> {
    let digits = match text.strip_prefix("X'").or_else(|| text.strip_prefix("x'")) {
        Some(quoted) => quoted.strip_suffix('\'')?,
        None => text.strip_prefix("0x")?,
    };
    hex::decode(digits).ok()
}

/// One literal — `'…'` with backslash escapes, or `X'…'` — and what
/// follows it: the escapes the server would undo, undone.
fn literal(rest: &str) -> Option<(Literal, &str)> {
    if rest.starts_with("X'") || rest.starts_with("x'") {
        let end = rest[2..].find('\'')? + 3;
        let bytes = binary(&rest[..end])?;
        return Some((Literal::Bytes(bytes), &rest[end..]));
    }
    let mut chars = rest.strip_prefix('\'')?.char_indices().peekable();
    let mut value = String::new();
    loop {
        let (at, c) = chars.next()?;
        match c {
            '\'' if chars.peek().is_some_and(|(_, next)| *next == '\'') => {
                chars.next();
                value.push('\'');
            }
            '\'' => return Some((Literal::Text(value), &rest[at + 2..])),
            '\\' => {
                let escaped = chars.next()?.1;
                let unescaped = match escaped {
                    '0' => '\0',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'b' => '\u{8}',
                    'Z' => '\u{1a}',
                    // A LIKE wildcard keeps its backslash, as the server keeps it.
                    '%' | '_' => {
                        value.push('\\');
                        escaped
                    }
                    other => other,
                };
                value.push(unescaped);
            }
            other => value.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Literal {
        Literal::Text(value.into())
    }

    #[test]
    fn bytes_round_trip_through_the_hex_form() {
        let all: Vec<u8> = (0..=255).collect();
        let literal = MARIADB.quote_bytes(&all);
        assert!(literal.starts_with("X'000102"));
        assert!(literal.ends_with("feff'"));
        assert_eq!(MARIADB.bytes_of(&literal), Some(all));
        assert_eq!(MARIADB.quote_bytes(b""), "X''");
        assert_eq!(MARIADB.bytes_of("X''"), Some(Vec::new()));
        assert_eq!(MARIADB.bytes_of("0xFFfe"), Some(vec![0xff, 0xfe]));
        assert_eq!(MARIADB.bytes_of("x'0a'"), Some(vec![0x0a]));
        assert_eq!(MARIADB.bytes_of("plain"), None);
        assert_eq!(MARIADB.bytes_of("X'abc'"), None, "an odd digit count");
        assert_eq!(MARIADB.bytes_of("X'zz'"), None, "not hex");
        assert_eq!(MARIADB.bytes_of("X'ab"), None, "never closed");
        assert_eq!(MARIADB.bytes_of("0x"), Some(Vec::new()));
        assert_eq!(MARIADB.binary_column("bytes"), "CONCAT('0x', HEX(bytes))");
    }

    #[test]
    fn a_literal_escapes_what_the_server_unescapes() {
        assert_eq!(MARIADB.quote_text("it's"), "'it\\'s'");
        assert_eq!(MARIADB.quote_text("back\\slash"), "'back\\\\slash'");
        assert_eq!(MARIADB.quote_text("a\nb\r\0\u{1a}"), "'a\\nb\\r\\0\\Z'");
        assert_eq!(MARIADB.quote_text(""), "''");
        assert_eq!(MARIADB.quote_identifier("in`box"), "`in``box`");
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| MARIADB.parse_insert_literal(sql);
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES ('it\\'s here');"),
            Some(("inbox".into(), "payload".into(), text("it's here")))
        );
        assert_eq!(
            parse("insert into `In box` ( `Payload` ) values ( '' )"),
            Some(("In box".into(), "Payload".into(), text("")))
        );
        assert_eq!(
            parse("INSERT INTO `in``box` (`a`) VALUES ('x')"),
            Some(("in`box".into(), "a".into(), text("x"))),
            "a doubled backtick is one, as quote_identifier writes it"
        );
        assert_eq!(
            parse("INSERT INTO orders.inbox (payload) VALUES ('a\\nb''c\\\\d\\%e')"),
            Some((
                "orders.inbox".into(),
                "payload".into(),
                text("a\nb'c\\d\\%e")
            ))
        );
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES (X'fffe')"),
            Some((
                "inbox".into(),
                "payload".into(),
                Literal::Bytes(vec![0xff, 0xfe])
            ))
        );
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES ('open").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES (X'abc')").is_none());
        assert!(parse("UPDATE inbox SET a = 'x'").is_none());
    }
}
