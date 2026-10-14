//! Oracle Database: `"…"` identifiers, their case kept, or bare with
//! `_ . $ #` in them — a bare one is stored upper case, so a target names
//! a table created unquoted as `INBOX`; Oracle refuses a `"` inside a
//! quoted identifier, so the doubled one this crate writes is refused by
//! the server rather than read as anything else. `'…'` literals with the
//! quote doubled and backslashes literal; an empty one is NULL, as Oracle
//! has it. Bytes as `HEXTORAW('…')`, which a RAW or BLOB column stores as
//! the bytes; a SELECT spells a RAW out with `RAWTOHEX(…)` and the answer
//! is bare upper-case hex. `:1` for a parameter; an INSERT answers its
//! new row's id only into a bind, so the id is asked for after it.

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::literal::Literal;
use crate::standard;

/// Oracle Database.
pub const ORACLE: Dialect = Dialect {
    identifier: Delimiter::IDENTIFIER,
    bare: &['_', '.', '$', '#'],
    marker: ":1",
    text: standard::text,
    bytes,
    literal,
    binary_column,
    binary,
    returning: Returning::Separately,
};

/// The function a binary literal is written in.
const HEXTORAW: &str = "HEXTORAW(";

/// `bytes` as `HEXTORAW('…')`, two upper-case digits a byte.
fn bytes(bytes: &[u8]) -> String {
    format!("{HEXTORAW}'{}')", hex::encode(bytes).to_ascii_uppercase())
}

/// `RAWTOHEX(<column>)`: a RAW spelled out as hex text.
fn binary_column(column: &str) -> String {
    format!("RAWTOHEX({column})")
}

/// The bytes bare hex digits name — what `RAWTOHEX` answers — or `None`
/// where `text` is not whole bytes of hex.
fn binary(text: &str) -> Option<Vec<u8>> {
    hex::decode(text).ok()
}

/// One literal — `HEXTORAW('…')`, in any case, or `'…'` — and what
/// follows it.
fn literal(rest: &str) -> Option<(Literal, &str)> {
    let head = rest.get(..HEXTORAW.len());
    if head.is_some_and(|head| head.eq_ignore_ascii_case(HEXTORAW)) {
        let inner = rest[HEXTORAW.len()..].trim_start();
        let (digits, after) = Delimiter::STRING.unquote_prefix(inner).ok()?;
        let after = after.trim_start().strip_prefix(')')?;
        return Some((Literal::Bytes(binary(&digits)?), after));
    }
    let (text, after) = Delimiter::STRING.unquote_prefix(rest).ok()?;
    Some((Literal::Text(text), after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_through_hextoraw_and_rawtohex() {
        let all: Vec<u8> = (0..=255).collect();
        let literal = ORACLE.quote_bytes(&all);
        assert!(literal.starts_with("HEXTORAW('000102"), "{literal}");
        assert!(literal.ends_with("FEFF')"), "{literal}");
        let Some((Literal::Bytes(read), "")) = ORACLE.read_literal(&literal) else {
            panic!("a binary literal");
        };
        assert_eq!(read, all);
        assert_eq!(ORACLE.quote_bytes(b""), "HEXTORAW('')");
        assert_eq!(ORACLE.bytes_of("FFfe"), Some(vec![0xff, 0xfe]));
        assert_eq!(ORACLE.bytes_of(""), Some(Vec::new()));
        assert_eq!(ORACLE.bytes_of("ABC"), None, "an odd digit count");
        assert_eq!(ORACLE.bytes_of("0xAB"), None, "RAWTOHEX answers bare hex");
        assert_eq!(ORACLE.binary_column("payload"), "RAWTOHEX(payload)");
    }

    #[test]
    fn quoting_doubles_the_delimiter_and_nothing_else() {
        assert_eq!(ORACLE.quote_text("it's"), "'it''s'");
        assert_eq!(ORACLE.quote_text("back\\slash"), "'back\\slash'");
        assert_eq!(ORACLE.quote_identifier("INBOX"), "\"INBOX\"");
        assert_eq!(ORACLE.quote_table("XMIP.INBOX"), "\"XMIP\".\"INBOX\"");
        assert_eq!(ORACLE.marker(), ":1");
        assert_eq!(
            ORACLE.bound("DELETE FROM inbox WHERE id = :1 AND n <> :10", "'41'"),
            "DELETE FROM inbox WHERE id = '41' AND n <> :10"
        );
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| ORACLE.parse_insert_literal(sql);
        let text = |value: &str| Literal::Text(value.into());
        assert_eq!(
            parse("INSERT INTO \"INBOX\" (\"PAYLOAD\") VALUES ('it''s here')"),
            Some(("INBOX".into(), "PAYLOAD".into(), text("it's here")))
        );
        assert_eq!(
            parse("insert into xmip.in$box# (payload) values (hextoraw( 'FF00' ))"),
            Some((
                "xmip.in$box#".into(),
                "payload".into(),
                Literal::Bytes(vec![0xff, 0])
            ))
        );
        assert!(parse("INSERT INTO inbox (payload) VALUES (HEXTORAW('ABC'))").is_none());
        assert!(parse("INSERT INTO inbox (payload) VALUES (:1)").is_none());
        assert!(parse("INSERT INTO inbox (a, b) VALUES ('x', 'y')").is_none());
    }
}
