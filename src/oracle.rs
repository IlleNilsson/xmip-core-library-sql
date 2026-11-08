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
//!
//! Two more literals are read, never written: the national `N'…'`, quoted
//! as `'…'` is, and the alternative quoting `q'[…]'`, `nq'[…]'` in any case,
//! taken as written up to its closing delimiter and a quote — `]`, `}`,
//! `>` or `)` for an opening bracket, otherwise the opening character
//! itself (Oracle Database SQL Language Reference, *Text Literals*). Read
//! as a plain literal, `q'[it's :1]'` ended after `it` and the `:1` in its
//! text was bound.

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
    line_comments: &["--"],
    text: standard::text,
    bytes,
    literal,
    opens,
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

/// Whether `rest` opens a literal: `'`, `N'`, or `q'` and `nq'` in any
/// case.
fn opens(rest: &str) -> bool {
    standard::opens(rest) || national(rest).is_some() || alternative(rest).is_some()
}

/// One literal — `HEXTORAW('…')`, `q'[…]'` or `nq'[…]'`, `N'…'` or `'…'`,
/// each prefix in any case — and what follows it.
fn literal(rest: &str) -> Option<(Literal, &str)> {
    let head = rest.get(..HEXTORAW.len());
    if head.is_some_and(|head| head.eq_ignore_ascii_case(HEXTORAW)) {
        let inner = rest[HEXTORAW.len()..].trim_start();
        let (digits, after) = Delimiter::STRING.unquote_prefix(inner).ok()?;
        let after = after.trim_start().strip_prefix(')')?;
        return Some((Literal::Bytes(binary(&digits)?), after));
    }
    if let Some(quoted) = alternative(rest) {
        let (text, after) = alternatively(quoted)?;
        return Some((Literal::Text(text), after));
    }
    let quoted = national(rest).unwrap_or(rest);
    let (text, after) = Delimiter::STRING.unquote_prefix(quoted).ok()?;
    Some((Literal::Text(text), after))
}

/// The `'…'` a national literal `N'…'` opens with; `None` where `rest`
/// opens with none.
fn national(rest: &str) -> Option<&str> {
    rest.strip_prefix(['N', 'n'])
        .filter(|quoted| quoted.starts_with('\''))
}

/// What follows the `q'` or `nq'`, in any case, `rest` opens with: the
/// opening delimiter and on; `None` where `rest` opens with neither.
fn alternative(rest: &str) -> Option<&str> {
    let rest = rest.strip_prefix(['N', 'n']).unwrap_or(rest);
    rest.strip_prefix(['Q', 'q'])?.strip_prefix('\'')
}

/// The text between an alternative quote's delimiters, and what follows
/// its closing quote; `None` where the delimiter is a space, a tab, a line
/// end or a quote, or the literal never closes.
fn alternatively(quoted: &str) -> Option<(String, &str)> {
    let open = quoted.chars().next()?;
    if matches!(open, ' ' | '\t' | '\r' | '\n' | '\'') {
        return None;
    }
    let close = match open {
        '[' => ']',
        '{' => '}',
        '<' => '>',
        '(' => ')',
        other => other,
    };
    let inner = &quoted[open.len_utf8()..];
    let (end, _) = inner
        .match_indices(close)
        .find(|&(at, _)| inner[at + close.len_utf8()..].starts_with('\''))?;
    let after = &inner[end + close.len_utf8() + 1..];
    Some((inner[..end].to_string(), after))
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
    fn an_alternative_quote_is_read_to_its_closing_delimiter_and_quote() {
        let read = |sql| ORACLE.read_literal(sql);
        let text = |value: &str| Literal::Text(value.into());
        assert_eq!(read("q'[it's :1]' rest"), Some((text("it's :1"), " rest")));
        assert_eq!(read("Q'{a]'b}'"), Some((text("a]'b"), "")));
        assert_eq!(read("nq'<x>'"), Some((text("x"), "")));
        assert_eq!(read("NQ'(x)'"), Some((text("x"), "")));
        assert_eq!(read("q'!it's!'"), Some((text("it's"), "")));
        assert_eq!(read("q'#a#b#'"), Some((text("a#b"), "")));
        assert_eq!(read("N'it''s'"), Some((text("it's"), "")));
        assert_eq!(read("q'[open'"), None, "never closed");
        assert_eq!(read("q' x '"), None, "a space delimits nothing");
    }

    #[test]
    fn a_parameter_inside_an_alternative_quote_stays() {
        // Read as a plain literal, `q'[it's :1]'` ended after `it` and the
        // `:1` in its text was bound.
        assert_eq!(
            ORACLE.bound("UPDATE t SET note = q'[it's :1]' WHERE id = :1", "'7'"),
            "UPDATE t SET note = q'[it's :1]' WHERE id = '7'"
        );
        assert_eq!(
            ORACLE.bound("UPDATE t SET note = Nq'{:1}' WHERE id = :1", "'7'"),
            "UPDATE t SET note = Nq'{:1}' WHERE id = '7'"
        );
        let open = "DELETE FROM t WHERE note = q'[open' AND id = :1";
        assert_eq!(ORACLE.bound(open, "'7'"), open);
        assert_eq!(
            ORACLE.bound("SELECT a FROM t WHERE b IN'x' AND id = :1", "'7'"),
            "SELECT a FROM t WHERE b IN'x' AND id = '7'",
            "the N that ends a word opens no national literal"
        );
    }

    #[test]
    fn an_insert_of_one_column_is_taken_apart() {
        let parse = |sql| ORACLE.parse_insert_literal(sql);
        assert_eq!(
            parse("INSERT INTO inbox (payload) VALUES (q'[it's]')"),
            Some((
                "inbox".into(),
                "payload".into(),
                Literal::Text("it's".into())
            ))
        );
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
