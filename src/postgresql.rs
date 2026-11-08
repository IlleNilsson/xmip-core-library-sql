//! `PostgreSQL`: `"…"` identifiers, their case kept; `'…'` literals with
//! the quote doubled and backslashes literal, as
//! `standard_conforming_strings` has had them since 9.1; bytes in the bytea
//! hex form, `\x` and two digits a byte, which a bytea column reads as the
//! bytes and answers under the default `bytea_output`; `$1` for a
//! parameter; a new row's id through `RETURNING`.
//!
//! Two more literals are read, never written: the escape string `E'…'`,
//! whose backslash escapes are undone and in which `\'` is a quote, and the
//! dollar-quoted string `$$…$$` or `$tag$…$tag$`, taken as written up to
//! its closing tag (`PostgreSQL` documentation, *Lexical Structure*,
//! 4.1.2). Read as plain literals, `E'it\'s $1'` ended after the backslash
//! and `$$ $1 $$` was no literal at all, so a parameter inside either was
//! bound. An escape string that names bytes that are not UTF-8, or a NUL,
//! is refused, as the server refuses it.

use std::iter::Peekable;
use std::str::CharIndices;

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
    line_comments: &["--"],
    text: standard::text,
    bytes,
    literal,
    opens,
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

/// Whether `rest` opens a literal: `'`, `E'` or `e'`, or a dollar quote's
/// opening tag.
fn opens(rest: &str) -> bool {
    standard::opens(rest) || escape_string(rest).is_some() || dollar_tag(rest).is_some()
}

/// One literal — `'…'`, `E'…'` or `$tag$…$tag$` — and what follows it: the
/// literal is text, and a bytea value in it is read by [`Dialect::bytes_of`].
fn literal(rest: &str) -> Option<(Literal, &str)> {
    if let Some(quoted) = escape_string(rest) {
        let (text, after) = escaped(quoted)?;
        return Some((Literal::Text(text), after));
    }
    if let Some(tag) = dollar_tag(rest) {
        let body = &rest[tag.len()..];
        let end = body.find(tag)?;
        let after = &body[end + tag.len()..];
        return Some((Literal::Text(body[..end].to_string()), after));
    }
    let (text, after) = Delimiter::STRING.unquote_prefix(rest).ok()?;
    Some((Literal::Text(text), after))
}

/// The escape string `rest` opens with, from its opening quote; `None`
/// where `rest` opens with no `E'` or `e'`.
fn escape_string(rest: &str) -> Option<&str> {
    rest.strip_prefix(['E', 'e'])
        .filter(|quoted| quoted.starts_with('\''))
}

/// The tag a dollar-quoted string opens with — `$$`, or `$` and a name of
/// letters, digits and underscores not starting with a digit, then `$` —
/// or `None` where `rest` opens with none; `$1` is a parameter.
fn dollar_tag(rest: &str) -> Option<&str> {
    let name = rest.strip_prefix('$')?;
    let end = name.find('$')?;
    let mut chars = name[..end].chars();
    let named = chars.next().is_none_or(|first| {
        (first.is_alphabetic() || first == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
    });
    named.then(|| &rest[..end + 2])
}

/// An escape string's text from its opening quote, and what follows it: a
/// quote doubled or after a backslash is one, and `\b \f \n \r \t`, octal
/// `\o` to `\ooo`, hex `\xh` and `\xhh`, `\uXXXX` and `\UXXXXXXXX` are
/// undone; any other character after a backslash is itself. `None` where it
/// never closes, a Unicode escape is malformed, or the bytes are not UTF-8
/// without a NUL.
fn escaped(quoted: &str) -> Option<(String, &str)> {
    let body = quoted.strip_prefix('\'')?;
    let mut value = Vec::new();
    let mut chars = body.char_indices().peekable();
    loop {
        let (at, c) = chars.next()?;
        match c {
            '\'' if chars.next_if(|&(_, next)| next == '\'').is_some() => value.push(b'\''),
            '\'' => {
                let text = String::from_utf8(value).ok()?;
                return (!text.contains('\0')).then(|| (text, &body[at + 1..]));
            }
            '\\' => unescape(&mut chars, &mut value)?,
            other => push(&mut value, other),
        }
    }
}

/// What a backslash and the characters after it name, onto `value`.
fn unescape(chars: &mut Peekable<CharIndices<'_>>, value: &mut Vec<u8>) -> Option<()> {
    let (_, escaped) = chars.next()?;
    match escaped {
        'b' => value.push(0x08),
        'f' => value.push(0x0c),
        'n' => value.push(b'\n'),
        'r' => value.push(b'\r'),
        't' => value.push(b'\t'),
        '0'..='7' => {
            let rest = digits(chars, 8, 2).unwrap_or((0, 0));
            let high = escaped.to_digit(8)?;
            // The server keeps the low byte of `\ooo`, as it does.
            let octal = (high << (3 * rest.1)) | rest.0;
            value.push(u8::try_from(octal & 0xff).ok()?);
        }
        'x' => match digits(chars, 16, 2) {
            Some((byte, _)) => value.push(u8::try_from(byte).ok()?),
            None => value.push(b'x'),
        },
        'u' | 'U' => {
            let wide = if escaped == 'u' { 4 } else { 8 };
            let point = exactly(chars, wide)?;
            let point = match point {
                0xd800..=0xdbff => {
                    let low = chars
                        .next()
                        .filter(|&(_, c)| c == '\\')
                        .and_then(|_| chars.next().filter(|&(_, c)| c == 'u'))
                        .and_then(|_| exactly(chars, 4))?;
                    let low = low.checked_sub(0xdc00).filter(|low| *low < 0x400)?;
                    0x10000 + ((point - 0xd800) << 10) + low
                }
                point => point,
            };
            push(value, char::from_u32(point)?);
        }
        other => push(value, other),
    }
    Some(())
}

/// Up to `most` digits in `radix`, and how many were read; `None` where
/// none were.
fn digits(chars: &mut Peekable<CharIndices<'_>>, radix: u32, most: u32) -> Option<(u32, u32)> {
    let mut read = (0, 0);
    while read.1 < most {
        let Some((_, digit)) = chars.next_if(|&(_, c)| c.is_digit(radix)) else {
            break;
        };
        read = (read.0 * radix + digit.to_digit(radix)?, read.1 + 1);
    }
    (read.1 > 0).then_some(read)
}

/// Exactly `wide` hex digits, or `None`.
fn exactly(chars: &mut Peekable<CharIndices<'_>>, wide: u32) -> Option<u32> {
    digits(chars, 16, wide)
        .filter(|&(_, read)| read == wide)
        .map(|(value, _)| value)
}

/// `c` as UTF-8 onto `value`.
fn push(value: &mut Vec<u8>, c: char) {
    value.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Literal {
        Literal::Text(value.into())
    }

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
    fn an_escape_string_undoes_its_escapes_and_a_quoted_quote_does_not_end_it() {
        let read = |sql| POSTGRESQL.read_literal(sql);
        assert_eq!(read("E'it\\'s' rest"), Some((text("it's"), " rest")));
        assert_eq!(read("e'it''s'"), Some((text("it's"), "")));
        assert_eq!(
            read("E'a\\nb\\t\\\\c\\q\\101\\x42\\x4g\\u00e9\\U0001F600'"),
            Some((text("a\nb\t\\cqAB\u{4}g\u{e9}\u{1f600}"), ""))
        );
        assert_eq!(read("E'\\ud83d\\ude00'"), Some((text("\u{1f600}"), "")));
        assert_eq!(read("E'\\xg'"), Some((text("xg"), "")), "no digit: an x");
        assert_eq!(read("E'\\u12'"), None, "a short Unicode escape");
        assert_eq!(read("E'\\ud83d'"), None, "half a surrogate pair");
        assert_eq!(read("E'\\xff'"), None, "not UTF-8");
        assert_eq!(read("E'\\0'"), None, "a NUL");
        assert_eq!(read("E'open\\'"), None, "never closed");
    }

    #[test]
    fn a_dollar_quoted_string_is_taken_as_written_to_its_closing_tag() {
        let read = |sql| POSTGRESQL.read_literal(sql);
        assert_eq!(
            read("$$it's \\ $1$$ rest"),
            Some((text("it's \\ $1"), " rest"))
        );
        assert_eq!(read("$body$ $$ $b$ $body$;"), Some((text(" $$ $b$ "), ";")));
        assert_eq!(read("$$open"), None);
        assert_eq!(read("$1"), None, "a parameter");
        assert_eq!(read("$1a$ x $1a$"), None, "a tag never starts with a digit");
    }

    #[test]
    fn a_parameter_inside_an_escape_or_dollar_quoted_string_stays() {
        // Read as plain literals, `E'it\'s $1'` ended after the backslash
        // and the `$1` in its text was bound; a dollar quote was none.
        assert_eq!(
            POSTGRESQL.bound("UPDATE t SET note = E'it\\'s $1' WHERE id = $1", "'7'"),
            "UPDATE t SET note = E'it\\'s $1' WHERE id = '7'"
        );
        assert_eq!(
            POSTGRESQL.bound("UPDATE t SET note = $$it's $1$$ WHERE id = $1", "'7'"),
            "UPDATE t SET note = $$it's $1$$ WHERE id = '7'"
        );
        assert_eq!(
            POSTGRESQL.bound("UPDATE t SET note = $x$ $1 $x$, n = $1", "'7'"),
            "UPDATE t SET note = $x$ $1 $x$, n = '7'"
        );
        for statement in [
            "DELETE FROM t WHERE note = E'open\\' AND id = $1",
            "DELETE FROM t WHERE note = $$open AND id = $1",
            "DELETE FROM t WHERE note = E'\\0' AND id = $1",
        ] {
            assert_eq!(POSTGRESQL.bound(statement, "'7'"), statement);
        }
        assert_eq!(
            POSTGRESQL.bound("SELECT * FROM t WHERE'x' = note AND id = $1", "'7'"),
            "SELECT * FROM t WHERE'x' = note AND id = '7'",
            "the E that ends a word opens no escape string"
        );
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| POSTGRESQL.parse_insert_literal(sql);
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
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES (E'it\\'s')"),
            Some(("inbox".into(), "payload".into(), text("it's")))
        );
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES ($q$a'b$q$)"),
            Some(("inbox".into(), "payload".into(), text("a'b")))
        );
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
        assert!(parse("INSERT INTO inbox (a) VALUES ('open").is_none());
        assert!(parse("UPDATE inbox SET a = 'x'").is_none());
    }
}
