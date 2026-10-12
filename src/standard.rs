//! ISO SQL (ISO/IEC 9075) as Xmip writes it where no server is named: a
//! script any database loads. `"…"` identifiers, `'…'` literals with the
//! quote doubled, bytes as the hex string literal `X'…'`, and `?` for a
//! parameter. `SQLite` writes its literals the same way ([`crate::SQLITE`]).

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::literal::Literal;

/// ISO SQL.
pub const STANDARD: Dialect = Dialect {
    identifier: Delimiter::IDENTIFIER,
    bare: &['_', '.'],
    marker: "?",
    text,
    bytes,
    literal,
    binary_column: str::to_string,
    binary,
    returning: Returning::Separately,
};

/// `text` between single quotes, every quote doubled.
pub(crate) fn text(text: &str) -> String {
    Delimiter::STRING.quote(text)
}

/// `bytes` as the hex string literal: `X'` then two lower-case digits a
/// byte, then `'`.
pub(crate) fn bytes(bytes: &[u8]) -> String {
    format!("X'{}'", hex::encode(bytes))
}

/// The bytes the hex string literal `X'…'` names, or `None` where `text`
/// is not one.
fn binary(text: &str) -> Option<Vec<u8>> {
    let (Literal::Bytes(bytes), "") = hex_literal(text)? else {
        return None;
    };
    Some(bytes)
}

/// One literal — `X'…'` or `'…'` — and what follows it.
pub(crate) fn literal(rest: &str) -> Option<(Literal, &str)> {
    if let Some(read) = hex_literal(rest) {
        return Some(read);
    }
    let (text, after) = Delimiter::STRING.unquote_prefix(rest).ok()?;
    Some((Literal::Text(text), after))
}

/// A hex string literal, `X'…'` or `x'…'`, and what follows it; `None`
/// where `rest` opens with none, or its digits are not whole bytes.
fn hex_literal(rest: &str) -> Option<(Literal, &str)> {
    let quoted = rest
        .strip_prefix("X'")
        .or_else(|| rest.strip_prefix("x'"))?;
    let end = quoted.find('\'')?;
    let bytes = hex::decode(&quoted[..end]).ok()?;
    Some((Literal::Bytes(bytes), &quoted[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_is_written_and_read_back_as_iso_has_it() {
        assert_eq!(STANDARD.quote_text("it's"), "'it''s'");
        assert_eq!(STANDARD.quote_bytes(&[0x6b, 0xff]), "X'6bff'");
        assert_eq!(STANDARD.quote_bytes(b""), "X''");
        assert_eq!(
            STANDARD.read_literal("'it''s', rest"),
            Some((Literal::Text("it's".into()), ", rest"))
        );
        assert_eq!(
            STANDARD.read_literal("X'6bff', rest"),
            Some((Literal::Bytes(vec![0x6b, 0xff]), ", rest"))
        );
        assert_eq!(STANDARD.bytes_of("X'6bff'"), Some(vec![0x6b, 0xff]));
        assert_eq!(STANDARD.bytes_of("X'6bff' "), None);
        assert_eq!(STANDARD.read_literal("X'abc'"), None, "an odd digit");
        assert_eq!(STANDARD.read_literal("X'ab"), None, "never closed");
        assert_eq!(STANDARD.read_literal("'open"), None);
        assert_eq!(STANDARD.binary_column("bytes"), "bytes");
        assert_eq!(STANDARD.quote_identifier("a\"b"), "\"a\"\"b\"");
    }
}
