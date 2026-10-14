//! IBM DB2: `"…"` identifiers, their case kept — DB2 folds a bare name to
//! upper case, so a quoted one names a table as the catalog holds it,
//! `INBOX` for a table created as `inbox`; `'…'` literals with the quote
//! doubled and nothing else escaped; bytes as the binary string constant
//! `BX'…'`, a VARBINARY, which `X'…'` is not (`X'…'` is a character
//! string); `?` for a parameter; a new row's id asked for after the
//! INSERT.
//!
//! A constant holds at most 32 672 bytes between its delimiters, and a
//! binary one at most 32 672 hex digits (Db2 11.5 SQL reference,
//! *Constants*). A longer value is written as its pieces concatenated —
//! `CLOB('…') || CLOB('…')`, `BLOB(BX'…') || BLOB(BX'…')` — a large object
//! the column takes whole, up to the 2 097 152 bytes a statement may hold.
//! A binary column comes back through `HEX(<column>)`: two digits a byte,
//! nothing around them, which is DB2's binary form here.

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::literal::Literal;
use crate::standard;

/// IBM DB2.
pub const DB2: Dialect = Dialect {
    identifier: Delimiter::IDENTIFIER,
    bare: &['_', '.'],
    marker: "?",
    text,
    bytes,
    literal,
    binary_column,
    binary,
    returning: Returning::Separately,
};

/// The most bytes a constant holds between its delimiters, and the most
/// hex digits a binary one holds.
const MOST: usize = 32_672;

/// `text` as one string literal, or as its pieces concatenated where one
/// literal would hold more than [`MOST`] bytes.
fn text(text: &str) -> String {
    let quoted = standard::text(text);
    if quoted.len() - 2 <= MOST {
        return quoted;
    }
    pieces(text)
        .into_iter()
        .map(|piece| format!("CLOB({})", standard::text(piece)))
        .collect::<Vec<_>>()
        .join(" || ")
}

/// `text` cut at character boundaries into pieces whose literals hold at
/// most [`MOST`] bytes each, a doubled quote counted twice.
fn pieces(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut start, mut weight) = (0, 0);
    for (at, c) in text.char_indices() {
        let width = c.len_utf8() + usize::from(c == '\'');
        if weight + width > MOST {
            pieces.push(&text[start..at]);
            (start, weight) = (at, 0);
        }
        weight += width;
    }
    pieces.push(&text[start..]);
    pieces
}

/// `bytes` as one binary string constant, or as its pieces concatenated
/// as large objects where one constant would hold more than [`MOST`]
/// digits.
fn bytes(bytes: &[u8]) -> String {
    if bytes.len() * 2 <= MOST {
        return format!("BX'{}'", hex::encode(bytes));
    }
    bytes
        .chunks(MOST / 2)
        .map(|piece| format!("BLOB(BX'{}')", hex::encode(piece)))
        .collect::<Vec<_>>()
        .join(" || ")
}

/// `HEX(<column>)`: a binary column spelled out, two digits a byte.
fn binary_column(column: &str) -> String {
    format!("HEX({column})")
}

/// The bytes `text` spells as hex digits, or `None` where it is not
/// whole bytes of them.
fn binary(text: &str) -> Option<Vec<u8>> {
    hex::decode(text).ok()
}

/// One literal — a string, a binary string constant, or their pieces
/// concatenated as [`text`] and [`bytes`] write them — and what follows.
fn literal(rest: &str) -> Option<(Literal, &str)> {
    let (mut value, mut after) = piece(rest.trim_start())?;
    while let Some(next) = after.trim_start().strip_prefix("||") {
        let (more, tail) = piece(next.trim_start())?;
        value = match (value, more) {
            (Literal::Text(mut text), Literal::Text(more)) => {
                text.push_str(&more);
                Literal::Text(text)
            }
            (Literal::Bytes(mut bytes), Literal::Bytes(more)) => {
                bytes.extend(more);
                Literal::Bytes(bytes)
            }
            _ => return None,
        };
        after = tail;
    }
    Some((value, after))
}

/// One piece: `'…'`, `BX'…'`, or either inside `CLOB(…)` or `BLOB(…)`.
fn piece(rest: &str) -> Option<(Literal, &str)> {
    for function in ["CLOB(", "BLOB("] {
        if rest
            .get(..function.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(function))
        {
            let (inner, after) = piece(rest[function.len()..].trim_start())?;
            return Some((inner, after.trim_start().strip_prefix(')')?));
        }
    }
    if let Some(quoted) = rest
        .strip_prefix("BX'")
        .or_else(|| rest.strip_prefix("bx'"))
    {
        let end = quoted.find('\'')?;
        let bytes = hex::decode(&quoted[..end]).ok()?;
        return Some((Literal::Bytes(bytes), &quoted[end + 1..]));
    }
    let (text, after) = Delimiter::STRING.unquote_prefix(rest).ok()?;
    Some((Literal::Text(text), after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_value_is_one_constant_and_reads_back() {
        assert_eq!(DB2.quote_text("it's"), "'it''s'");
        assert_eq!(DB2.quote_text("back\\slash"), "'back\\slash'");
        assert_eq!(DB2.quote_bytes(&[0x6b, 0xff]), "BX'6bff'");
        assert_eq!(DB2.quote_bytes(b""), "BX''");
        assert_eq!(DB2.quote_identifier("in\"box"), "\"in\"\"box\"");
        assert_eq!(DB2.binary_column("PAYLOAD"), "HEX(PAYLOAD)");
        assert_eq!(DB2.bytes_of("6BFF"), Some(vec![0x6b, 0xff]));
        assert_eq!(DB2.bytes_of(""), Some(Vec::new()));
        assert_eq!(DB2.bytes_of("6BF"), None, "an odd digit");
        assert_eq!(DB2.bytes_of("BX'6BFF'"), None, "only the digits");
        assert_eq!(
            DB2.read_literal("BX'6bff', rest"),
            Some((Literal::Bytes(vec![0x6b, 0xff]), ", rest"))
        );
        assert_eq!(DB2.marker(), "?");
        assert_eq!(DB2.returning(), Returning::Separately);
    }

    #[test]
    fn a_long_value_is_written_in_pieces_and_read_back_whole() {
        let all: Vec<u8> = (0..=255).cycle().take(MOST + 7).collect();
        let written = DB2.quote_bytes(&all);
        assert_eq!(written.matches("BLOB(BX'").count(), 3);
        assert!(written.split(" || ").all(|piece| piece.len() <= MOST + 10));
        assert_eq!(DB2.read_literal(&written), Some((Literal::Bytes(all), "")));
        let long = "a'é".repeat(MOST / 3);
        let written = DB2.quote_text(&long);
        assert!(written.starts_with("CLOB('a''é"));
        for piece in written.split(" || ") {
            let inner = &piece["CLOB('".len()..piece.len() - "')".len()];
            assert!(inner.len() <= MOST, "{}", inner.len());
        }
        assert_eq!(DB2.read_literal(&written), Some((Literal::Text(long), "")));
        assert_eq!(DB2.read_literal("'a' || BX'00'"), None, "text and bytes");
        assert_eq!(DB2.read_literal("CLOB('a'"), None, "never closed");
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| DB2.parse_insert_literal(sql);
        let sql = DB2.insert("INBOX", "PAYLOAD", &DB2.quote_bytes(&[1, 2]));
        assert_eq!(sql, "INSERT INTO \"INBOX\" (\"PAYLOAD\") VALUES (BX'0102')");
        assert_eq!(
            parse(&sql),
            Some(("INBOX".into(), "PAYLOAD".into(), Literal::Bytes(vec![1, 2])))
        );
        let long = "x".repeat(MOST + 1);
        let sql = DB2.insert("INBOX", "BODY", &DB2.quote_text(&long));
        assert_eq!(
            parse(&sql),
            Some(("INBOX".into(), "BODY".into(), Literal::Text(long)))
        );
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
    }
}
