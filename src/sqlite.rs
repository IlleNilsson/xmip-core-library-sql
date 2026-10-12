//! `SQLite` 3: literals as ISO writes them ([`crate::STANDARD`]), a BLOB
//! answered as its hex digits through `hex(…)`, and a new row's id through
//! `RETURNING`.

use codec::hex;

use crate::delimiter::Delimiter;
use crate::dialect::{Dialect, Returning};
use crate::standard;

/// `SQLite`.
pub const SQLITE: Dialect = Dialect {
    identifier: Delimiter::IDENTIFIER,
    bare: &['_', '.'],
    marker: "?",
    text: standard::text,
    bytes: standard::bytes,
    literal: standard::literal,
    binary_column,
    binary,
    returning: Returning::Returning,
};

/// `hex(<column>)`: a BLOB spelled out as its digits.
fn binary_column(column: &str) -> String {
    format!("hex({column})")
}

/// `hex(…)` answered as its digits.
fn binary(text: &str) -> Option<Vec<u8>> {
    hex::decode(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blob_comes_back_as_its_digits() {
        assert_eq!(SQLITE.binary_column("bytes"), "hex(bytes)");
        assert_eq!(SQLITE.bytes_of("6BFF"), Some(vec![0x6b, 0xff]));
        assert_eq!(SQLITE.bytes_of("X'6bff'"), None);
        assert_eq!(SQLITE.quote_bytes(&[0x6b]), "X'6b'");
        assert_eq!(SQLITE.quote_text("it's"), "'it''s'");
        assert_eq!(SQLITE.returning(), Returning::Returning);
    }
}
