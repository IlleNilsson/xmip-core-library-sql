//! T-SQL, as SQL Server reads it: bracketed identifiers with `]]` for a
//! bracket, or bare with `_ . # @` in them; `N'…'` literals, `N` because a
//! plain literal is the server's single-byte collation and a Stream is
//! UTF-8; bytes as the binary literal `0x…` T-SQL itself writes, which a
//! varbinary column stores as the bytes and answers in the same form;
//! `@P1` for a parameter; a new row's id through `OUTPUT INSERTED`.

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::literal::Literal;

/// T-SQL.
pub const TSQL: Dialect = Dialect {
    identifier: Delimiter::BRACKET,
    bare: &['_', '.', '#', '@'],
    marker: "@P1",
    text,
    bytes,
    literal,
    binary_column: str::to_string,
    binary,
    returning: Returning::Output,
};

/// `text` as a Unicode string literal: `N`, quoted, every quote doubled.
fn text(text: &str) -> String {
    format!("N{}", Delimiter::STRING.quote(text))
}

/// `bytes` as the binary literal: `0x` then two lower-case digits a byte.
fn bytes(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// The bytes a binary literal names, or `None` when `text` is not one.
fn binary(text: &str) -> Option<Vec<u8>> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;
    hex::decode(digits).ok()
}

/// One literal — `N'…'`, `'…'` or `0x…` — and what follows it.
fn literal(rest: &str) -> Option<(Literal, &str)> {
    let rest = rest.trim_start();
    if rest.starts_with("0x") || rest.starts_with("0X") {
        let digits = &rest[2..];
        let end = digits
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(digits.len());
        let bytes = binary(&rest[..2 + end])?;
        return Some((Literal::Bytes(bytes), &digits[end..]));
    }
    let quoted = rest.strip_prefix('N').unwrap_or(rest);
    let (value, after) = Delimiter::STRING.unquote_prefix(quoted).ok()?;
    Some((Literal::Text(value), after))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Literal {
        Literal::Text(value.into())
    }

    #[test]
    fn bytes_round_trip_through_the_binary_literal() {
        let all: Vec<u8> = (0..=255).collect();
        let literal = TSQL.quote_bytes(&all);
        assert!(literal.starts_with("0x000102"));
        assert_eq!(TSQL.bytes_of(&literal), Some(all));
        assert_eq!(TSQL.quote_bytes(b""), "0x");
        assert_eq!(TSQL.bytes_of("0x"), Some(Vec::new()));
        assert_eq!(TSQL.bytes_of("0XAB"), Some(vec![0xab]));
        assert_eq!(TSQL.bytes_of("plain"), None);
        assert_eq!(TSQL.bytes_of("0xabc"), None, "an odd digit count");
        assert_eq!(TSQL.bytes_of("0xzz"), None, "not hex");
    }

    #[test]
    fn a_literal_is_unicode_and_doubles_its_quote() {
        assert_eq!(TSQL.quote_text("it's"), "N'it''s'");
        assert_eq!(TSQL.quote_text("back\\slash"), "N'back\\slash'");
        assert_eq!(TSQL.quote_text(""), "N''");
        assert_eq!(TSQL.quote_identifier("in]box"), "[in]]box]");
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| TSQL.parse_insert_literal(sql);
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES (N'it''s here');"),
            Some(("inbox".into(), "payload".into(), text("it's here")))
        );
        assert_eq!(
            parse("insert into [In ]]box] ( [Payload] ) values ( '' )"),
            Some(("In ]box".into(), "Payload".into(), text("")))
        );
        assert_eq!(
            parse("INSERT INTO dbo.inbox (payload) VALUES (0xFFfe)"),
            Some((
                "dbo.inbox".into(),
                "payload".into(),
                Literal::Bytes(vec![0xff, 0xfe])
            ))
        );
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES (0x)"),
            Some(("inbox".into(), "payload".into(), Literal::Bytes(Vec::new())))
        );
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES ('open").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES (0xabc)").is_none());
        assert!(parse("INSERT INTO [open (a) VALUES ('x')").is_none());
        assert!(parse("UPDATE inbox SET a = 'x'").is_none());
    }
}
